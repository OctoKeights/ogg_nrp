//! Framing: the port of libogg's `framing.c`. Packets go into a [`StreamState`]
//! and come out as pages. The bytes of a file go into a [`SyncState`] and come
//! out as pages, which go into a [`StreamState`] and come out as packets.

/// The CRC tables of `crctable.h`, worked out when the crate is built: the
/// Ethernet generator polynomial, unreflected, with a start and an end value of
/// 0, as slices of eight.
const CRC_LOOKUP: [[u32; 256]; 8] = {
    let mut table = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = (i as u32) << 24;
        let mut j = 0;
        while j < 8 {
            crc = (crc << 1) ^ (if crc & (1 << 31) != 0 { 0x04c1_1db7 } else { 0 });
            j += 1;
        }
        table[0][i] = crc;
        i += 1;
    }
    let mut i = 0;
    while i < 256 {
        let mut j = 1;
        while j < 8 {
            table[j][i] =
                table[0][((table[j - 1][i] >> 24) & 0xFF) as usize] ^ (table[j - 1][i] << 8);
            j += 1;
        }
        i += 1;
    }
    table
};

fn update_crc(mut crc: u32, mut buffer: &[u8]) -> u32 {
    while buffer.len() >= 8 {
        crc ^= u32::from(buffer[0]) << 24
            | u32::from(buffer[1]) << 16
            | u32::from(buffer[2]) << 8
            | u32::from(buffer[3]);
        crc = CRC_LOOKUP[7][(crc >> 24) as usize]
            ^ CRC_LOOKUP[6][((crc >> 16) & 0xFF) as usize]
            ^ CRC_LOOKUP[5][((crc >> 8) & 0xFF) as usize]
            ^ CRC_LOOKUP[4][(crc & 0xFF) as usize]
            ^ CRC_LOOKUP[3][usize::from(buffer[4])]
            ^ CRC_LOOKUP[2][usize::from(buffer[5])]
            ^ CRC_LOOKUP[1][usize::from(buffer[6])]
            ^ CRC_LOOKUP[0][usize::from(buffer[7])];
        buffer = &buffer[8..];
    }
    for &byte in buffer {
        crc = (crc << 8) ^ CRC_LOOKUP[0][(((crc >> 24) & 0xff) ^ u32::from(byte)) as usize];
    }
    crc
}

/// Returns the checksum of a page, with the header's own checksum field (bytes
/// 22 to 25) read as zeros whatever it holds.
fn page_crc(header: &[u8], body: &[u8]) -> u32 {
    let mut crc = 0;
    if header.len() > 22 {
        crc = update_crc(crc, &header[..22]);
        let field = header.len().min(26) - 22;
        crc = update_crc(crc, &[0u8; 4][..field]);
        if header.len() > 26 {
            crc = update_crc(crc, &header[26..]);
        }
    } else {
        crc = update_crc(crc, header);
    }
    update_crc(crc, body)
}

/// `ogg_page_checksum_set`: works out a page's checksum, and writes it into its
/// header. A header too short to hold one is left alone.
pub fn page_checksum_set(header: &mut [u8], body: &[u8]) {
    if header.len() < 26 {
        return;
    }
    let crc = page_crc(header, body);
    header[22..26].copy_from_slice(&crc.to_le_bytes());
}

/// One page of an Ogg stream (`ogg_page`): its header (27 bytes and the segment
/// table) and its body, borrowed from the state that made or found it.
#[derive(Clone, Copy, Debug)]
pub struct Page<'a> {
    /// The header: 27 bytes, then the segment table.
    pub header: &'a [u8],
    /// The body: the packet segments the table lists.
    pub body: &'a [u8],
}

