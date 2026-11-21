//! Prints what the port does with made-up input, for comparing with libogg
//! (compare/dump.c prints the same from the C library). See compare/compare.sh.
//!
//!     dump bits <seed>      the bit packers, both bit orders: written, then
//!                           read back
//!     dump framing <seed>   packets into pages, then the bytes (some
//!                           damaged) back into packets

#![allow(clippy::manual_is_multiple_of, dead_code)]

use ogg_nrp::bitwise::{PackReaderOf, PackWriterOf};
use ogg_nrp::{Packet, Page, StreamState, SyncState};
use std::fmt::Write as _;

struct Random(u32);

impl Random {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }
}

fn hash(data: &[u8]) -> u32 {
    let mut h = 2_166_136_261u32;
    for &byte in data {
        h ^= u32::from(byte);
        h = h.wrapping_mul(16_777_619);
    }
    h
}

fn bits_round<const MSB: bool>(random: &mut Random, out: &mut String) {
    let mut w = PackWriterOf::<MSB>::new();
    let count = random.next() % 200 + 20;
    for _ in 0..count {
        let k = random.next() % 400;
        let mut value = random.next() << 8;
        value ^= random.next();
        if k < 280 {
            let bits = (random.next() % 33) as i32;
            w.write(value, bits);
        } else if k < 300 {
            w.writealign();
        } else if k < 340 {
            let n = random.next() % 40;
            let source: Vec<u8> = (0..n).map(|_| (random.next() & 255) as u8).collect();
            let bits = (random.next() % (n * 8 + 1)) as usize;
            w.writecopy(&source, bits);
        } else if k < 346 {
            let total = w.bits();
            let to = (random.next() % (total as u32 + 1)) as usize;
            w.writetrunc(to);
        } else if k < 347 {
            w.reset();
        } else if k < 349 {
            let odd = random.next() % 8;
            let bits = match odd {
                0 => 33,
                1 => -1,
                _ => 31,
            };
            w.write(value, bits);
        } else {
            w.write(value, 32);
        }
        writeln!(out, "w {} {} {} {}", k, w.bytes(), w.bits(), w.writecheck()).unwrap();
    }
    let copy = w.get_buffer().to_vec();
    writeln!(out, "written {} {:08x}", copy.len(), hash(&copy)).unwrap();

    let mut r = PackReaderOf::<MSB>::new(&copy);
    let count = random.next() % 200 + 60;
    for _ in 0..count {
        let k = random.next() % 100;
        let mut got = 0;
        if k < 50 {
            let bits = (random.next() % 33) as i32;
            got = r.read(bits);
        } else if k < 60 {
            let bits = (random.next() % 33) as i32;
            got = r.look(bits);
        } else if k < 70 {
            got = r.look1();
        } else if k < 78 {
            let bits = (random.next() % 40) as i32;
            r.adv(bits);
        } else if k < 84 {
            r.adv1();
        } else if k < 92 {
            got = r.read1();
        } else if k < 94 {
            got = r.look(if random.next() & 1 != 0 { 33 } else { -1 });
        } else if k < 95 {
            got = r.read(33);
        } else {
            got = r.read(32);
        }
        writeln!(out, "r {} {} {} {}", k, got, r.bytes(), r.bits()).unwrap();
    }
}

fn dump_bits(random: &mut Random, out: &mut String) {
    for round in 0..4 {
        if round & 1 != 0 {
            bits_round::<true>(random, out);
        } else {
            bits_round::<false>(random, out);
        }
    }
}

fn print_page(how: &str, page: &Page, file: &mut Vec<u8>, out: &mut String) {
    write!(out, "{how} ").unwrap();
    for byte in page.header {
        write!(out, "{byte:02x}").unwrap();
    }
    writeln!(out, " {} {:08x}", page.body.len(), hash(page.body)).unwrap();
    file.extend_from_slice(page.header);
    file.extend_from_slice(page.body);
}

fn print_packet(what: &str, result: i32, packet: Option<Packet>, out: &mut String) {
    match packet {
        Some(packet) if result == 1 => writeln!(
            out,
            "{what} 1 {} {:08x} {} {} {} {}",
            packet.packet.len(),
            hash(packet.packet),
            i32::from(packet.b_o_s != 0),
            i32::from(packet.e_o_s != 0),
            packet.granulepos,
            packet.packetno
        )
        .unwrap(),
        _ => writeln!(out, "{what} {result}").unwrap(),
    }
}

