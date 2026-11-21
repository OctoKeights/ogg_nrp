/* Prints what libogg does with made-up input, for comparing with the Rust port
   (examples/dump.rs prints the same from the port). See compare.sh.

     dump bits <seed>      the bit packers, both bit orders: written, then read back
     dump framing <seed>   packets into pages, then the bytes (some damaged) back into packets */
#include <ogg/ogg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static unsigned int state;

static unsigned int next(void) {
  state = state * 1664525u + 1013904223u;
  return state >> 8;
}

static unsigned int hash(const unsigned char *data, long size) {
  unsigned int h = 2166136261u;
  long i;
  for (i = 0; i < size; i++) {
    h ^= data[i];
    h *= 16777619u;
  }
  return h;
}

static void dump_bits(void) {
  int round;
  for (round = 0; round < 4; round++) {
    int msb = round & 1;
    oggpack_buffer w;
    unsigned char *copy;
    long size;
    unsigned int count, i;
    oggpack_buffer r;
    if (msb) oggpackB_writeinit(&w); else oggpack_writeinit(&w);
    count = next() % 200;
    count += 20;
    for (i = 0; i < count; i++) {
      unsigned int k = next() % 400;
      unsigned int value = next() << 8;
      value ^= next();
      if (k < 280) {
        int bits = (int)(next() % 33);
        if (msb) oggpackB_write(&w, value, bits); else oggpack_write(&w, value, bits);
      } else if (k < 300) {
        if (msb) oggpackB_writealign(&w); else oggpack_writealign(&w);
      } else if (k < 340) {
        unsigned char source[40];
        unsigned int n = next() % 40;
        unsigned int j;
        long bits;
        for (j = 0; j < n; j++) source[j] = (unsigned char)(next() & 255);
        bits = (long)(next() % (n * 8 + 1));
        if (msb) oggpackB_writecopy(&w, source, bits); else oggpack_writecopy(&w, source, bits);
      } else if (k < 346) {
        long total = msb ? oggpackB_bits(&w) : oggpack_bits(&w);
        long to = (long)(next() % (unsigned int)(total + 1));
        if (msb) oggpackB_writetrunc(&w, to); else oggpack_writetrunc(&w, to);
      } else if (k < 347) {
        if (msb) oggpackB_reset(&w); else oggpack_reset(&w);
      } else if (k < 349) {
        unsigned int odd = next() % 8;
        int bits = odd == 0 ? 33 : odd == 1 ? -1 : 31;
        if (msb) oggpackB_write(&w, value, bits); else oggpack_write(&w, value, bits);
      } else {
        if (msb) oggpackB_write(&w, value, 32); else oggpack_write(&w, value, 32);
      }
      printf("w %u %ld %ld %d\n", k, msb ? oggpackB_bytes(&w) : oggpack_bytes(&w), msb ? oggpackB_bits(&w) : oggpack_bits(&w),
             msb ? oggpackB_writecheck(&w) : oggpack_writecheck(&w));
    }
    size = oggpack_writecheck(&w) ? 0 : oggpack_bytes(&w);
    copy = malloc((size_t)size + 1);
    if (size) memcpy(copy, oggpack_get_buffer(&w), (size_t)size);
    printf("written %ld %08x\n", size, hash(copy, size));
    oggpack_writeclear(&w);

    if (msb) oggpackB_readinit(&r, copy, (int)size); else oggpack_readinit(&r, copy, (int)size);
    count = next() % 200;
    count += 60;
    for (i = 0; i < count; i++) {
      unsigned int k = next() % 100;
      long got = 0;
      if (k < 50) {
        int bits = (int)(next() % 33);
        got = msb ? oggpackB_read(&r, bits) : oggpack_read(&r, bits);
      } else if (k < 60) {
        int bits = (int)(next() % 33);
        got = msb ? oggpackB_look(&r, bits) : oggpack_look(&r, bits);
      } else if (k < 70) {
        got = msb ? oggpackB_look1(&r) : oggpack_look1(&r);
      } else if (k < 78) {
        int bits = (int)(next() % 40);
        if (msb) oggpackB_adv(&r, bits); else oggpack_adv(&r, bits);
      } else if (k < 84) {
        if (msb) oggpackB_adv1(&r); else oggpack_adv1(&r);
      } else if (k < 92) {
        got = msb ? oggpackB_read1(&r) : oggpack_read1(&r);
      } else if (k < 94) {
        got = msb ? oggpackB_look(&r, (next() & 1) ? 33 : -1) : oggpack_look(&r, (next() & 1) ? 33 : -1);
      } else if (k < 95) {
        got = msb ? oggpackB_read(&r, 33) : oggpack_read(&r, 33);
      } else {
        got = msb ? oggpackB_read(&r, 32) : oggpack_read(&r, 32);
      }
      printf("r %u %ld %ld %ld\n", k, got, msb ? oggpackB_bytes(&r) : oggpack_bytes(&r), msb ? oggpackB_bits(&r) : oggpack_bits(&r));
    }
    free(copy);
  }
}