impl Page<'_> {
    /// `ogg_page_version`: the stream structure version (0).
    #[must_use]
    pub fn version(&self) -> i32 {
        i32::from(self.header[4])
    }

    /// `ogg_page_continued`: the page begins with the rest of a packet begun before
    /// it.
    #[must_use]
    pub fn continued(&self) -> bool {
        self.header[5] & 0x01 != 0
    }

    /// `ogg_page_bos`: the first page of its stream.
    #[must_use]
    pub fn bos(&self) -> bool {
        self.header[5] & 0x02 != 0
    }

    /// `ogg_page_eos`: the last page of its stream.
    #[must_use]
    pub fn eos(&self) -> bool {
        self.header[5] & 0x04 != 0
    }

    /// `ogg_page_granulepos`: the position of the last packet that ends on this
    /// page (-1 when none does).
    #[must_use]
    pub fn granulepos(&self) -> i64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.header[6..14]);
        i64::from_le_bytes(bytes)
    }

    /// `ogg_page_serialno`: the number of the stream the page belongs to.
    #[must_use]
    pub fn serialno(&self) -> i32 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.header[14..18]);
        i32::from_le_bytes(bytes)
    }

    /// `ogg_page_pageno`: the page's number in its stream (never negative: the C
    /// library's `long` where it has 64 bits).
    #[must_use]
    pub fn pageno(&self) -> i64 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.header[18..22]);
        i64::from(u32::from_le_bytes(bytes))
    }

    /// `ogg_page_packets`: the packets that end on this page (one begun on an
    /// earlier page counts, and one that goes on to the next does not).
    #[must_use]
    pub fn packets(&self) -> i32 {
        let n = usize::from(self.header[26]);
        self.header[27..27 + n]
            .iter()
            .filter(|&&lacing| lacing < 255)
            .count() as i32
    }
}

/// One packet (`ogg_packet`): its bytes, borrowed, and what the framing knows
/// of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Packet<'a> {
    /// The packet's bytes (the C library's `packet` and `bytes`).
    pub packet: &'a [u8],
    /// Not zero for the first packet of its stream.
    pub b_o_s: i64,
    /// Not zero for the last packet of its stream.
    pub e_o_s: i64,
    /// The position the packet ends at, or -1 unless it is the last to end on its
    /// page.
    pub granulepos: i64,
    /// The packet's number in its stream. The framing knows where there is a hole
    /// in the data, and this is how the codec gets to know it.
    pub packetno: i64,
}

/// A logical stream being written or read (`ogg_stream_state`).
#[derive(Clone, Debug)]
pub struct StreamState {
    /// Bytes from packet bodies; its length is the C library's `body_storage`.
    body_data: Vec<u8>,
    /// Bytes stored; fill mark.
    body_fill: usize,
    /// Bytes of fill returned.
    body_returned: usize,
    /// The values that will go to the segment table, with flags above the low byte:
    /// 0x100 begins a packet, 0x200 ends the stream and 0x400 marks a hole.
    lacing_vals: Vec<i32>,
    /// A granulepos for each lacing value.
    granule_vals: Vec<i64>,
    lacing_fill: usize,
    lacing_packet: usize,
    lacing_returned: usize,
    /// Working space for header encode.
    header: [u8; 282],
    header_fill: usize,
    /// Set when the last packet of the stream has been buffered.
    e_o_s: bool,
    /// Set after the first page of the stream has been written.
    b_o_s: bool,
    serialno: i64,
    pageno: i64,
    packetno: i64,
    granulepos: i64,
    /// After `clear` the C library's state has no storage and every call fails.
    cleared: bool,
}

impl StreamState {
    /// `ogg_stream_init`: a stream with this serial number.
    #[must_use]
    pub fn new(serialno: i32) -> Self {
        Self {
            body_data: vec![0; 16 * 1024],
            body_fill: 0,
            body_returned: 0,
            lacing_vals: vec![0; 1024],
            granule_vals: vec![0; 1024],
            lacing_fill: 0,
            lacing_packet: 0,
            lacing_returned: 0,
            header: [0; 282],
            header_fill: 0,
            e_o_s: false,
            b_o_s: false,
            serialno: i64::from(serialno),
            pageno: 0,
            packetno: 0,
            granulepos: 0,
            cleared: false,
        }
    }

    /// `ogg_stream_check`: 0 while the stream is usable, and -1 once it has been
    /// cleared.
    #[must_use]
    pub fn check(&self) -> i32 {
        if self.cleared { -1 } else { 0 }
    }

    /// `ogg_stream_clear`: frees the storage. Every later call fails, as in the C
    /// library.
    pub fn clear(&mut self) {
        *self = Self::new(0);
        self.body_data = Vec::new();
        self.lacing_vals = Vec::new();
        self.granule_vals = Vec::new();
        self.cleared = true;
    }

