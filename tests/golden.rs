//! The dumps of examples/dump.rs for seeds 1 to 40, against what libogg 1.3.5
//! printed for them (compare/dump.c, hashed): the port checked against the C
//! library without it there. compare/compare.sh does the same against the
//! library itself, for as many seeds as wanted.

// The hashes are as the C library's dump printed them.
#![allow(clippy::unreadable_literal)]

#[path = "../examples/dump.rs"]
mod dump;

fn check(what: &str, golden: &[u32]) {
    for (i, &expected) in golden.iter().enumerate() {
        let seed = i as u32 + 1;
        let text = dump::run(what, seed).unwrap();
        assert_eq!(dump::hash_of(&text), expected, "{what} {seed}");
    }
}

#[test]
fn bits() {
    check(
        "bits",
        &[
            0x9ff8d961, 0x1410380f, 0x3b449625, 0xc6e515cf, 0x44b87aed, 0xbf4e9dfd, 0x1e477027,
            0x501571ad, 0x8c2a95ad, 0xfcfe5895, 0x25b9a5e4, 0x395b1f45, 0xc9d0d596, 0x37298d4f,
            0x665024e8, 0x8d9afc33, 0xdb02bdbf, 0x9da17857, 0x55f4681e, 0x664e2b0c, 0xdcf30673,
            0x21524475, 0x755a6df4, 0x19b9f038, 0x138c5259, 0x1a10f962, 0x3d0d4d51, 0xd2192e63,
            0x2dde95c4, 0xc82be2c4, 0xc98c95b6, 0xf1fb958d, 0x6f350963, 0x02121287, 0x13b647d9,
            0xf9a62f40, 0x03c6d553, 0x881b480c, 0xb6eb598c, 0x7e4f854b,
        ],
    );
}

#[test]
fn framing() {
    check(
        "framing",
        &[
            0xbde4c2d9, 0xcc11a4b7, 0x2e694798, 0xd4602854, 0x8894c20c, 0x35dbab6e, 0x8a4f7679,
            0x9cb9b20f, 0x0f6c79d2, 0xdded393a, 0x52819218, 0x011764cf, 0xf70edd95, 0xb4e36bd7,
            0xd12bdb6e, 0xfa0d901e, 0x525edb52, 0x8a5e66ac, 0x37734609, 0xb456e845, 0x530c6df8,
            0xeae6b000, 0x1a3f0680, 0x0465938c, 0xb8af88c1, 0x36387c62, 0x47ff8db2, 0x45887fde,
            0x3d55e6fc, 0xde8b6464, 0x6b1750e5, 0x2d21b7ae, 0x3b397032, 0x81c9295c, 0xa8e00034,
            0x3bc1a000, 0xda50c0e9, 0x726b2a47, 0x28114a66, 0x015b0a83,
        ],
    );
}
