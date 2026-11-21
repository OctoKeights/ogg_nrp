# Verification

[Overview](../README.md) · [Guide](guide.md) · **Verification**

---

ogg_nrp is checked against libogg itself: both are run over the same made-up input and
everything they give back compared byte for byte.

## The comparison

`compare/compare.sh` builds `compare/dump.c` against a checkout of libogg and
`examples/dump.rs` against ogg_nrp, runs both, and compares what they print:

```sh
compare/compare.sh <checkout of libogg 1.3.5>
```

Both programs make the same input from each seed:

- **bits:** the bit packers written and read back in both orders, with values of every
  width, and writes and reads that fail.

- **framing:** packets of every awkward size (empty, a multiple of 255, longer than a page)
  turned into pages of several interleaved streams; the bytes then damaged (bits flipped,
  pieces cut out, garbage put in) and read back in pieces of random sizes.

Both print every page, packet and return value, and the two printouts have to be the same
byte for byte.

## Without libogg

`cargo test` runs the first forty seeds of both against hashes of what libogg printed for
them (`tests/golden.rs`), so any change in output shows without a C compiler.