    /// The stream's serial number.
    #[must_use]
    pub fn serialno(&self) -> i64 {
        self.serialno
    }

    fn body_expand(&mut self, needed: usize) {
        if self.body_data.len() as i64 - needed as i64 <= self.body_fill as i64 {
            self.body_data
                .resize(self.body_data.len() + needed + 1024, 0);
        }
    }

    fn lacing_expand(&mut self, needed: usize) {
        if self.lacing_vals.len() as i64 - needed as i64 <= self.lacing_fill as i64 {
            let storage = self.lacing_vals.len() + needed + 32;
            self.lacing_vals.resize(storage, 0);
            self.granule_vals.resize(storage, 0);
        }
    }

    /// `ogg_stream_iovecin`: one packet given as pieces, to go into pages.
    pub fn iovecin(&mut self, iov: &[&[u8]], e_o_s: bool, granulepos: i64) -> i32 {
        if self.check() != 0 {
            return -1;
        }
        let bytes: usize = iov.iter().map(|piece| piece.len()).sum();
        let lacing_vals = bytes / 255 + 1;

        if self.body_returned != 0 {
            // Advance packet data according to the body_returned pointer. We had to keep it
            // around to return a pointer into the buffer last call.
            self.body_fill -= self.body_returned;
            if self.body_fill != 0 {
                self.body_data
                    .copy_within(self.body_returned..self.body_returned + self.body_fill, 0);
            }
            self.body_returned = 0;
        }

        // Make sure we have the buffer storage.
        self.body_expand(bytes);
        self.lacing_expand(lacing_vals);

        // Copy in the submitted packet.
        for piece in iov {
            self.body_data[self.body_fill..self.body_fill + piece.len()].copy_from_slice(piece);
            self.body_fill += piece.len();
        }

        // Store lacing vals for this packet.
        let at = self.lacing_fill;
        for i in 0..lacing_vals - 1 {
            self.lacing_vals[at + i] = 255;
            self.granule_vals[at + i] = self.granulepos;
        }
        self.lacing_vals[at + lacing_vals - 1] = (bytes % 255) as i32;
        self.granule_vals[at + lacing_vals - 1] = granulepos;
        self.granulepos = granulepos;

        // Flag the first segment as the beginning of the packet.
        self.lacing_vals[at] |= 0x100;

        self.lacing_fill += lacing_vals;

        // For the sake of completeness.
        self.packetno += 1;

        if e_o_s {
            self.e_o_s = true;
        }
        0
    }

    /// `ogg_stream_packetin`: one packet, to go into pages. Its `e_o_s` and
    /// `granulepos` are used, and the stream counts the packets itself.
    pub fn packetin(&mut self, op: &Packet) -> i32 {
        self.iovecin(&[op.packet], op.e_o_s != 0, op.granulepos)
    }

    /// Conditionally flushes a page. Without `force`, only pages of the nominal
    /// size are flushed, and with it, a page is flushed as long as there is any
    /// data at all.
    fn flush_i(&mut self, mut force: bool, nfill: i32) -> Option<Page<'_>> {
        let maxvals = self.lacing_fill.min(255);
        let mut vals = 0;
        let mut bytes = 0usize;
        let mut acc: i64 = 0;
        let mut granule_pos: i64 = -1;

        if self.check() != 0 {
            return None;
        }
        if maxvals == 0 {
            return None;
        }

        // Construct a page: decide how many segments to include.
        if self.b_o_s {
            // Don't unnecessarily span pages, and unless necessary, don't flush pages with
            // fewer than four packets on them: this expands the page size to reduce
            // overhead if incoming packets are large.
            let mut packets_done = 0;
            let mut packet_just_done = 0;
            while vals < maxvals {
                if acc > i64::from(nfill) && packet_just_done >= 4 {
                    force = true;
                    break;
                }
                acc += i64::from(self.lacing_vals[vals] & 0x0ff);
                if (self.lacing_vals[vals] & 0xff) < 255 {
                    granule_pos = self.granule_vals[vals];
                    packets_done += 1;
                    packet_just_done = packets_done;
                } else {
                    packet_just_done = 0;
                }
                vals += 1;
            }
            if vals == 255 {
                force = true;
            }
        } else {
            // The initial header page: it must only include the initial header packet.
            granule_pos = 0;
            while vals < maxvals {
                if (self.lacing_vals[vals] & 0x0ff) < 255 {
                    vals += 1;
                    break;
                }
                vals += 1;
            }
        }