static unsigned char *file;
static long file_size, file_room;

static void file_add(const unsigned char *data, long size) {
  if (file_size + size > file_room) {
    file_room = (file_size + size) * 2 + 4096;
    file = realloc(file, (size_t)file_room);
  }
  memcpy(file + file_size, data, (size_t)size);
  file_size += size;
}

static void print_page(const char *how, ogg_page *page) {
  long i;
  printf("%s ", how);
  for (i = 0; i < page->header_len; i++) printf("%02x", page->header[i]);
  printf(" %ld %08x\n", page->body_len, hash(page->body, page->body_len));
  file_add(page->header, page->header_len);
  file_add(page->body, page->body_len);
}

static void dump_framing(void) {
  ogg_stream_state streams[3];
  int serials[3];
  int ended[3] = {0, 0, 0};
  ogg_int64_t granules[3] = {0, 0, 0};
  unsigned int count, total, p, i;
  ogg_page page;
  ogg_sync_state sync;
  long position;
  unsigned int damage, known;

  count = next() % 3;
  count += 1;
  for (i = 0; i < count; i++) {
    unsigned int serial = next() << 8;
    serial ^= next();
    serial += i; /* never two the same */
    serials[i] = (int)serial;
    ogg_stream_init(&streams[i], serials[i]);
  }
  if (count > 1 && serials[1] == serials[0]) serials[1] += 7;

  /* Packets into pages. */
  total = next() % 60;
  total += 5;
  for (p = 0; p < total; p++) {
    unsigned int s = next() % count;
    unsigned int k = next() % 20;
    unsigned int size, j, fill, how, out;
    unsigned char *data;
    int eos, result;
    ogg_int64_t granule;
    switch (k) {
      case 0: size = 0; break;
      case 1: size = 255; break;
      case 2: size = 254; break;
      case 3: size = 256; break;
      case 4: size = 510; break;
      case 5: size = next() % 200000; break;
      case 7: size = 65025; break;
      case 8: size = next() % 300; size += 65025; break;
      case 6: size = next() % 4; size = (size + 1) * 255; break;
      default: size = next() % 600; break;
    }
    fill = next();
    data = malloc(size + 1);
    for (j = 0; j < size; j++) data[j] = (unsigned char)((fill + j * 7 + (j >> 8)) & 255);
    granules[s] += next() % 1000;
    granule = granules[s];
    if (next() % 9 == 0) granule = -1;
    eos = next() % 15 == 0;
    how = next() % 4;
    out = next() % 10;
    if (ended[s]) {
      free(data);
      continue;
    }
    if (how == 0) {
      ogg_iovec_t iov[3];
      iov[0].iov_base = data;
      iov[0].iov_len = size / 3;
      iov[1].iov_base = data + size / 3;
      iov[1].iov_len = size / 2 - size / 3;
      iov[2].iov_base = data + size / 2;
      iov[2].iov_len = size - size / 2;
      result = ogg_stream_iovecin(&streams[s], iov, 3, eos, granule);
    } else {
      ogg_packet packet;
      memset(&packet, 0, sizeof(packet));
      packet.packet = data;
      packet.bytes = (long)size;
      packet.e_o_s = eos;
      packet.granulepos = granule;
      result = ogg_stream_packetin(&streams[s], &packet);
    }
    free(data);
    if (eos) ended[s] = 1;
    printf("in %u %u %d %lld %d\n", s, size, eos, (long long)granule, result);
    if (out < 6) {
      while (ogg_stream_pageout(&streams[s], &page)) print_page("pageout", &page);
    } else if (out == 6) {
      int nfill = (int)(next() % 8192);
      while (ogg_stream_pageout_fill(&streams[s], &page, nfill)) print_page("pageout_fill", &page);
    } else if (out == 7) {
      if (ogg_stream_flush(&streams[s], &page)) print_page("flush", &page);
    } else if (out == 8) {
      int nfill = (int)(next() % 3000);
      if (ogg_stream_flush_fill(&streams[s], &page, nfill)) print_page("flush_fill", &page);
    }
  }
  for (i = 0; i < count; i++) {
    while (ogg_stream_flush(&streams[i], &page)) print_page("end", &page);
    printf("eos %d\n", ogg_stream_eos(&streams[i]));
    ogg_stream_clear(&streams[i]);
    printf("cleared %d %d\n", ogg_stream_check(&streams[i]), ogg_stream_eos(&streams[i]));
  }

  /* Damage. */
  damage = next() % 9;
  if ((damage == 3 || damage == 4) && file_size > 0) {
    unsigned int flips = next() % 4;
    flips += 1;
    for (i = 0; i < flips; i++) {
      unsigned int at = next() % (unsigned int)file_size;
      file[at] ^= (unsigned char)(1u << (next() % 8));
    }
  } else if ((damage == 5 || damage == 6) && file_size > 0) {
    unsigned int at = next() % (unsigned int)file_size;
    unsigned int length = next() % 300;
    if ((long)(at + length) > file_size) length = (unsigned int)(file_size - at);
    memmove(file + at, file + at + length, (size_t)(file_size - at - length));
    file_size -= length;
  } else if (damage >= 7) {
    unsigned char garbage[100];
    unsigned int at = next() % (unsigned int)(file_size + 1);
    unsigned int length = next() % 100;
    unsigned char *joined = malloc((size_t)file_size + 100);
    for (i = 0; i < length; i++) garbage[i] = (unsigned char)(next() & 255);
    if (length >= 8 && (next() & 1)) memcpy(garbage + 2, "OggS", 4);
    memcpy(joined, file, at);
    memcpy(joined + at, garbage, length);
    memcpy(joined + at + length, file + at, (size_t)(file_size - at));
    free(file);
    file = joined;
    file_size += length;
    file_room = file_size;
  }
  printf("file %ld %08x %u\n", file_size, hash(file, file_size), damage);
  known = count;
  if (count > 1 && next() % 3 == 0) known = count - 1; /* a stream the reader does not know */

  /* The bytes back into packets. */
  for (i = 0; i < count; i++) ogg_stream_init(&streams[i], serials[i]);
  ogg_sync_init(&sync);
  position = 0;
  while (position < file_size) {
    long chunk = (long)(next() % 5000) + 1;
    char *room;
    unsigned int odd = next() % 40;
    if (chunk > file_size - position) chunk = file_size - position;
    room = ogg_sync_buffer(&sync, chunk);
    memcpy(room, file + position, (size_t)chunk);
    printf("wrote %ld %d\n", chunk, ogg_sync_wrote(&sync, chunk));
    position += chunk;
    if (odd == 0) {
      printf("stream reset %d\n", ogg_stream_reset(&streams[0]));
    } else if (odd == 1) {
      printf("sync reset %d\n", ogg_sync_reset(&sync));
    } else if (odd == 2) {
      printf("serial reset %d\n", ogg_stream_reset_serialno(&streams[0], serials[0]));
    }
    for (;;) {
      long got;
      int found = -1;
      if (next() % 8 == 0) {
        got = ogg_sync_pageseek(&sync, &page);
        printf("pageseek %ld\n", got);
      } else {
        got = ogg_sync_pageout(&sync, &page);
        printf("pageout %ld\n", got);
      }
      if (got == 0) break;
      if (got < 0) continue;
      printf("page %d %d %d %d %lld %d %ld %d %ld %ld %08x\n", ogg_page_version(&page), ogg_page_continued(&page) != 0, ogg_page_bos(&page) != 0,
             ogg_page_eos(&page) != 0, (long long)ogg_page_granulepos(&page), ogg_page_serialno(&page), ogg_page_pageno(&page), ogg_page_packets(&page),
             page.header_len, page.body_len, hash(page.body, page.body_len));
      for (i = 0; i < known; i++)
        if (serials[i] == ogg_page_serialno(&page)) found = (int)i;
      if (found < 0) {
        printf("pagein other %d\n", ogg_stream_pagein(&streams[0], &page));
        continue;
      }
      printf("pagein %d %d\n", found, ogg_stream_pagein(&streams[found], &page));
      for (;;) {
        ogg_packet packet;
        int result;
        if (next() % 5 == 0) {
          result = ogg_stream_packetpeek(&streams[found], &packet);
          if (result == 1)
            printf("peek 1 %ld %08x %d %d %lld %lld\n", packet.bytes, hash(packet.packet, packet.bytes), packet.b_o_s != 0, packet.e_o_s != 0,
                   (long long)packet.granulepos, (long long)packet.packetno);
          else
            printf("peek %d\n", result);
        }
        result = ogg_stream_packetout(&streams[found], &packet);
        if (result == 1)
          printf("packet 1 %ld %08x %d %d %lld %lld\n", packet.bytes, hash(packet.packet, packet.bytes), packet.b_o_s != 0, packet.e_o_s != 0,
                 (long long)packet.granulepos, (long long)packet.packetno);
        else
          printf("packet %d\n", result);
        if (result == 0) break;
      }
    }
  }
  for (i = 0; i < count; i++) {
    printf("read eos %d\n", ogg_stream_eos(&streams[i]));
    ogg_stream_clear(&streams[i]);
  }
  ogg_sync_clear(&sync);
  free(file);
}

int main(int argc, char **argv) {
  if (argc < 3) {
    fprintf(stderr, "usage: dump bits|framing <seed>\n");
    return 2;
  }
  state = (unsigned int)strtoul(argv[2], NULL, 10);
  if (strcmp(argv[1], "bits") == 0) dump_bits();
  else if (strcmp(argv[1], "framing") == 0) dump_framing();
  else return 2;
  return 0;
}
