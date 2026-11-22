//! Packing variable sized words into an octet stream: the port of libogg's
//! `bitwise.c`.
//!
//! There are two bit orders, as in the C library. `oggpack_*` fills each byte
//! from its least significant bit ([`PackWriter`], [`PackReader`]: what Vorbis
//! uses), and `oggpackB_*` from its most significant bit ([`PackWriterB`],
//! [`PackReaderB`]: what Theora uses). The C library has one `oggpack_buffer`
//! for writing and for reading. Here a writer owns its bytes, and a reader
//! borrows the bytes it reads.

use alloc::{vec, vec::Vec};

const BUFFER_INCREMENT: usize = 256;
const MAX_WRITER_BYTES: usize = (usize::MAX - 7) / 8;

#[allow(clippy::unreadable_literal)] // as libogg writes it
const MASK: [u64; 33] = [
    0x00000000, 0x00000001, 0x00000003, 0x00000007, 0x0000000f, 0x0000001f, 0x0000003f, 0x0000007f,
    0x000000ff, 0x000001ff, 0x000003ff, 0x000007ff, 0x00000fff, 0x00001fff, 0x00003fff, 0x00007fff,
    0x0000ffff, 0x0001ffff, 0x0003ffff, 0x0007ffff, 0x000fffff, 0x001fffff, 0x003fffff, 0x007fffff,
    0x00ffffff, 0x01ffffff, 0x03ffffff, 0x07ffffff, 0x0fffffff, 0x1fffffff, 0x3fffffff, 0x7fffffff,
    0xffffffff,
];

const MASK8B: [u8; 9] = [0x00, 0x80, 0xc0, 0xe0, 0xf0, 0xf8, 0xfc, 0xfe, 0xff];

/// A buffer bits are written to (`oggpack_buffer` after `oggpack_writeinit`).
/// `MSB` picks the bit order: please use the names [`PackWriter`] and
/// [`PackWriterB`].
///
/// As in the C library, a write that cannot be done (more than 32 bits, fewer
/// than none) clears the writer: it then holds nothing, takes no more writes,
/// and [`writecheck`](Self::writecheck) says so.
#[derive(Clone, Debug)]
pub struct PackWriterOf<const MSB: bool> {
    /// The storage: its length is the C library's `storage`. Empty once cleared.
    buffer: Vec<u8>,
    endbyte: usize,
    endbit: u32,
}

/// `oggpack_*`: bits go into each byte from the least significant end.
pub type PackWriter = PackWriterOf<false>;
/// `oggpackB_*`: bits go into each byte from the most significant end.
pub type PackWriterB = PackWriterOf<true>;