        if !force {
            return None;
        }

        // Construct the header in temp storage.
        self.header[..4].copy_from_slice(b"OggS");

        // Stream structure version.
        self.header[4] = 0x00;

        // Continued packet flag?
        self.header[5] = 0x00;
        if self.lacing_vals[0] & 0x100 == 0 {
            self.header[5] |= 0x01;
        }
        // First page flag?
        if !self.b_o_s {
            self.header[5] |= 0x02;
        }
        // Last page flag?
        if self.e_o_s && self.lacing_fill == vals {
            self.header[5] |= 0x04;
        }
        self.b_o_s = true;

        // 64 bits of PCM position.
        self.header[6..14].copy_from_slice(&granule_pos.to_le_bytes());

        // 32 bits of stream serial number.
        self.header[14..18].copy_from_slice(&self.serialno.to_le_bytes()[..4]);

        // 32 bits of page counter (we have both counter and page header because this
        // value can roll over). It is -1 because someone called reset: a strange thing
        // to do in a stream being written, but it has plausible uses.
        if self.pageno == -1 {
            self.pageno = 0;
        }
        self.header[18..22].copy_from_slice(&self.pageno.to_le_bytes()[..4]);
        self.pageno += 1;

        // Zero for computation; filled in later.
        self.header[22..26].fill(0);

        // Segment table.
        self.header[26] = (vals & 0xff) as u8;
        for i in 0..vals {
            let lacing = (self.lacing_vals[i] & 0xff) as u8;
            self.header[i + 27] = lacing;
            bytes += usize::from(lacing);
        }

        self.header_fill = vals + 27;
        let body_at = self.body_returned;

        // Advance the lacing data and set the body_returned pointer.
        self.lacing_fill -= vals;
        self.lacing_vals
            .copy_within(vals..vals + self.lacing_fill, 0);
        self.granule_vals
            .copy_within(vals..vals + self.lacing_fill, 0);
        self.body_returned += bytes;

        // Calculate the checksum.
        let header_fill = self.header_fill;
        page_checksum_set(
            &mut self.header[..header_fill],
            &self.body_data[body_at..body_at + bytes],
        );

