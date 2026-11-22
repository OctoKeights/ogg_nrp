# Changelog

## 0.2.0

- **Multi-stream demuxing, EOF framing and `std::io` helpers.** Added `OggDemuxer` for
  multiplexed Ogg streams, `SyncState::pageout_eof` for trailing unpaginated bytes,
  `SyncState::read_from` and `Page::write_to` (under the default `std` feature), typed
  `PageOut` and `PacketOut` wrappers, `Packet::new` / `bos` / `eos`, `try_read` / `try_look`,
  and `PackWriter::into_vec`.

- **`#![no_std]` support.** Disabling default features builds with `#![no_std]` and `alloc`.

- **Hardened bounds and faster framing.** Guarded negative bit advances and overflow edges in
  the bit packers, made `Page` accessors and `StreamState::pagein` bounds-safe on short
  headers, added `i32::MAX` guards on `iovecin` and checked buffer growth in `SyncState`,
  tracked read/write mode on `StreamState` so mixing encode and decode returns `-1` instead of
  panicking, removed the throwaway allocation in `StreamState::clear`, and eliminated
  quadratic `copy_within` shifts when flushing multi-page packets.

- Output on valid and damaged streams is unchanged, checked against libogg 1.3.5.

## 0.1.0

- First release: a native port of [libogg](https://github.com/xiph/ogg) 1.3.5, the Ogg
  container by the Xiph.Org Foundation, moved out of
  Noolith (a game of the author's), where it was `ogg-rs`: `SyncState`,
  `StreamState` and `Page` for reading and writing pages and packets, and the bit packers
  `PackWriter`, `PackReader`, `PackWriterB` and `PackReaderB`.

- **Checked against libogg.** Every page, packet and return value matches libogg's on
  made-up and damaged input (see [Verification](docs/verification.md)).
