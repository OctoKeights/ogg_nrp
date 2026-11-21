# ogg_nrp

**The Ogg container in pure, safe Rust, byte for byte as libogg.**

**Overview** · [Guide](docs/guide.md) · [Verification](docs/verification.md)

---

ogg_nrp is a native Rust port of [libogg](https://github.com/xiph/ogg) 1.3.5 by the
Xiph.Org Foundation: the container of Vorbis, Theora, Opus and FLAC streams. It finds the
pages in a file and gives each stream's packets, puts packets into pages, and has the bit
packers a codec reads and writes its packets with.

## Highlights

- **Byte-for-byte output.** Every page, packet and return value matches libogg's, in both
  directions, damaged input included.

- **Everything libogg has.** `SyncState` finds pages in bytes given in pieces of any size,
  past damage; `StreamState` turns pages into packets and packets into pages, with
  libogg's choices of where a page ends; and the bit packers, least or most significant bit
  first.

- **libogg's names.** The functions keep their names without the `ogg_` prefix, as methods,
  and return what libogg returns, so its documentation reads for this crate too.

- **Zero dependencies.** Standard library only: no C compiler, no build script.

- **No `unsafe` code.** Enforced by `#![forbid(unsafe_code)]`.

## Installation

```toml
[dependencies]
ogg_nrp = "0.1"
```

## Quick start

```rust
use ogg_nrp::{StreamState, SyncState};

fn packets(file: &[u8]) -> Vec<Vec<u8>> {
    let mut sync = SyncState::new();
    sync.write(file);
    let mut stream: Option<StreamState> = None;
    let mut found = Vec::new();
    loop {
        let (got, page) = sync.pageout();
        if got == 0 {
            break; // no more whole pages
        }
        let Some(page) = page else { continue }; // bytes skipped to the next page
        let stream = stream.get_or_insert_with(|| StreamState::new(page.serialno()));
        if stream.pagein(&page) != 0 {
            continue; // a page of another stream
        }
        while let (1, Some(packet)) = stream.packetout() {
            found.push(packet.packet.to_vec());
        }
    }
    found
}
```

The [Guide](docs/guide.md) has writing pages, the bit packers, and where the crate differs
from libogg. [theora_nrp](https://github.com/OctoKeights/theora_nrp), a Theora video
decoder, reads its packets with it.

## Licence

BSD 3-clause, as libogg is; see [`LICENSE`](LICENSE), which keeps libogg's copyright notice.

Bug reports and contributions are welcome, above all any input that ogg_nrp reads or writes
differently from libogg.