        Some(Page {
            header: &self.header[..header_fill],
            body: &self.body_data[body_at..body_at + bytes],
        })
    }

    /// `ogg_stream_flush`: a page of what is buffered, even an undersized one, or
    /// `None` when nothing is. It flushes one page of the usual size at a time, so
    /// only `None` says that all packets have gone into pages.
    pub fn flush(&mut self) -> Option<Page<'_>> {
        self.flush_i(true, 4096)
    }

    /// `ogg_stream_flush_fill`: [`flush`](Self::flush), with the nominal page size
    /// given.
    pub fn flush_fill(&mut self, nfill: i32) -> Option<Page<'_>> {
        self.flush_i(true, nfill)
    }

    fn pageout_force(&self) -> bool {
        // "We're done, now flush", and the initial header page.
        self.lacing_fill != 0 && (self.e_o_s || !self.b_o_s)
    }

    /// `ogg_stream_pageout`: a page, once enough is buffered for one. The page is
    /// good until the next call on this stream.
    pub fn pageout(&mut self) -> Option<Page<'_>> {
        if self.check() != 0 {
            return None;
        }
        let force = self.pageout_force();
        self.flush_i(force, 4096)
    }

    /// `ogg_stream_pageout_fill`: [`pageout`](Self::pageout), with the nominal page
    /// size given.
    pub fn pageout_fill(&mut self, nfill: i32) -> Option<Page<'_>> {
        if self.check() != 0 {
            return None;
        }
        let force = self.pageout_force();
        self.flush_i(force, nfill)
    }

    /// `ogg_stream_eos`: whether the stream's last packet has been written or read
    /// (also true of a cleared stream).
    #[must_use]
    pub fn eos(&self) -> bool {
        if self.check() != 0 {
            return true;
        }
        self.e_o_s
    }

    /// `ogg_stream_pagein`: adds a page read from a file to the stream, cut into
    /// packet segments. Returns -1 when the page is of another stream or version.
    #[allow(clippy::too_many_lines)] // libogg's function, kept whole
    pub fn pagein(&mut self, og: &Page) -> i32 {
        let header = og.header;
        let mut body = og.body;
        let mut segptr = 0usize;

        let version = og.version();
        let continued = og.continued();
        let mut bos = og.bos();
        let eos = og.eos();
        let granulepos = og.granulepos();
        let serialno = og.serialno();
        let pageno = og.pageno();
        let segments = usize::from(header[26]);

        if self.check() != 0 {
            return -1;
        }
        // A page from a `SyncState` or a `StreamState` is always whole. One put
        // together by hand whose lengths disagree with its segment table is not taken,
        // where the C library reads past its end.
        if header.len() < 27 + segments
            || body.len()
                != header[27..27 + segments]
                    .iter()
                    .map(|&v| usize::from(v))
                    .sum::<usize>()
        {
            return -1;
        }

        // Clean up 'returned data'.
        {
            let lr = self.lacing_returned;
            let br = self.body_returned;

            // Body data.
            if br != 0 {
                self.body_fill -= br;
                if self.body_fill != 0 {
                    self.body_data.copy_within(br..br + self.body_fill, 0);
                }
                self.body_returned = 0;
            }

            if lr != 0 {
                // Segment table.
                if self.lacing_fill - lr != 0 {
                    self.lacing_vals.copy_within(lr..self.lacing_fill, 0);
                    self.granule_vals.copy_within(lr..self.lacing_fill, 0);
                }
                self.lacing_fill -= lr;
                self.lacing_packet -= lr;
                self.lacing_returned = 0;
            }
        }

        // Check the serial number.
        if i64::from(serialno) != self.serialno {
            return -1;
        }
        if version > 0 {
            return -1;
        }

        self.lacing_expand(segments + 1);

        // Are we in sequence?
        if pageno != self.pageno {
            // Unroll previous partial packet (if any).
            for i in self.lacing_packet..self.lacing_fill {
                self.body_fill -= (self.lacing_vals[i] & 0xff) as usize;
            }
            self.lacing_fill = self.lacing_packet;

            // Make a note of dropped data in segment table.
            if self.pageno != -1 {
                self.lacing_vals[self.lacing_fill] = 0x400;
                self.lacing_fill += 1;
                self.lacing_packet += 1;
            }
        }

        // Are we a 'continued packet' page? If so, we may need to skip some segments.
        if continued
            && (self.lacing_fill < 1
                || (self.lacing_vals[self.lacing_fill - 1] & 0xff) < 255
                || self.lacing_vals[self.lacing_fill - 1] == 0x400)
        {
            bos = false;
            while segptr < segments {
                let val = usize::from(header[27 + segptr]);
                body = &body[val..];
                segptr += 1;
                if val < 255 {
                    break;
                }
            }
        }

        if !body.is_empty() {
            self.body_expand(body.len());
            self.body_data[self.body_fill..self.body_fill + body.len()].copy_from_slice(body);
            self.body_fill += body.len();
        }

        {
            let mut saved = None;
            while segptr < segments {
                let val = i32::from(header[27 + segptr]);
                self.lacing_vals[self.lacing_fill] = val;
                self.granule_vals[self.lacing_fill] = -1;

                if bos {
                    self.lacing_vals[self.lacing_fill] |= 0x100;
                    bos = false;
                }

                if val < 255 {
                    saved = Some(self.lacing_fill);
                }

                self.lacing_fill += 1;
                segptr += 1;

                if val < 255 {
                    self.lacing_packet = self.lacing_fill;
                }
            }

            // Set the granulepos on the last granuleval of the last full packet.
            if let Some(saved) = saved {
                self.granule_vals[saved] = granulepos;
            }
        }

        if eos {
            self.e_o_s = true;
            if self.lacing_fill > 0 {
                self.lacing_vals[self.lacing_fill - 1] |= 0x200;
            }
        }

        self.pageno = pageno + 1;

        0
    }

    /// `ogg_stream_reset`: back to an initial state, keeping the serial number. It
    /// is good to call before seeking, for one.
    pub fn reset(&mut self) -> i32 {
        if self.check() != 0 {
            return -1;
        }
        self.body_fill = 0;
        self.body_returned = 0;

        self.lacing_fill = 0;
        self.lacing_packet = 0;
        self.lacing_returned = 0;

        self.header_fill = 0;

        self.e_o_s = false;
        self.b_o_s = false;
        self.pageno = -1;
        self.packetno = 0;
        self.granulepos = 0;
        0
    }

    /// `ogg_stream_reset_serialno`: [`reset`](Self::reset), with another serial
    /// number.
    pub fn reset_serialno(&mut self, serialno: i32) -> i32 {
        if self.check() != 0 {
            return -1;
        }
        self.reset();
        self.serialno = i64::from(serialno);
        0
    }

    /// The last part of decode: the stream is broken into packet segments, which
    /// are grouped into packets here (or the out-of-sync marker is returned).
    fn packetout_i(&mut self, adv: bool) -> (i32, Option<Packet<'_>>) {
        let mut ptr = self.lacing_returned;

        if self.lacing_packet <= ptr {
            return (0, None);
        }

        if self.lacing_vals[ptr] & 0x400 != 0 {
            // We need to tell the codec there's a gap; it might need to handle previous
            // packet dependencies.
            self.lacing_returned += 1;
            self.packetno += 1;
            return (-1, None);
        }

        // Gather the whole packet. We'll have no holes or a partial packet.
        let mut size = self.lacing_vals[ptr] & 0xff;
        let mut bytes = size as usize;
        let mut eos = self.lacing_vals[ptr] & 0x200; // Last packet of the stream?
        let bos = self.lacing_vals[ptr] & 0x100; // First packet of the stream?

        while size == 255 {
            ptr += 1;
            let val = self.lacing_vals[ptr];
            size = val & 0xff;
            if val & 0x200 != 0 {
                eos = 0x200;
            }
            bytes += size as usize;
        }

        let at = self.body_returned;
        let packet = Packet {
            packet: &[],
            e_o_s: i64::from(eos),
            b_o_s: i64::from(bos),
            packetno: self.packetno,
            granulepos: self.granule_vals[ptr],
        };

        if adv {
            self.body_returned += bytes;
            self.lacing_returned = ptr + 1;
            self.packetno += 1;
        }
        (
            1,
            Some(Packet {
                packet: &self.body_data[at..at + bytes],
                ..packet
            }),
        )
    }

    /// `ogg_stream_packetout`: the next packet. Returns 1 with a packet, 0 when
    /// there is not a whole one yet, and -1 where data is missing before the next
    /// one (a hole: ask again). The packet is good until the next call on this
    /// stream.
    pub fn packetout(&mut self) -> (i32, Option<Packet<'_>>) {
        if self.check() != 0 {
            return (0, None);
        }
        self.packetout_i(true)
    }

    /// `ogg_stream_packetpeek`: [`packetout`](Self::packetout) without taking the
    /// packet out (a hole is still passed over).
    pub fn packetpeek(&mut self) -> (i32, Option<Packet<'_>>) {
        if self.check() != 0 {
            return (0, None);
        }
        self.packetout_i(false)
    }
}

