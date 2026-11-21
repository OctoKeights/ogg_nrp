//! The Ogg container, ported to Rust from libogg 1.3.5 by the Xiph.Org
//! Foundation (<https://github.com/xiph/ogg>).
//!
//! It has two parts, as in the C library:
//!
//! - [`framing`] (the port of `framing.c`): a stream of bytes cut into pages
//!   with [`SyncState`], and pages turned into packets and packets into pages
//!   with [`StreamState`].
//! - [`bitwise`] (the port of `bitwise.c`): the bit packers a codec writes and
//!   reads its packets with, least significant bit first ([`PackWriter`],
//!   [`PackReader`]) and most significant bit first ([`PackWriterB`],
//!   [`PackReaderB`]).
//!
//! The functions keep the C library's names without their `ogg_` prefixes, as
//! methods, and return what it returns. Where the C library hands out a pointer
//! into its own buffer, the Rust hands out a slice borrowed from the state, so
//! it cannot be kept past the next call.
//!
//! Reading a file:
//!
//! ```
//! use ogg_nrp::{StreamState, SyncState};
//!
//! fn packets(file: &[u8]) -> Vec<Vec<u8>> {
//!     let mut sync = SyncState::new();
//!     sync.write(file);
//!     let mut stream: Option<StreamState> = None;
//!     let mut found = Vec::new();
//!     loop {
//!         let (got, page) = sync.pageout();
//!         if got == 0 {
//!             break; // No more whole pages.
//!         }
//!         // None: bytes were skipped to the next page.
//!         let Some(page) = page else { continue };
//!         let serialno = page.serialno();
//!         let stream =
//!             stream.get_or_insert_with(|| StreamState::new(serialno));
//!         if stream.pagein(&page) != 0 {
//!             continue; // A page of another stream.
//!         }
//!         while let (1, Some(packet)) = stream.packetout() {
//!             found.push(packet.packet.to_vec());
//!         }
//!     }
//!     found
//! }
//! # assert!(packets(&[]).is_empty());
//! ```

#![forbid(unsafe_code)]
// The arithmetic is kept as the C library writes it.
#![allow(clippy::manual_div_ceil)]

pub mod bitwise;
pub mod framing;

pub use bitwise::{PackReader, PackReaderB, PackWriter, PackWriterB};
pub use framing::{Packet, Page, StreamState, SyncState, page_checksum_set};
