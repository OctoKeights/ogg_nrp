#!/bin/sh
# Compares the Rust port with libogg, the C library.
#
#   compare/compare.sh <folder with libogg's sources> [seeds]
#
# The folder is a checkout of https://github.com/xiph/ogg (the port is of its version 1.3.5).
# Builds compare/dump.c against the library's two source files, runs both dumps of both sides
# for each seed (200 when not given) and compares what they print byte for byte.
ogg=$1
seeds=${2:-200}
if [ ! -f "$ogg/src/framing.c" ]; then
    echo "usage: compare/compare.sh <folder with libogg's sources> [seeds]" >&2
    exit 2
fi
here=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# ogg/config_types.h is what libogg's configure step writes.
mkdir -p "$work/include/ogg"
cat > "$work/include/ogg/config_types.h" <<'TYPES'
#ifndef __CONFIG_TYPES_H__
#define __CONFIG_TYPES_H__
#include <stdint.h>
typedef int16_t ogg_int16_t;
typedef uint16_t ogg_uint16_t;
typedef int32_t ogg_int32_t;
typedef uint32_t ogg_uint32_t;
typedef int64_t ogg_int64_t;
typedef uint64_t ogg_uint64_t;
#endif
TYPES
cc -O1 -w -I"$work/include" -I"$ogg/include" -o "$work/dump" "$here/compare/dump.c" "$ogg/src/framing.c" "$ogg/src/bitwise.c" || exit 2
(cd "$here" && cargo build --release -q --example dump) || exit 2
rust=$(cd "$here" && cargo metadata --format-version 1 --no-deps 2>/dev/null | sed 's/.*"target_directory":"\([^"]*\)".*/\1/')/release/examples/dump

failed=0
for dump in bits framing; do
    differ=0
    seed=1
    while [ "$seed" -le "$seeds" ]; do
        "$work/dump" "$dump" "$seed" > "$work/c.txt"
        "$rust" "$dump" "$seed" > "$work/rust.txt"
        if ! cmp -s "$work/c.txt" "$work/rust.txt"; then
            differ=$((differ + 1))
            [ "$differ" -le 3 ] && echo "$dump $seed differs: $(cmp "$work/c.txt" "$work/rust.txt" 2>&1 | head -1)"
        fi
        seed=$((seed + 1))
    done
    echo "$dump: $seeds seeds, $differ differ"
    [ "$differ" -ne 0 ] && failed=1
done
exit $failed