/// The bytes of a physical stream on their way to becoming pages
/// (`ogg_sync_state`).
#[derive(Clone, Debug, Default)]
pub struct SyncState {
    /// Its length is the C library's `storage`.
    data: Vec<u8>,
    fill: usize,
    returned: usize,
    unsynced: bool,
    headerbytes: usize,
    bodybytes: usize,
    /// Where the page found last lies in `data`: its start, and its header's
    /// length.
    page_at: usize,
    page_header: usize,
    page_body: usize,
}

impl SyncState {
    /// `ogg_sync_init`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `ogg_sync_clear`: frees the storage.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `ogg_sync_check`: always 0, as a `SyncState` cannot be had unready.
    #[must_use]
    pub fn check(&self) -> i32 {
        0
    }

    /// `ogg_sync_buffer`: room for at least `size` more bytes, to be filled from
    /// its start and then reported with [`wrote`](Self::wrote). Returns `None`, and
    /// the state is cleared, when `size` is more than the C library takes.
    pub fn buffer(&mut self, size: usize) -> Option<&mut [u8]> {
        // First, clear out any space that has been previously returned.
        if self.returned != 0 {
            self.fill -= self.returned;
            if self.fill > 0 {
                self.data
                    .copy_within(self.returned..self.returned + self.fill, 0);
            }
            self.returned = 0;
        }

        if size as i64 > self.data.len() as i64 - self.fill as i64 {
            // We need to extend the internal buffer.
            if size as i64 > i64::from(i32::MAX) - 4096 - self.fill as i64 {
                self.clear();
                return None;
            }
            let newsize = size + self.fill + 4096; // An extra page to be nice.
            self.data.resize(newsize, 0);
        }

        // Expose a segment at least as large as requested at the fill mark.
        Some(&mut self.data[self.fill..])
    }

