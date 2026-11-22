//! Multi-stream demultiplexing for interleaved Ogg containers (such as `.ogv`
//! files carrying both Theora video and Vorbis audio).

use crate::framing::{PacketOut, Page, PageOut, StreamState, SyncState};
use alloc::vec::Vec;

/// Default cap on concurrently tracked logical streams in [`OggDemuxer`].
const DEFAULT_MAX_STREAMS: usize = 256;

/// A multi-stream Ogg demultiplexer that feeds bytes through a [`SyncState`]
/// and routes pages by serial number to dynamically tracked [`StreamState`]s.
///
/// Each call to [`next_packet`](Self::next_packet) yields `(serialno,
/// PacketOut)` for whichever logical stream next produces a complete packet or
/// reports a gap (hole), or `None` when more container bytes are required.
#[derive(Clone, Debug)]
pub struct OggDemuxer {
    sync: SyncState,
    streams: Vec<StreamState>,
    current_stream: Option<usize>,
    max_streams: usize,
    eof: bool,
}

impl Default for OggDemuxer {
    fn default() -> Self {
        Self::new()
    }
}

impl OggDemuxer {
    /// Creates a new multi-stream demuxer tracking up to 256 concurrent logical
    /// streams.
    #[must_use]
    pub fn new() -> Self {
        Self::with_max_streams(DEFAULT_MAX_STREAMS)
    }

    /// Creates a new multi-stream demuxer tracking at most `max_streams`
    /// concurrent logical streams. Pages belonging to new serial numbers beyond
    /// `max_streams` are ignored.
    #[must_use]
    pub fn with_max_streams(max_streams: usize) -> Self {
        Self {
            sync: SyncState::new(),
            streams: Vec::new(),
            current_stream: None,
            max_streams,
            eof: false,
        }
    }

    /// Appends raw container bytes to the internal [`SyncState`]. Returns `0` on
    /// success, or `-1` if `bytes` exceeds the sync buffer limit.
    pub fn write(&mut self, bytes: &[u8]) -> i32 {
        self.eof = false;
        self.sync.write(bytes)
    }

    /// Exposes a mutable slice of at least `size` bytes at the end of the
    /// internal [`SyncState`] buffer, to be followed by [`wrote`](Self::wrote).
    #[must_use]
    pub fn buffer(&mut self, size: usize) -> Option<&mut [u8]> {
        self.sync.buffer(size)
    }

    /// Commits `bytes` bytes written into [`buffer`](Self::buffer). Returns `0`
    /// on success, or `-1` if `bytes` exceeds the available room.
    pub fn wrote(&mut self, bytes: usize) -> i32 {
        if bytes > 0 {
            self.eof = false;
        }
        self.sync.wrote(bytes)
    }

    /// Reads up to `chunk_size` bytes from `reader` into the internal
    /// [`SyncState`]. If `Ok(0)` is returned when `chunk_size > 0`, physical EOF
    /// is automatically marked so [`next_packet`](Self::next_packet) can resync
    /// past any trailing truncated `"OggS"` headers.
    ///
    /// # Errors
    ///
    /// Returns any I/O error reported by `reader`, or an error if `chunk_size`
    /// exceeds the maximum sync buffer capacity.
    #[cfg(feature = "std")]
    pub fn read_from<R: std::io::Read>(
        &mut self,
        reader: &mut R,
        chunk_size: usize,
    ) -> std::io::Result<usize> {
        let n = self.sync.read_from(reader, chunk_size)?;
        if n == 0 && chunk_size > 0 {
            self.eof = true;
        } else if n > 0 {
            self.eof = false;
        }
        Ok(n)
    }

    /// Marks that physical end-of-file has been reached so subsequent
    /// [`next_packet`](Self::next_packet) calls use [`SyncState::pageout_eof`]
    /// to resync past truncated bogus `"OggS"` headers near EOF.
    pub fn mark_eof(&mut self) {
        self.eof = true;
    }

    /// Resets the internal [`SyncState`] and all tracked [`StreamState`]s (for
    /// instance before seeking), preserving discovered stream serial numbers.
    pub fn reset(&mut self) {
        self.sync.reset();
        for stream in &mut self.streams {
            stream.reset();
        }
        self.current_stream = None;
        self.eof = false;
    }

    /// Frees all buffered data and forgets all tracked logical streams.
    pub fn clear(&mut self) {
        self.sync.clear();
        self.streams.clear();
        self.current_stream = None;
        self.eof = false;
    }

    /// Returns the number of logical streams discovered so far.
    #[must_use]
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// Returns the tracked [`StreamState`]s in discovery order.
    #[must_use]
    pub fn streams(&self) -> &[StreamState] {
        &self.streams
    }