impl<const MSB: bool> Default for PackWriterOf<MSB> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const MSB: bool> PackWriterOf<MSB> {
    /// `oggpack_writeinit`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: vec![0; BUFFER_INCREMENT],
            endbyte: 0,
            endbit: 0,
        }
    }

    /// Creates a writer pre-allocated to hold at least `capacity` written bytes
    /// without reallocating.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let storage = capacity.saturating_add(4).max(5);
        Self {
            buffer: vec![0; storage],
            endbyte: 0,
            endbit: 0,
        }
    }

    /// `oggpack_writecheck`: 0 while the writer is good, and -1 once it has been
    /// cleared.
    #[must_use]
    pub fn writecheck(&self) -> i32 {
        if self.buffer.is_empty() { -1 } else { 0 }
    }

    /// `oggpack_writetrunc`: cuts what was written back to `bits` bits. If
    /// `bits` exceeds [`bits`](Self::bits), it is clamped to what has been
    /// written so far.
    pub fn writetrunc(&mut self, bits: usize) {
        if self.buffer.is_empty() {
            return;
        }
        let bits = bits.min(self.bits());
        let bytes = bits >> 3;
        let rem = bits - bytes * 8;
        self.endbit = rem as u32;
        self.endbyte = bytes;
        if let Some(byte) = self.buffer.get_mut(bytes) {
            if MSB {
                *byte &= MASK8B[rem];
            } else {
                *byte &= MASK[rem] as u8;
            }
        }
    }

    /// `oggpack_write`: the low `bits` bits of `value` (0 to 32 of them).
    pub fn write(&mut self, value: u32, bits: i32) {
        if !(0..=32).contains(&bits) {
            self.writeclear();
            return;
        }
        if self.buffer.is_empty() {
            return;
        }
        if self.endbyte >= self.buffer.len().saturating_sub(4) {
            let Some(min_len) = self.endbyte.checked_add(5) else {
                self.writeclear();
                return;
            };
            let Some(grow_len) = self.buffer.len().checked_add(BUFFER_INCREMENT) else {
                self.writeclear();
                return;
            };
            let new_len = grow_len.max(min_len);
            if new_len > MAX_WRITER_BYTES {
                self.writeclear();
                return;
            }
            self.buffer.resize(new_len, 0);
        }
        let at = self.endbyte;
        let endbit = self.endbit;
        let total = bits as u32 + endbit;
        if MSB {
            let value = (u64::from(value) & MASK[bits as usize]) << (32 - bits);
            self.buffer[at] |= (value >> (24 + endbit)) as u8;
            if total >= 8 {
                self.buffer[at + 1] = (value >> (16 + endbit)) as u8;
                if total >= 16 {
                    self.buffer[at + 2] = (value >> (8 + endbit)) as u8;
                    if total >= 24 {
                        self.buffer[at + 3] = (value >> endbit) as u8;
                        if total >= 32 {
                            self.buffer[at + 4] = if endbit != 0 {
                                (value << (8 - endbit)) as u8
                            } else {
                                0
                            };
                        }
                    }
                }
            }
        } else {
            let value = u64::from(value) & MASK[bits as usize];
            self.buffer[at] |= (value << endbit) as u8;
            if total >= 8 {
                self.buffer[at + 1] = (value >> (8 - endbit)) as u8;
                if total >= 16 {
                    self.buffer[at + 2] = (value >> (16 - endbit)) as u8;
                    if total >= 24 {
                        self.buffer[at + 3] = (value >> (24 - endbit)) as u8;
                        if total >= 32 {
                            self.buffer[at + 4] = if endbit != 0 {
                                (value >> (32 - endbit)) as u8
                            } else {
                                0
                            };
                        }
                    }
                }
            }
        }
        self.endbyte += (total / 8) as usize;
        self.endbit = total & 7;
    }

    /// `oggpack_writealign`: zero bits up to the next whole byte.
    pub fn writealign(&mut self) {
        let bits = 8 - self.endbit as i32;
        if bits < 8 {
            self.write(0, bits);
        }
    }

    /// `oggpack_writecopy`: the first `bits` bits of `source`, in this writer's bit
    /// order. A `source` shorter than that clears the writer, as its other errors
    /// do.
    pub fn writecopy(&mut self, source: &[u8], bits: usize) {
        if self.buffer.is_empty() {
            return;
        }
        let bytes = bits / 8;
        let rem_bits = bits - bytes * 8;
        if source.len() < bytes + usize::from(rem_bits != 0) {
            self.writeclear();
            return;
        }
        let Some(pbytes) = (self.endbit as usize).checked_add(bits).map(|n| n / 8) else {
            self.writeclear();
            return;
        };
        let Some(needed_end) = self.endbyte.checked_add(pbytes) else {
            self.writeclear();
            return;
        };
        if needed_end > MAX_WRITER_BYTES {
            self.writeclear();
            return;
        }
        // Expand storage up-front.
        if needed_end >= self.buffer.len().saturating_sub(4) {
            let Some(new_len) = needed_end.checked_add(BUFFER_INCREMENT) else {
                self.writeclear();
                return;
            };
            self.buffer.resize(new_len, 0);
        }
        // Copy whole octets.
        if self.endbit != 0 {
            // Unaligned copy. Do it the hard way.
            for &byte in &source[..bytes] {
                self.write(u32::from(byte), 8);
            }
        } else {
            // Aligned block copy.
            self.buffer[self.endbyte..self.endbyte + bytes].copy_from_slice(&source[..bytes]);
            self.endbyte += bytes;
            self.buffer[self.endbyte] = 0;
        }
        // Copy trailing bits.
        if rem_bits != 0 {
            if MSB {
                self.write(u32::from(source[bytes] >> (8 - rem_bits)), rem_bits as i32);
            } else {
                self.write(u32::from(source[bytes]), rem_bits as i32);
            }
        }
    }

    /// `oggpack_reset`: back to nothing written, keeping the storage.
    pub fn reset(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        self.buffer[0] = 0;
        self.endbit = 0;
        self.endbyte = 0;
    }

    /// `oggpack_writeclear`: frees the storage. The writer takes no more writes.
    pub fn writeclear(&mut self) {
        self.buffer = Vec::new();
        self.endbyte = 0;
        self.endbit = 0;
    }

    /// `oggpack_bytes`: the bytes written, the last one counted when it is begun.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.endbyte + (self.endbit as usize + 7) / 8
    }

    /// `oggpack_bits`: the bits written.
    #[must_use]
    pub fn bits(&self) -> usize {
        self.endbyte * 8 + self.endbit as usize
    }

    /// `oggpack_get_buffer`: what was written, [`bytes`](Self::bytes) long.
    #[must_use]
    pub fn get_buffer(&self) -> &[u8] {
        &self.buffer[..self.bytes().min(self.buffer.len())]
    }

    /// Consumes the writer and returns the written bytes as a [`Vec<u8>`] without
    /// copying, or `None` if the writer has been cleared by an invalid write.
    #[must_use]
    pub fn into_vec(mut self) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            return None;
        }
        let len = self.bytes().min(self.buffer.len());
        self.buffer.truncate(len);
        Some(self.buffer)
    }
}