    /// `ogg_sync_wrote`: `bytes` more bytes were put into
    /// [`buffer`](Self::buffer)'s room. Returns -1 when that is more than the room
    /// there was.
    pub fn wrote(&mut self, bytes: usize) -> i32 {
        if self.fill + bytes > self.data.len() {
            return -1;
        }
        self.fill += bytes;
        0
    }

    /// [`buffer`](Self::buffer), a copy and [`wrote`](Self::wrote) in one: adds
    /// `bytes` to what is to be cut into pages. Returns -1 when the C library would
    /// not take as much.
    pub fn write(&mut self, bytes: &[u8]) -> i32 {
        match self.buffer(bytes.len()) {
            Some(room) => room[..bytes.len()].copy_from_slice(bytes),
            None => return -1,
        }
        self.wrote(bytes.len())
    }

    fn page(&self) -> Page<'_> {
        let header_end = self.page_at + self.page_header;
        Page {
            header: &self.data[self.page_at..header_end],
            body: &self.data[header_end..header_end + self.page_body],
        }
    }

    fn pageseek_i(&mut self) -> i64 {
        let page = self.returned;
        let bytes = self.fill - self.returned;

        let synced = 'sync: {
            if self.headerbytes == 0 {
                if bytes < 27 {
                    return 0; // Not enough for a header.
                }

                // Verify capture pattern.
                if &self.data[page..page + 4] != b"OggS" {
                    break 'sync false;
                }

                let headerbytes = usize::from(self.data[page + 26]) + 27;
                if bytes < headerbytes {
                    return 0; // Not enough for header + seg table.
                }

                // Count up body length in the segment table.
                for i in 0..usize::from(self.data[page + 26]) {
                    self.bodybytes += usize::from(self.data[page + 27 + i]);
                }
                self.headerbytes = headerbytes;
            }

            if self.bodybytes + self.headerbytes > bytes {
                return 0;
            }

            // The whole test page is buffered. Verify the checksum.
            let header = &self.data[page..page + self.headerbytes];
            let body =
                &self.data[page + self.headerbytes..page + self.headerbytes + self.bodybytes];
            let stored = u32::from_le_bytes([header[22], header[23], header[24], header[25]]);
            // A mismatch is a corrupt page (or a miscapture, and not a page at all), so
            // sync is lost.
            page_crc(header, body) == stored
        };

        if synced {
            // Yes, have a whole page all ready to go.
            self.page_at = page;
            self.page_header = self.headerbytes;
            self.page_body = self.bodybytes;

            self.unsynced = false;
            let bytes = self.headerbytes + self.bodybytes;
            self.returned += bytes;
            self.headerbytes = 0;
            self.bodybytes = 0;
            return bytes as i64;
        }

        self.headerbytes = 0;
        self.bodybytes = 0;

        // Search for possible capture.
        let next = match self.data[page + 1..page + bytes]
            .iter()
            .position(|&byte| byte == b'O')
        {
            Some(found) => page + 1 + found,
            None => self.fill,
        };

        self.returned = next;
        -((next - page) as i64)
    }

    /// `ogg_sync_pageseek`: syncs the stream, for finding page boundaries. A number
    /// above 0, with a page, says a page of that many bytes was found at the
    /// current place. 0 says more data is needed for one. Below 0, that many bytes
    /// were skipped: ask again.
    pub fn pageseek(&mut self) -> (i64, Option<Page<'_>>) {
        let ret = self.pageseek_i();
        if ret > 0 {
            (ret, Some(self.page()))
        } else {
            (ret, None)
        }
    }

    /// `ogg_sync_pageout`: the next page. Returns 1 with a page, 0 when more data
    /// is needed, and -1 where bytes had to be skipped to find the next page (a
    /// hole in the data, reported once: ask again). The page is good until the next
    /// call on this state.
    pub fn pageout(&mut self) -> (i32, Option<Page<'_>>) {
        // All we need to do is verify a page at the head of the stream buffer. If it
        // doesn't verify, we look for the next potential frame.
        loop {
            let ret = self.pageseek_i();
            if ret > 0 {
                // Have a page.
                return (1, Some(self.page()));
            }
            if ret == 0 {
                // Need more data.
                return (0, None);
            }

            // Head did not start a synced page... skipped some bytes.
            if !self.unsynced {
                self.unsynced = true;
                return (-1, None);
            }

            // Loop. Keep looking.
        }
    }

    /// `ogg_sync_reset`: forgets what is buffered. It is good to call before
    /// seeking, for one.
    pub fn reset(&mut self) -> i32 {
        self.fill = 0;
        self.returned = 0;
        self.unsynced = false;
        self.headerbytes = 0;
        self.bodybytes = 0;
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first values of libogg's crctable.h.
    #[test]
    fn crc_table() {
        assert_eq!(
            &CRC_LOOKUP[0][..4],
            &[0x0000_0000, 0x04c1_1db7, 0x0982_3b6e, 0x0d43_26d9]
        );
        assert_eq!(CRC_LOOKUP[0][255], 0xb1f7_40b4);
        assert_eq!(
            &CRC_LOOKUP[1][..3],
            &[0x0000_0000, 0xd219_c1dc, 0xa0f2_9e0f]
        );
        assert_eq!(CRC_LOOKUP[7][255], 0x6760_d264);
    }

    #[test]
    fn packets_through_pages() {
        let mut writer = StreamState::new(0x1234);
        let packets: Vec<Vec<u8>> = [0usize, 1, 254, 255, 256, 70000, 17]
            .iter()
            .map(|&n| (0..n).map(|i| i as u8).collect())
            .collect();
        let mut file = Vec::new();
        for (i, packet) in packets.iter().enumerate() {
            let op = Packet {
                packet,
                e_o_s: i64::from(i + 1 == packets.len()),
                granulepos: i as i64,
                ..Packet::default()
            };
            assert_eq!(writer.packetin(&op), 0);
            while let Some(page) = writer.pageout() {
                file.extend_from_slice(page.header);
                file.extend_from_slice(page.body);
            }
        }
        assert!(writer.eos());

        let mut sync = SyncState::new();
        let mut reader = StreamState::new(0x1234);
        let mut read = Vec::new();
        for chunk in file.chunks(999) {
            assert_eq!(sync.write(chunk), 0);
            while let (1, Some(page)) = sync.pageout() {
                assert_eq!(page.serialno(), 0x1234);
                assert_eq!(reader.pagein(&page), 0);
                while let (1, Some(packet)) = reader.packetout() {
                    assert_eq!(packet.packetno, read.len() as i64);
                    read.push(packet.packet.to_vec());
                }
            }
        }
        assert_eq!(read, packets);
        assert!(reader.eos());
    }

    #[test]
    fn a_hole_is_reported() {
        let mut writer = StreamState::new(1);
        let mut pages = Vec::new();
        for i in 0..6 {
            writer.packetin(&Packet {
                packet: &[i; 100],
                granulepos: i64::from(i),
                ..Packet::default()
            });
            while let Some(page) = writer.flush() {
                pages.push([page.header, page.body].concat());
            }
        }
        let mut sync = SyncState::new();
        let mut reader = StreamState::new(1);
        let mut results = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            if i == 2 {
                continue; // a page lost
            }
            sync.write(page);
            while let (1, Some(page)) = sync.pageout() {
                reader.pagein(&page);
                loop {
                    let (result, _) = reader.packetout();
                    if result == 0 {
                        break;
                    }
                    results.push(result);
                }
            }
        }
        assert_eq!(results, [1, 1, -1, 1, 1, 1]);
    }
}
