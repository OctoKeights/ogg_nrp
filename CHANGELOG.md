# Changelog

## 0.1.0

- First release: a native port of [libogg](https://github.com/xiph/ogg) 1.3.5, the Ogg
  container by the Xiph.Org Foundation, moved out of
  Noolith (a game of the author's), where it was `ogg-rs`: `SyncState`,
  `StreamState` and `Page` for reading and writing pages and packets, and the bit packers
  `PackWriter`, `PackReader`, `PackWriterB` and `PackReaderB`.

- **Checked against libogg.** Every page, packet and return value matches libogg's on
  made-up and damaged input (see [Verification](docs/verification.md)).