#[allow(clippy::too_many_lines)] // one run of the comparison, as dump.c's
fn dump_framing(random: &mut Random, out: &mut String) {
    let mut file: Vec<u8> = Vec::new();
    let mut serials = [0i32; 3];
    let mut ended = [false; 3];
    let mut granules = [0i64; 3];
    let mut streams: Vec<StreamState> = Vec::new();

    let count = (random.next() % 3 + 1) as usize;
    for (i, slot) in serials.iter_mut().enumerate().take(count) {
        let mut serial = random.next() << 8;
        serial ^= random.next();
        serial = serial.wrapping_add(i as u32);
        *slot = serial as i32;
        streams.push(StreamState::new(*slot));
    }
    if count > 1 && serials[1] == serials[0] {
        serials[1] = serials[1].wrapping_add(7);
    }

    // Packets into pages.
    let total = random.next() % 60 + 5;
    for _ in 0..total {
        let s = (random.next() % count as u32) as usize;
        let k = random.next() % 20;
        let size = match k {
            0 => 0,
            1 => 255,
            2 => 254,
            3 => 256,
            4 => 510,
            5 => random.next() % 200_000,
            7 => 65025,
            8 => random.next() % 300 + 65025,
            6 => (random.next() % 4 + 1) * 255,
            _ => random.next() % 600,
        } as usize;
        let fill = random.next();
        let data: Vec<u8> = (0..size as u32)
            .map(|j| (fill.wrapping_add(j * 7).wrapping_add(j >> 8) & 255) as u8)
            .collect();
        granules[s] += i64::from(random.next() % 1000);
        let mut granule = granules[s];
        if random.next() % 9 == 0 {
            granule = -1;
        }
        let eos = random.next() % 15 == 0;
        let how = random.next() % 4;
        let how_out = random.next() % 10;
        if ended[s] {
            continue;
        }
        let result = if how == 0 {
            streams[s].iovecin(
                &[
                    &data[..size / 3],
                    &data[size / 3..size / 2],
                    &data[size / 2..],
                ],
                eos,
                granule,
            )
        } else {
            streams[s].packetin(&Packet {
                packet: &data,
                e_o_s: i64::from(eos),
                granulepos: granule,
                ..Packet::default()
            })
        };
        if eos {
            ended[s] = true;
        }
        writeln!(
            out,
            "in {} {} {} {} {}",
            s,
            size,
            i32::from(eos),
            granule,
            result
        )
        .unwrap();
        if how_out < 6 {
            while let Some(page) = streams[s].pageout() {
                print_page("pageout", &page, &mut file, out);
            }
        } else if how_out == 6 {
            let nfill = (random.next() % 8192) as i32;
            while let Some(page) = streams[s].pageout_fill(nfill) {
                print_page("pageout_fill", &page, &mut file, out);
            }
        } else if how_out == 7 {
            if let Some(page) = streams[s].flush() {
                print_page("flush", &page, &mut file, out);
            }
        } else if how_out == 8 {
            let nfill = (random.next() % 3000) as i32;
            if let Some(page) = streams[s].flush_fill(nfill) {
                print_page("flush_fill", &page, &mut file, out);
            }
        }
    }
    for stream in &mut streams {
        while let Some(page) = stream.flush() {
            print_page("end", &page, &mut file, out);
        }
        writeln!(out, "eos {}", i32::from(stream.eos())).unwrap();
        stream.clear();
        writeln!(
            out,
            "cleared {} {}",
            stream.check(),
            i32::from(stream.eos())
        )
        .unwrap();
    }

    // Damage.
    let damage = random.next() % 9;
    if (damage == 3 || damage == 4) && !file.is_empty() {
        let flips = random.next() % 4 + 1;
        for _ in 0..flips {
            let at = (random.next() % file.len() as u32) as usize;
            file[at] ^= 1u8 << (random.next() % 8);
        }
    } else if (damage == 5 || damage == 6) && !file.is_empty() {
        let at = (random.next() % file.len() as u32) as usize;
        let mut length = (random.next() % 300) as usize;
        if at + length > file.len() {
            length = file.len() - at;
        }
        file.drain(at..at + length);
    } else if damage >= 7 {
        let at = (random.next() % (file.len() as u32 + 1)) as usize;
        let length = (random.next() % 100) as usize;
        let mut garbage: Vec<u8> = (0..length).map(|_| (random.next() & 255) as u8).collect();
        if length >= 8 && random.next() & 1 != 0 {
            garbage[2..6].copy_from_slice(b"OggS");
        }
        file.splice(at..at, garbage);
    }
    writeln!(out, "file {} {:08x} {}", file.len(), hash(&file), damage).unwrap();
    let mut known = count;
    if count > 1 && random.next() % 3 == 0 {
        known = count - 1; // A stream the reader does not know.
    }

    // The bytes back into packets.
    let mut streams: Vec<StreamState> = serials[..count]
        .iter()
        .map(|&serial| StreamState::new(serial))
        .collect();
    let mut sync = SyncState::new();
    let mut position = 0;
    while position < file.len() {
        let mut chunk = (random.next() % 5000) as usize + 1;
        let odd = random.next() % 40;
        if chunk > file.len() - position {
            chunk = file.len() - position;
        }
        sync.buffer(chunk).unwrap()[..chunk].copy_from_slice(&file[position..position + chunk]);
        writeln!(out, "wrote {} {}", chunk, sync.wrote(chunk)).unwrap();
        position += chunk;
        if odd == 0 {
            writeln!(out, "stream reset {}", streams[0].reset()).unwrap();
        } else if odd == 1 {
            writeln!(out, "sync reset {}", sync.reset()).unwrap();
        } else if odd == 2 {
            writeln!(
                out,
                "serial reset {}",
                streams[0].reset_serialno(serials[0])
            )
            .unwrap();
        }
        loop {
            let (got, page) = if random.next() % 8 == 0 {
                let (got, page) = sync.pageseek();
                writeln!(out, "pageseek {got}").unwrap();
                (got, page)
            } else {
                let (got, page) = sync.pageout();
                writeln!(out, "pageout {got}").unwrap();
                (i64::from(got), page)
            };
            if got == 0 {
                break;
            }
            let Some(page) = page else { continue };
            writeln!(
                out,
                "page {} {} {} {} {} {} {} {} {} {} {:08x}",
                page.version(),
                i32::from(page.continued()),
                i32::from(page.bos()),
                i32::from(page.eos()),
                page.granulepos(),
                page.serialno(),
                page.pageno(),
                page.packets(),
                page.header.len(),
                page.body.len(),
                hash(page.body)
            )
            .unwrap();
            let Some(found) = serials[..known]
                .iter()
                .rposition(|&serial| serial == page.serialno())
            else {
                writeln!(out, "pagein other {}", streams[0].pagein(&page)).unwrap();
                continue;
            };
            writeln!(out, "pagein {} {}", found, streams[found].pagein(&page)).unwrap();
            loop {
                if random.next() % 5 == 0 {
                    let (result, packet) = streams[found].packetpeek();
                    print_packet("peek", result, packet, out);
                }
                let (result, packet) = streams[found].packetout();
                print_packet("packet", result, packet, out);
                if result == 0 {
                    break;
                }
            }
        }
    }
    for stream in &streams {
        writeln!(out, "read eos {}", i32::from(stream.eos())).unwrap();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump bits|framing <seed>");
        std::process::exit(2);
    }
    match run(&args[1], args[2].parse().unwrap_or(0)) {
        Some(out) => print!("{out}"),
        None => std::process::exit(2),
    }
}

/// Returns what `dump <what> <seed>` prints.
#[must_use]
pub fn run(what: &str, seed: u32) -> Option<String> {
    let mut random = Random(seed);
    let mut out = String::new();
    match what {
        "bits" => dump_bits(&mut random, &mut out),
        "framing" => dump_framing(&mut random, &mut out),
        _ => return None,
    }
    Some(out)
}

/// Returns the hash the dumps use, for tests/golden.rs.
#[must_use]
pub fn hash_of(text: &str) -> u32 {
    hash(text.as_bytes())
}
