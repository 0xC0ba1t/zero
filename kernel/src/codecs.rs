//! Decoders for pictures, animations and sound: PNG (with a DEFLATE
//! decoder), GIF (with LZW, all frames), BMP, PPM/PGM and WAV.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// A picture: 0xAARRGGBB pixels (alpha 255 = opaque).
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

pub struct Animation {
    pub w: usize,
    pub h: usize,
    /// Frames (complete pictures) and how long each one shows, in ms.
    pub frames: Vec<(Vec<u32>, u32)>,
}

pub struct Sound {
    pub rate: u32,
    pub channels: u16,
    /// Samples, interleaved.
    pub samples: Vec<i16>,
}

fn be32(d: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}

fn le16(d: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([d[i], d[i + 1]])
}

fn le32(d: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}

/// Decodes any supported picture format (the first frame of a GIF).
pub fn decode_picture(d: &[u8]) -> Result<Picture, String> {
    if d.len() < 8 {
        return Err(String::from("file too small"));
    }
    if d.starts_with(b"\x89PNG") {
        return png(d);
    }
    if d.starts_with(b"BM") {
        return bmp(d);
    }
    if d.starts_with(b"GIF8") {
        let a = gif(d)?;
        let (px, _) = a.frames.into_iter().next().ok_or_else(|| String::from("no frames"))?;
        return Ok(Picture { w: a.w, h: a.h, px });
    }
    if d.starts_with(b"P6") || d.starts_with(b"P5") {
        return ppm(d);
    }
    Err(String::from("unknown picture format"))
}

// ---------------------------------------------------------------- DEFLATE

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl<'a> Bits<'a> {
    fn need(&mut self, n: u32) -> Result<(), String> {
        while self.nbits < n {
            if self.pos >= self.d.len() {
                return Err(String::from("compressed data ends early"));
            }
            self.bit |= (self.d[self.pos] as u32) << self.nbits;
            self.pos += 1;
            self.nbits += 8;
        }
        Ok(())
    }
    fn get(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Ok(v)
    }
    fn align(&mut self) {
        self.bit = 0;
        self.nbits = 0;
    }
}

struct Huff {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huff {
    fn new(lengths: &[u8]) -> Huff {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Huff { counts, symbols }
    }