/// Bytes that bits are read from (`oggpack_buffer` after `oggpack_readinit`).
/// `MSB` picks the bit order: please use the names [`PackReader`] and
/// [`PackReaderB`].
///
/// As in the C library, reading past the end gives -1 and leaves the reader at
/// its end, where every later read gives -1 too.
#[derive(Clone, Debug)]
pub struct PackReaderOf<'a, const MSB: bool> {
    buffer: &'a [u8],
    endbyte: i64,
    endbit: i64,
}

/// `oggpack_*`: bits come out of each byte from the least significant end.
pub type PackReader<'a> = PackReaderOf<'a, false>;
/// `oggpackB_*`: bits come out of each byte from the most significant end.
pub type PackReaderB<'a> = PackReaderOf<'a, true>;

impl<'a, const MSB: bool> PackReaderOf<'a, MSB> {
    /// `oggpack_readinit`.
    #[must_use]
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            endbyte: 0,
            endbit: 0,
        }
    }

    fn storage(&self) -> i64 {
        self.buffer.len() as i64
    }

    fn overflow(&mut self) -> i64 {
        self.endbyte = self.storage();
        self.endbit = 1;
        -1
    }

    fn byte(&self, offset: i64) -> u64 {
        u64::from(self.buffer[(self.endbyte + offset) as usize])
    }

    /// Returns the next `bits` bits (already checked to be there, with `total` =
    /// bits + endbit > 0).
    fn peek(&self, bits: i64, total: i64) -> i64 {
        let endbit = self.endbit;
        if MSB {
            let m = 32 - bits;
            // The C library works in 32 bits here: what is shifted out at the top is lost.
            let mut ret = (self.byte(0) << (24 + endbit)) as u32;
            if total > 8 {
                ret |= (self.byte(1) << (16 + endbit)) as u32;
                if total > 16 {
                    ret |= (self.byte(2) << (8 + endbit)) as u32;
                    if total > 24 {
                        ret |= (self.byte(3) << endbit) as u32;
                        if total > 32 && endbit != 0 {
                            ret |= (self.byte(4) >> (8 - endbit)) as u32;
                        }
                    }
                }
            }
            i64::from((ret >> (m >> 1)) >> ((m + 1) >> 1))
        } else {
            let mut ret = self.byte(0) >> endbit;
            if total > 8 {
                ret |= self.byte(1) << (8 - endbit);
                if total > 16 {
                    ret |= self.byte(2) << (16 - endbit);
                    if total > 24 {
                        ret |= self.byte(3) << (24 - endbit);
                        if total > 32 && endbit != 0 {
                            ret |= self.byte(4) << (32 - endbit);
                        }
                    }
                }
            }
            (ret & MASK[bits as usize]) as i64
        }
    }

    /// `oggpack_look`: the next `bits` bits (0 to 32) without moving on, or -1 when
    /// they are not there.
    #[must_use]
    pub fn look(&self, bits: i32) -> i64 {
        if !(0..=32).contains(&bits) {
            return -1;
        }
        let bits = i64::from(bits);
        let total = bits + self.endbit;
        if self.endbyte >= self.storage() - 4 {
            // Not the main path.
            if self.endbyte > self.storage() - ((total + 7) >> 3) {
                return -1;
            } else if total == 0 {
                return 0;
            }
        }
        self.peek(bits, total)
    }

    /// Peeks at the next `bits` bits (`0..=32`) as a `u32` without advancing, or
    /// returns `None` when `bits > 32` or the remaining buffer is too short.
    #[must_use]
    pub fn try_look(&self, bits: u8) -> Option<u32> {
        let got = self.look(i32::from(bits));
        if got < 0 { None } else { Some(got as u32) }
    }

    /// `oggpack_look1`: the next bit without moving on, or -1 at the end.
    #[must_use]
    pub fn look1(&self) -> i64 {
        if self.endbyte >= self.storage() {
            return -1;
        }
        let shift = if MSB { 7 - self.endbit } else { self.endbit };
        ((self.byte(0) >> shift) & 1) as i64
    }

    /// `oggpack_adv`: moves on by `bits` bits. A negative `bits` marks the reader
    /// as overflowed.
    pub fn adv(&mut self, bits: i32) {
        if bits < 0 {
            self.overflow();
            return;
        }
        let total = i64::from(bits) + self.endbit;
        if self.endbyte > self.storage() - ((total + 7) >> 3) {
            self.overflow();
            return;
        }
        self.endbyte += total / 8;
        self.endbit = total & 7;
    }

    /// `oggpack_adv1`: moves on by one bit, without looking where the end is, as
    /// the C library does not.
    pub fn adv1(&mut self) {
        self.endbit += 1;
        if self.endbit > 7 {
            self.endbit = 0;
            self.endbyte += 1;
        }
    }

    /// `oggpack_read`: the next `bits` bits (0 to 32), or -1 when they are not
    /// there.
    #[must_use]
    pub fn read(&mut self, bits: i32) -> i64 {
        if !(0..=32).contains(&bits) {
            return self.overflow();
        }
        let bits = i64::from(bits);
        let total = bits + self.endbit;
        if self.endbyte >= self.storage() - 4 {
            // Not the main path.
            if self.endbyte > self.storage() - ((total + 7) >> 3) {
                return self.overflow();
            } else if total == 0 {
                return 0;
            }
        }
        let ret = self.peek(bits, total);
        self.endbyte += total / 8;
        self.endbit = total & 7;
        ret
    }

    /// Reads the next `bits` bits (`0..=32`) as a `u32`, or returns `None` (and
    /// marks the reader as overflowed) when `bits > 32` or the remaining buffer
    /// is too short.
    #[must_use]
    pub fn try_read(&mut self, bits: u8) -> Option<u32> {
        let got = self.read(i32::from(bits));
        if got < 0 { None } else { Some(got as u32) }
    }

    /// `oggpack_read1`: the next bit, or -1 at the end.
    #[must_use]
    pub fn read1(&mut self) -> i64 {
        if self.endbyte >= self.storage() {
            return self.overflow();
        }
        let shift = if MSB { 7 - self.endbit } else { self.endbit };
        let ret = ((self.byte(0) >> shift) & 1) as i64;
        self.endbit += 1;
        if self.endbit > 7 {
            self.endbit = 0;
            self.endbyte += 1;
        }
        ret
    }

    /// `oggpack_bytes`: the bytes read, the last one counted when it is begun.
    #[must_use]
    pub fn bytes(&self) -> i64 {
        self.endbyte + (self.endbit + 7) / 8
    }

    /// `oggpack_bits`: the bits read.
    #[must_use]
    pub fn bits(&self) -> i64 {
        self.endbyte * 8 + self.endbit
    }

    /// `oggpack_get_buffer`: the bytes being read.
    #[must_use]
    pub fn get_buffer(&self) -> &'a [u8] {
        self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_orders() {
        let mut w = PackWriter::new();
        let mut b = PackWriterB::new();
        for (value, bits) in [(1u32, 1), (0x2a, 7), (0xdead_beef, 32), (0, 0), (5, 3)] {
            w.write(value, bits);
            b.write(value, bits);
        }
        assert_eq!(w.bits(), 43);
        assert_eq!(w.get_buffer(), [0x55, 0xef, 0xbe, 0xad, 0xde, 0x05]);
        assert_eq!(b.get_buffer(), [0xaa, 0xde, 0xad, 0xbe, 0xef, 0xa0]);

        let mut r = PackReader::new(w.get_buffer());
        let mut rb = PackReaderB::new(b.get_buffer());
        for (value, bits) in [(1i64, 1), (0x2a, 7), (0xdead_beef, 32), (0, 0), (5, 3)] {
            assert_eq!(r.look(bits), value);
            assert_eq!(r.read(bits), value);
            assert_eq!(rb.read(bits), value);
        }
        // Past the end: -1 from then on.
        assert_eq!(r.read(6), -1);
        assert_eq!(r.read1(), -1);
        assert_eq!(rb.read(32), -1);
    }

    #[test]
    fn a_bad_write_clears() {
        let mut w = PackWriter::new();
        w.write(1, 8);
        assert_eq!(w.writecheck(), 0);
        w.write(1, 33);
        assert_eq!(w.writecheck(), -1);
        w.write(1, 8);
        assert_eq!(w.bytes(), 0);
        assert!(w.into_vec().is_none());
    }

    #[test]
    fn adv_negative_marks_overflow_without_panic() {
        let data = [0xa5u8, 0x5a, 0xff, 0x00, 0x12];
        for neg in [-1, -8, -100, i32::MIN] {
            let mut r = PackReader::new(&data);
            assert_eq!(r.read(4), 0x5);
            r.adv(neg);
            assert_eq!(r.look(1), -1);
            assert_eq!(r.look1(), -1);
            assert_eq!(r.read(8), -1);
            assert_eq!(r.read1(), -1);

            let mut rb = PackReaderB::new(&data);
            rb.adv(neg);
            assert_eq!(rb.read(1), -1);
        }
    }

    #[test]
    fn writetrunc_past_written_and_usize_max_never_panics() {
        let mut w = PackWriter::new();
        w.write(0xabcd, 16);
        // Truncating beyond written bits clamps to current written bits (16).
        w.writetrunc(100_000);
        assert_eq!(w.bits(), 16);
        w.writetrunc(usize::MAX);
        assert_eq!(w.bits(), 16);
        // Subsequent writes must remain safe and accurate.
        w.write(0xef, 8);
        assert_eq!(w.bits(), 24);
        assert_eq!(w.get_buffer(), [0xcd, 0xab, 0xef]);

        let mut wb = PackWriterB::new();
        wb.writetrunc(usize::MAX);
        assert_eq!(wb.bits(), 0);
        wb.write(0b101, 3);
        assert_eq!(wb.get_buffer(), [0xa0]);
    }

    #[test]
    fn writecopy_overflow_clears_without_panic() {
        let mut w = PackWriter::new();
        w.write(1, 3); // endbit != 0 so `endbit + usize::MAX` would previously overflow!
        w.writecopy(&[0xff; 8], usize::MAX);
        assert_eq!(w.writecheck(), -1);
        assert_eq!(w.bytes(), 0);
    }

    #[test]
    fn try_read_try_look_and_capacity_into_vec() {
        let mut w = PackWriter::with_capacity(0);
        assert_eq!(w.writecheck(), 0);
        w.write(0xdead_beef, 32);
        w.write(0b101, 3);
        let vec = w.into_vec().unwrap();
        assert_eq!(vec, [0xef, 0xbe, 0xad, 0xde, 0x05]);

        let mut r = PackReader::new(&vec);
        assert_eq!(r.try_look(33), None);
        assert_eq!(r.try_look(32), Some(0xdead_beef));
        assert_eq!(r.try_read(32), Some(0xdead_beef));
        assert_eq!(r.try_read(3), Some(0b101));
        assert_eq!(r.try_look(6), None);
        assert_eq!(r.try_read(5), Some(0)); // remaining 5 zero-pad bits in the 5th byte
        assert_eq!(r.try_look(1), None);
        assert_eq!(r.try_read(1), None);
        // Once overflowed, even 0-bit read returns None.
        assert_eq!(r.try_read(0), None);

        let mut w2 = PackWriterB::with_capacity(64);
        for i in 0..64u32 {
            w2.write(i, 8);
        }
        let out = w2.into_vec().unwrap();
        assert_eq!(out.len(), 64);
        assert_eq!(out[0], 0);
        assert_eq!(out[63], 63);
    }
}