    /// Looks up the [`StreamState`] for `serialno`, if discovered.
    #[must_use]
    pub fn stream(&self, serialno: i32) -> Option<&StreamState> {
        self.streams
            .iter()
            .find(|s| s.serialno() == i64::from(serialno))
    }

    /// Returns a shared reference to the underlying [`SyncState`].
    #[must_use]
    pub fn sync(&self) -> &SyncState {
        &self.sync
    }

    fn route_page(
        streams: &mut Vec<StreamState>,
        max_streams: usize,
        page: &Page<'_>,
    ) -> Option<usize> {
        let serialno = page.serialno();
        let idx = if let Some(idx) = streams
            .iter()
            .position(|s| s.serialno() == i64::from(serialno))
        {
            // Support chained bitstreams reusing a serial number after EOS.
            if streams[idx].eos() && page.bos() {
                streams[idx].reset();
            }
            idx
        } else {
            if streams.len() >= max_streams {
                return None;
            }
            let idx = streams.len();
            streams.push(StreamState::new(serialno));
            idx
        };

        if streams[idx].pagein(page) == 0 {
            Some(idx)
        } else {
            None
        }
    }

    /// Advances demultiplexing and returns `(serialno, PacketOut)` for the next
    /// available packet ([`PacketOut::Packet`]) or stream gap
    /// ([`PacketOut::Hole`]), or `None` when more input bytes are needed.
    #[must_use]
    pub fn next_packet(&mut self) -> Option<(i32, PacketOut<'_>)> {
        let ready_idx = loop {
            if let Some(idx) = self.current_stream {
                if self.streams[idx].has_ready_packet() {
                    break idx;
                }
                self.current_stream = None;
            }

            let page = if self.eof {
                self.sync.pageout_eof()?
            } else {
                match self.sync.next_page() {
                    PageOut::Page(page) => page,
                    PageOut::Hole => continue,
                    PageOut::NeedData => return None,
                }
            };

            if let Some(idx) = Self::route_page(&mut self.streams, self.max_streams, &page) {
                self.current_stream = Some(idx);
            }
        };

        let serialno = self.streams[ready_idx].serialno() as i32;
        let out = self.streams[ready_idx].next_packet();
        Some((serialno, out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Packet;
    use alloc::vec;

    #[test]
    fn demux_multitrack_theora_vorbis_interleaved() {
        const VIDEO_SERIAL: i32 = 0x1001;
        const AUDIO_SERIAL: i32 = 0x2002;

        let mut video = StreamState::new(VIDEO_SERIAL);
        let mut audio = StreamState::new(AUDIO_SERIAL);
        let mut file = Vec::new();

        // 1. BOS headers (Theora first, then Vorbis, per Ogg `.ogv` multiplexing rules).
        assert_eq!(video.packetin(&Packet::new(b"\x80theora-id")), 0);
        if let Some(p) = video.flush() {
            file.extend_from_slice(p.header);
            file.extend_from_slice(p.body);
        }
        assert_eq!(audio.packetin(&Packet::new(b"\x01vorbis-id")), 0);
        if let Some(p) = audio.flush() {
            file.extend_from_slice(p.header);
            file.extend_from_slice(p.body);
        }

        // 2. Large multi-page video keyframe interleaved mid-packet with audio packets!
        let large_keyframe: Vec<u8> = (0..80_000usize).map(|i| (i & 0xff) as u8).collect();
        let mut v_pkt = Packet::new(&large_keyframe);
        v_pkt.granulepos = 64;
        v_pkt.e_o_s = 1;
        assert_eq!(video.packetin(&v_pkt), 0);

        // Emit first page of the large video keyframe.
        let vp1 = video.pageout().unwrap();
        file.extend_from_slice(vp1.header);
        file.extend_from_slice(vp1.body);

        // Interleave two audio packets on one flushed page in the middle of the video packet!
        for i in 0..2u8 {
            let mut a_pkt = Packet::new(if i == 0 { b"audio-0" } else { b"audio-1" });
            a_pkt.granulepos = i64::from(i + 1) * 1024;
            a_pkt.e_o_s = i64::from(i == 1);
            assert_eq!(audio.packetin(&a_pkt), 0);
        }
        while let Some(ap) = audio.flush() {
            file.extend_from_slice(ap.header);
            file.extend_from_slice(ap.body);
        }

        // Emit remaining pages of the large video keyframe.
        while let Some(vp) = video.flush() {
            file.extend_from_slice(vp.header);
            file.extend_from_slice(vp.body);
        }

        // Demux in small arbitrary chunks!
        let mut demux = OggDemuxer::new();
        let mut video_packets = Vec::new();
        let mut audio_packets = Vec::new();

        for chunk in file.chunks(517) {
            assert_eq!(demux.write(chunk), 0);
            while let Some((serial, out)) = demux.next_packet() {
                let pkt = out.into_packet().expect("no holes in clean stream");
                if serial == VIDEO_SERIAL {
                    video_packets.push((pkt.bos(), pkt.eos(), pkt.granulepos, pkt.packet.to_vec()));
                } else if serial == AUDIO_SERIAL {
                    audio_packets.push((pkt.bos(), pkt.eos(), pkt.granulepos, pkt.packet.to_vec()));
                } else {
                    panic!("unexpected serial {serial}");
                }
            }
        }

        assert_eq!(demux.stream_count(), 2);
        assert!(demux.stream(VIDEO_SERIAL).unwrap().eos());
        assert!(demux.stream(AUDIO_SERIAL).unwrap().eos());

        assert_eq!(video_packets.len(), 2);
        assert_eq!(
            video_packets[0],
            (true, false, 0, b"\x80theora-id".to_vec())
        );
        assert_eq!(video_packets[1], (false, true, 64, large_keyframe));

        assert_eq!(audio_packets.len(), 3);
        assert_eq!(
            audio_packets[0],
            (true, false, 0, b"\x01vorbis-id".to_vec())
        );
        assert_eq!(audio_packets[1], (false, false, -1, b"audio-0".to_vec()));
        assert_eq!(audio_packets[2], (false, true, 2048, b"audio-1".to_vec()));
    }

    #[test]
    fn demux_reports_hole_on_dropped_page_and_respects_max_streams() {
        let mut s1 = StreamState::new(10);
        let mut pages = Vec::new();
        for i in 0..4u8 {
            let body = [i; 32];
            let mut p = Packet::new(&body);
            p.granulepos = i64::from(i);
            assert_eq!(s1.packetin(&p), 0);
            if let Some(page) = s1.flush() {
                pages.push([page.header, page.body].concat());
            }
        }

        let mut s2 = StreamState::new(20);
        assert_eq!(s2.packetin(&Packet::new(b"ignored-track")), 0);
        let extra_page = s2.flush().unwrap();
        let extra_bytes = [extra_page.header, extra_page.body].concat();

        let mut demux = OggDemuxer::with_max_streams(1);
        assert_eq!(demux.write(&pages[0]), 0);
        // Drop `pages[1]`!
        assert_eq!(demux.write(&extra_bytes), 0); // Exceeds `max_streams = 1`, ignored.
        assert_eq!(demux.write(&pages[2]), 0);
        assert_eq!(demux.write(&pages[3]), 0);

        let mut events = Vec::new();
        while let Some((serial, out)) = demux.next_packet() {
            assert_eq!(serial, 10);
            match out {
                PacketOut::Packet(p) => events.push(Some(p.packet[0])),
                PacketOut::Hole => events.push(None),
                PacketOut::NeedData => unreachable!(),
            }
        }
        assert_eq!(events, vec![Some(0), None, Some(2), Some(3)]);
        assert_eq!(demux.stream_count(), 1);
    }

    #[cfg(feature = "std")]
    #[test]
    fn demux_read_from_handles_eof_bogus_oggs() {
        let mut s = StreamState::new(77);
        assert_eq!(s.packetin(&Packet::new(b"bos")), 0);
        let p0 = s.flush().unwrap();
        let b0 = [p0.header, p0.body].concat();

        let mut eos = Packet::new(b"eos");
        eos.e_o_s = 1;
        assert_eq!(s.packetin(&eos), 0);
        let p1 = s.flush().unwrap();
        let b1 = [p1.header, p1.body].concat();

        let mut bogus = vec![0u8; 27 + 100];
        bogus[..4].copy_from_slice(b"OggS");
        bogus[26] = 100;
        bogus[27..].fill(255);

        let mut raw = Vec::new();
        raw.extend_from_slice(&b0);
        raw.extend_from_slice(&bogus);
        raw.extend_from_slice(&b1);

        let mut cursor = std::io::Cursor::new(raw);
        let mut demux = OggDemuxer::new();
        let mut got = Vec::new();
        loop {
            let n = demux.read_from(&mut cursor, 64).unwrap();
            while let Some((serial, PacketOut::Packet(p))) = demux.next_packet() {
                assert_eq!(serial, 77);
                got.push(p.packet.to_vec());
            }
            if n == 0 {
                break;
            }
        }
        assert_eq!(got, vec![b"bos".to_vec(), b"eos".to_vec()]);
    }
}