    fn decode(&self, b: &mut Bits) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(String::from("bad Huffman code"))
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Decompresses a zlib stream (2 byte header, DEFLATE data).
pub fn zlib(d: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    if d.len() < 2 {
        return Err(String::from("no zlib header"));
    }
    inflate(&d[2..], expected)
}

pub fn inflate(d: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::with_capacity(expected);
    let mut b = Bits { d, pos: 0, bit: 0, nbits: 0 };
    loop {
        let last = b.get(1)?;
        let kind = b.get(2)?;
        match kind {
            0 => {
                b.align();
                if b.pos + 4 > d.len() {
                    return Err(String::from("bad stored block"));
                }
                let len = le16(d, b.pos) as usize;
                b.pos += 4;
                if b.pos + len > d.len() {
                    return Err(String::from("stored block too long"));
                }
                out.extend_from_slice(&d[b.pos..b.pos + len]);
                b.pos += len;
            }
            1 | 2 => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [0u8; 288];
                    for (i, v) in l.iter_mut().enumerate() {
                        *v = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
                    }
                    (Huff::new(&l), Huff::new(&[5u8; 30]))
                } else {
                    let hlit = b.get(5)? as usize + 257;
                    let hdist = b.get(5)? as usize + 1;
                    let hclen = b.get(4)? as usize + 4;
                    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORDER.iter().take(hclen) {
                        cl[o] = b.get(3)? as u8;
                    }
                    let ch = Huff::new(&cl);
                    let mut lens = vec![0u8; hlit + hdist];
                    let mut i = 0;
                    while i < hlit + hdist {
                        let sym = ch.decode(&mut b)?;
                        match sym {
                            0..=15 => {
                                lens[i] = sym as u8;
                                i += 1;
                            }
                            16 => {
                                let prev = if i > 0 { lens[i - 1] } else { return Err(String::from("bad repeat")) };
                                for _ in 0..3 + b.get(2)? {
                                    if i < lens.len() {
                                        lens[i] = prev;
                                        i += 1;
                                    }
                                }
                            }
                            17 => i += 3 + b.get(3)? as usize,
                            _ => i += 11 + b.get(7)? as usize,
                        }
                    }
                    if i > hlit + hdist {
                        return Err(String::from("code lengths overflow"));
                    }
                    (Huff::new(&lens[..hlit]), Huff::new(&lens[hlit..]))
                };
                loop {
                    let sym = lit.decode(&mut b)?;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let s = (sym - 257) as usize;
                        if s >= 29 {
                            return Err(String::from("bad length"));
                        }
                        let len = LEN_BASE[s] as usize + b.get(LEN_EXTRA[s] as u32)? as usize;
                        let ds = dist.decode(&mut b)? as usize;
                        if ds >= 30 {
                            return Err(String::from("bad distance"));
                        }
                        let dd = DIST_BASE[ds] as usize + b.get(DIST_EXTRA[ds] as u32)? as usize;
                        if dd > out.len() {
                            return Err(String::from("distance too far"));
                        }
                        let start = out.len() - dd;
                        for k in 0..len {
                            let v = out[start + k];
                            out.push(v);
                        }
                    }
                }
            }
            _ => return Err(String::from("bad block type")),
        }
        if last == 1 {
            break;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- PNG

fn png(d: &[u8]) -> Result<Picture, String> {
    let mut i = 8;
    let (mut w, mut h, mut depth, mut ctype, mut interlace) = (0usize, 0usize, 0u8, 0u8, 0u8);
    let mut palette: Vec<u32> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut idat: Vec<u8> = Vec::new();
    while i + 8 <= d.len() {
        let len = be32(d, i) as usize;
        let kind = &d[i + 4..i + 8];
        let body = d.get(i + 8..i + 8 + len).ok_or_else(|| String::from("truncated PNG"))?;
        match kind {
            b"IHDR" => {
                w = be32(body, 0) as usize;
                h = be32(body, 4) as usize;
                depth = body[8];
                ctype = body[9];
                interlace = body[12];
            }
            b"PLTE" => palette = body.chunks_exact(3).map(|c| 0xFF00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect(),
            b"tRNS" => trns = body.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + len;
    }
    if w == 0 || h == 0 || w * h > 16_000_000 {
        return Err(String::from("bad PNG size"));
    }
    if interlace != 0 {
        return Err(String::from("interlaced PNGs are not supported"));
    }
    let channels = match ctype {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => return Err(String::from("unknown PNG colour type")),
    };
    if depth != 8 && !(ctype == 3 || ctype == 0) {
        return Err(String::from("only 8-bit PNGs are supported"));
    }
    let bits_pp = channels * depth as usize;
    let stride = (w * bits_pp + 7) / 8;
    let raw = zlib(&idat, (stride + 1) * h)?;
    if raw.len() < (stride + 1) * h {
        return Err(String::from("PNG data too short"));
    }
    let bpp = ((bits_pp + 7) / 8).max(1);
    let mut cur = vec![0u8; stride];
    let mut prev = vec![0u8; stride];
    let mut px = vec![0u32; w * h];
    for y in 0..h {
        let line = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let filter = line[0];
        for x in 0..stride {
            let a = if x >= bpp { cur[x - bpp] as i32 } else { 0 };
            let b = prev[x] as i32;
            let c = if x >= bpp { prev[x - bpp] as i32 } else { 0 };
            let v = line[x + 1] as i32;
            cur[x] = match filter {
                1 => v + a,
                2 => v + b,
                3 => v + (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    v + if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
                }
                _ => v,
            } as u8;
        }
        for x in 0..w {
            let p = match ctype {
                2 => {
                    let o = x * 3;
                    0xFF00_0000 | (cur[o] as u32) << 16 | (cur[o + 1] as u32) << 8 | cur[o + 2] as u32
                }
                6 => {
                    let o = x * 4;
                    (cur[o + 3] as u32) << 24 | (cur[o] as u32) << 16 | (cur[o + 1] as u32) << 8 | cur[o + 2] as u32
                }
                4 => {
                    let g = cur[x * 2] as u32;
                    (cur[x * 2 + 1] as u32) << 24 | g << 16 | g << 8 | g
                }
                _ => {
                    // gray or palette, 1 to 8 bits
                    let bit = x * depth as usize;
                    let byte = cur[bit / 8];
                    let shift = 8 - depth as usize - bit % 8;
                    let v = (byte >> shift) & ((1u16 << depth) - 1) as u8;
                    if ctype == 3 {
                        let mut c = palette.get(v as usize).copied().unwrap_or(0xFF00_0000);
                        if let Some(&a) = trns.get(v as usize) {
                            c = (c & 0xFFFFFF) | (a as u32) << 24;
                        }
                        c
                    } else {
                        let g = (v as u32 * 255) / ((1u32 << depth) - 1);
                        0xFF00_0000 | g << 16 | g << 8 | g
                    }
                }
            };
            px[y * w + x] = p;
        }
        core::mem::swap(&mut cur, &mut prev);
    }
    Ok(Picture { w, h, px })
}

// ---------------------------------------------------------------- BMP, PPM

fn bmp(d: &[u8]) -> Result<Picture, String> {
    if d.len() < 54 {
        return Err(String::from("truncated BMP"));
    }
    let off = le32(d, 10) as usize;
    let w = le32(d, 18) as i32;
    let hh = le32(d, 22) as i32;
    let bpp = le16(d, 28);
    let comp = le32(d, 30);
    if comp != 0 && comp != 3 {
        return Err(String::from("compressed BMPs are not supported"));
    }
    if bpp != 24 && bpp != 32 {
        return Err(String::from("only 24 and 32-bit BMPs are supported"));
    }
    let (w, h, top_down) = (w.unsigned_abs() as usize, hh.unsigned_abs() as usize, hh < 0);
    let bytes = bpp as usize / 8;
    let stride = (w * bytes + 3) & !3;
    let mut px = vec![0u32; w * h];
    for y in 0..h {
        let sy = if top_down { y } else { h - 1 - y };
        let row = off + sy * stride;
        for x in 0..w {
            let o = row + x * bytes;
            if o + 3 > d.len() {
                return Err(String::from("truncated BMP"));
            }
            px[y * w + x] = 0xFF00_0000 | (d[o + 2] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o] as u32;
        }
    }
    Ok(Picture { w, h, px })
}

fn ppm(d: &[u8]) -> Result<Picture, String> {
    let gray = d[1] == b'5';
    let mut fields = Vec::new();
    let mut i = 2;
    while fields.len() < 3 && i < d.len() {
        while i < d.len() && (d[i].is_ascii_whitespace() || d[i] == b'#') {
            if d[i] == b'#' {
                while i < d.len() && d[i] != b'\n' {
                    i += 1;
                }
            }
            i += 1;
        }
        let start = i;
        while i < d.len() && d[i].is_ascii_digit() {
            i += 1;
        }
        fields.push(core::str::from_utf8(&d[start..i]).ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0));
    }
    i += 1;
    let (w, h) = (fields[0], fields[1]);
    let n = if gray { 1 } else { 3 };
    if d.len() < i + w * h * n {
        return Err(String::from("truncated PPM"));
    }
    let px = (0..w * h)
        .map(|k| {
            let o = i + k * n;
            if gray {
                let g = d[o] as u32;
                0xFF00_0000 | g << 16 | g << 8 | g
            } else {
                0xFF00_0000 | (d[o] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o + 2] as u32
            }
        })
        .collect();
    Ok(Picture { w, h, px })
}

// ---------------------------------------------------------------- GIF

pub fn gif(d: &[u8]) -> Result<Animation, String> {
    if d.len() < 13 {
        return Err(String::from("truncated GIF"));
    }
    let w = le16(d, 6) as usize;
    let h = le16(d, 8) as usize;
    if w == 0 || h == 0 || w * h > 8_000_000 {
        return Err(String::from("bad GIF size"));
    }
    let flags = d[10];
    let bg_index = d[11] as usize;
    let mut i = 13;
    let mut global: Vec<u32> = Vec::new();
    if flags & 0x80 != 0 {
        let n = 2usize << (flags & 7);
        for k in 0..n {
            let o = i + k * 3;
            if o + 2 >= d.len() {
                break;
            }
            global.push(0xFF00_0000 | (d[o] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o + 2] as u32);
        }
        i += n * 3;
    }
    let bg = global.get(bg_index).copied().unwrap_or(0xFF00_0000);
    let mut canvas = vec![bg; w * h];
    let mut frames = Vec::new();
    let (mut delay, mut transparent, mut disposal) = (100u32, None::<u8>, 0u8);
    while i < d.len() {
        match d[i] {
            0x21 => {
                let label = d.get(i + 1).copied().unwrap_or(0);
                i += 2;
                if label == 0xF9 && i + 5 < d.len() {
                    let pf = d[i + 1];
                    delay = (le16(d, i + 2) as u32 * 10).max(20);
                    transparent = if pf & 1 != 0 { Some(d[i + 4]) } else { None };
                    disposal = (pf >> 2) & 7;
                }
                while i < d.len() && d[i] != 0 {
                    i += d[i] as usize + 1;
                }
                i += 1;
            }
            0x2C => {
                if i + 10 > d.len() {
                    break;
                }
                let (fx, fy) = (le16(d, i + 1) as usize, le16(d, i + 3) as usize);
                let (fw, fh) = (le16(d, i + 5) as usize, le16(d, i + 7) as usize);
                let lf = d[i + 9];
                i += 10;
                let mut local = Vec::new();
                if lf & 0x80 != 0 {
                    let n = 2usize << (lf & 7);
                    for k in 0..n {
                        let o = i + k * 3;
                        if o + 2 >= d.len() {
                            break;
                        }
                        local.push(0xFF00_0000 | (d[o] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o + 2] as u32);
                    }
                    i += n * 3;
                }
                let interlaced = lf & 0x40 != 0;
                let min_code = *d.get(i).ok_or_else(|| String::from("truncated GIF"))?;
                i += 1;
                let mut data = Vec::new();
                while i < d.len() && d[i] != 0 {
                    let n = d[i] as usize;
                    data.extend_from_slice(&d[i + 1..(i + 1 + n).min(d.len())]);
                    i += n + 1;
                }
                i += 1;
                let indices = lzw(&data, min_code, fw * fh)?;
                let pal = if local.is_empty() { &global } else { &local };
                let before = if disposal == 3 { Some(canvas.clone()) } else { None };
                let rows: Vec<usize> = if interlaced {
                    let mut r = Vec::new();
                    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                        let mut y = start;
                        while y < fh {
                            r.push(y);
                            y += step;
                        }
                    }
                    r
                } else {
                    (0..fh).collect()
                };
                for (k, &idx) in indices.iter().enumerate().take(fw * fh) {
                    if Some(idx) == transparent {
                        continue;
                    }
                    let (lx, ly) = (k % fw, rows.get(k / fw).copied().unwrap_or(k / fw));
                    let (x, y) = (fx + lx, fy + ly);
                    if x < w && y < h {
                        canvas[y * w + x] = pal.get(idx as usize).copied().unwrap_or(0xFF00_0000);
                    }
                }
                frames.push((canvas.clone(), delay));
                match disposal {
                    2 => {
                        for y in fy..(fy + fh).min(h) {
                            for x in fx..(fx + fw).min(w) {
                                canvas[y * w + x] = bg;
                            }
                        }
                    }
                    3 => {
                        if let Some(b) = before {
                            canvas = b;
                        }
                    }
                    _ => {}
                }
                transparent = None;
                if frames.len() >= 600 {
                    break;
                }
            }
            0x3B => break,
            _ => i += 1,
        }
    }
    if frames.is_empty() {
        return Err(String::from("GIF has no pictures"));
    }
    Ok(Animation { w, h, frames })
}

fn lzw(data: &[u8], min_code: u8, max: usize) -> Result<Vec<u8>, String> {
    let min_code = min_code.clamp(2, 11) as u32;
    let clear = 1u32 << min_code;
    let end = clear + 1;
    let mut size = min_code + 1;
    let mut prefix: Vec<u16> = vec![0; 4096];
    let mut suffix: Vec<u8> = vec![0; 4096];
    let mut next = end + 1;
    let mut out = Vec::with_capacity(max);
    let mut prev: Option<u32> = None;
    let (mut bits, mut nbits, mut pos) = (0u32, 0u32, 0usize);
    let mut stack: Vec<u8> = Vec::with_capacity(4096);
    for k in 0..clear {
        suffix[k as usize] = k as u8;
    }
    loop {
        while nbits < size {
            if pos >= data.len() {
                return Ok(out);
            }
            bits |= (data[pos] as u32) << nbits;
            pos += 1;
            nbits += 8;
        }
        let code = bits & ((1 << size) - 1);
        bits >>= size;
        nbits -= size;
        if code == clear {
            size = min_code + 1;
            next = end + 1;
            prev = None;
            continue;
        }
        if code == end || out.len() >= max {
            return Ok(out);
        }
        let first_of = |mut c: u32, prefix: &Vec<u16>, suffix: &Vec<u8>| -> u8 {
            while c > clear {
                c = prefix[c as usize] as u32;
            }
            suffix[c as usize]
        };
        match prev {
            None => {
                out.push(suffix[code as usize]);
            }
            Some(p) => {
                let known = code < next;
                let c = if known { code } else { p };
                stack.clear();
                let mut cur = c;
                while cur > clear && stack.len() < 4096 {
                    stack.push(suffix[cur as usize]);
                    cur = prefix[cur as usize] as u32;
                }
                stack.push(suffix[cur as usize]);
                let first = *stack.last().unwrap();
                out.extend(stack.iter().rev());
                if !known {
                    out.push(first);
                }
                if next < 4096 {
                    prefix[next as usize] = p as u16;
                    suffix[next as usize] = if known { first } else { first_of(p, &prefix, &suffix) };
                    next += 1;
                    if next == (1 << size) && size < 12 {
                        size += 1;
                    }
                }
            }
        }
        prev = Some(code);
    }
}

// ---------------------------------------------------------------- WAV

pub fn wav(d: &[u8]) -> Result<Sound, String> {
    if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Err(String::from("not a WAV file"));
    }
    let mut i = 12;
    let (mut rate, mut channels, mut bits, mut format) = (0u32, 0u16, 0u16, 0u16);
    while i + 8 <= d.len() {
        let id = &d[i..i + 4];
        let len = le32(d, i + 4) as usize;
        let body = &d[i + 8..(i + 8 + len).min(d.len())];
        if id == b"fmt " && body.len() >= 16 {
            format = le16(body, 0);
            channels = le16(body, 2);
            rate = le32(body, 4);
            bits = le16(body, 14);
        } else if id == b"data" {
            if format != 1 {
                return Err(String::from("only plain (PCM) WAV files are supported"));
            }
            let samples = match bits {
                8 => body.iter().map(|&b| ((b as i16) - 128) << 8).collect(),
                16 => body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(),
                _ => return Err(String::from("only 8 and 16-bit WAV files are supported")),
            };
            return Ok(Sound { rate, channels: channels.max(1), samples });
        }
        i += 8 + len + (len & 1);
    }
    Err(String::from("WAV file has no sound"))
}

/// Converts sound to 48 kHz stereo (what the sound card plays).
pub fn to_48k_stereo(s: &Sound) -> Vec<i16> {
    let ch = s.channels.max(1) as usize;
    let frames_in = s.samples.len() / ch;
    if frames_in == 0 || s.rate == 0 {
        return Vec::new();
    }
    let frames_out = (frames_in as u64 * 48000 / s.rate as u64) as usize;
    let mut out = Vec::with_capacity(frames_out * 2);
    let step = ((s.rate as u64) << 16) / 48000;
    let mut pos: u64 = 0;
    for _ in 0..frames_out {
        let i = (pos >> 16) as usize;
        let f = (pos & 0xFFFF) as i32;
        let j = (i + 1).min(frames_in - 1);
        let get = |k: usize, c: usize| s.samples[k * ch + c.min(ch - 1)] as i32;
        for c in 0..2 {
            let v = (get(i, c) * (65536 - f) + get(j, c) * f) >> 16;
            out.push(v as i16);
        }
        pos += step;
    }
    out
}
