# Guide

[Overview](../README.md) · **Guide** · [Verification](verification.md)

---

- [Pages and packets](#pages-and-packets)
- [Reading a file](#reading-a-file)
- [Writing a stream](#writing-a-stream)
- [The bit packers](#the-bit-packers)
- [Correspondence with libogg](#correspondence-with-libogg)
- [Differences from libogg](#differences-from-libogg)

## Pages and packets

An Ogg file is a run of **pages**, each up to about 64 KB: a 27-byte header (the capture
pattern `OggS`, the stream's serial number, the page's number in its stream, a granule
position, a checksum) and a segment table, then the body. A **packet**, the unit a codec
writes, is cut into segments of up to 255 bytes and spread over as many pages as it needs.
Several logical streams (a video and its sound) are interleaved page by page in one file,
each known by its serial number.

## Reading a file

`SyncState` takes the file's bytes, in pieces of any size (`write`), and gives whole pages
(`pageout`), checking each one's checksum and skipping to the next capture pattern past
damage. `pageout` returns libogg's three answers: 1 and a page, 0 when it needs more bytes,
and -1 when bytes were skipped.

`StreamState::new(serial)` takes the pages of one stream (`pagein`, which refuses a page of
another stream) and gives its packets (`packetout`, or `packetpeek` to look without taking).
A gap in the page numbers shows as a -1 from `packetout`: a packet was lost there.

A page or packet is a pair of slices borrowed from the state that made it, good until the
next call on that state; copy what you keep.

## Writing a stream

`StreamState::packetin` takes packets, with their granule position and the beginning- and
end-of-stream flags; `pageout` gives a page once enough is buffered for one, and `flush`
gives whatever is buffered as a page at once (after the headers, before seeking). As in
libogg, `pageout_fill` and `flush_fill` take the page size to aim for.

## The bit packers

Codecs read and write their packets a bit at a time: `PackWriter` and `PackReader` least
significant bit first (libogg's `oggpack_*`, which Vorbis uses), `PackWriterB` and
`PackReaderB` most significant bit first (its `oggpackB_*`, which Theora uses). `write`
takes a value and a number of bits up to 32; `read` gives them back, or -1 past the end;
`look` reads without moving on.

## Correspondence with libogg

| libogg | ogg_nrp |
|---|---|
| `ogg_sync_state`, `ogg_sync_*` | `SyncState` and its methods |
| `ogg_stream_state`, `ogg_stream_*` | `StreamState` and its methods |
| `ogg_page`, `ogg_page_*` | `Page` and its methods |
| `ogg_packet` | `Packet` |
| `oggpack_buffer`, `oggpack_*` | `PackWriter`, `PackReader` |
| `oggpackB_*` | `PackWriterB`, `PackReaderB` |

The return values are libogg's: 0 for success, -1 for failure, and so on, so its
documentation reads for these too.

## Differences from libogg

- A page or a packet the C library returns as pointers into its own buffers is a pair of
  slices borrowed from the state here: it is good until the next call on that state, and the
  compiler holds you to it.
- There is nothing to free: `ogg_stream_destroy`, `ogg_sync_destroy` and `ogg_packet_clear`
  have no counterpart. `clear` is still there, and leaves a state that refuses every call, as
  in the C library.
- A page number is never negative (the C library's `long` where it has 64 bits).
- `StreamState::pagein` refuses a page put together by hand whose lengths disagree with its
  segment table, where the C library would read past its end.
