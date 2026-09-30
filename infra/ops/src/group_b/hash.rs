//! Hash ops (hash.go).
use crate::group_b::util::*;
use perch_interpreter::{handler, Args, Bindings, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;

type HashFn = fn(&mut dyn Read) -> std::io::Result<String>;

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert("md5".into(), string_hash(md5_hex));
    m.insert("sha1".into(), string_hash(sha1_hex));
    m.insert("sha256".into(), string_hash(sha256_hex));
    m.insert("md5_file".into(), file_hash(md5_hex));
    m.insert("sha1_file".into(), file_hash(sha1_hex));
    m.insert("sha256_file".into(), file_hash(sha256_hex));
    m.insert(
        "crc32".into(),
        pure(|a| Ok(Value::String(format!("{:08x}", crc32fast::hash(arg_string(a, &["value", "_0"]).as_bytes()))))),
    );
    // verify_sha256 PATH HASH -> bool
    m.insert(
        "verify_sha256".into(),
        handler(|_i: &Interpreter, b: &mut Bindings, a: &Args<'_>| -> Result<Value> {
            let path = resolve(&arg_string(a, &["path", "_0"]), b);
            let expected = arg_string(a, &["hash", "_1"]);
            let Ok(mut f) = std::fs::File::open(&path) else { return Ok(Value::Bool(false)) };
            let got = sha256_hex(&mut f).map_err(|e| perch_interpreter::err(format!("read {path}: {}", go_io_msg(&e))))?;
            Ok(Value::Bool(got == expected))
        }),
    );
}

fn string_hash(f: HashFn) -> Handler {
    handler(move |_i, _b, a| {
        let s = arg_string(a, &["value", "_0"]);
        Ok(Value::String(f(&mut s.as_bytes()).unwrap_or_default()))
    })
}

fn file_hash(f: HashFn) -> Handler {
    handler(move |_i, b, a| {
        let p = resolve(&arg_string(a, &["path", "_0"]), b);
        let mut file = open_file(&p)?;
        let h = f(&mut file).map_err(|e| perch_interpreter::err(format!("read {p}: {}", go_io_msg(&e))))?;
        Ok(Value::String(h))
    })
}

fn digest_reader<D: sha1::Digest>(r: &mut dyn Read) -> std::io::Result<String> {
    let mut h = D::new();
    let mut buf = [0u8; 32 * 1024];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

fn sha1_hex(r: &mut dyn Read) -> std::io::Result<String> {
    digest_reader::<sha1::Sha1>(r)
}

fn sha256_hex(r: &mut dyn Read) -> std::io::Result<String> {
    digest_reader::<sha2::Sha256>(r)
}

// ── MD5 (RFC 1321) ────────────────────────────────────────────────────────

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4,
    11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

fn md5_block(state: &mut [u32; 4], block: &[u8]) {
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let m: Vec<u32> = block.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let f2 = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(f2.rotate_left(S[i]));
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

fn md5_hex(r: &mut dyn Read) -> std::io::Result<String> {
    let mut state = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut buf = [0u8; 64 * 512];
    let mut pending: Vec<u8> = Vec::new();
    let mut total: u64 = 0;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        pending.extend_from_slice(&buf[..n]);
        let whole = pending.len() / 64 * 64;
        for blk in pending[..whole].chunks_exact(64) {
            md5_block(&mut state, blk);
        }
        pending.drain(..whole);
    }
    pending.push(0x80);
    while pending.len() % 64 != 56 {
        pending.push(0);
    }
    pending.extend_from_slice(&(total.wrapping_mul(8)).to_le_bytes());
    for blk in pending.chunks_exact(64) {
        md5_block(&mut state, blk);
    }
    let mut out = Vec::with_capacity(16);
    for w in state {
        out.extend_from_slice(&w.to_le_bytes());
    }
    Ok(hex::encode(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(f: HashFn, s: &str) -> String {
        f(&mut s.as_bytes()).unwrap()
    }

    #[test]
    fn known_vectors() {
        assert_eq!(h(md5_hex, ""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(h(md5_hex, "abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(h(md5_hex, &"a".repeat(1000)), "cabe45dcc9ae5b66ba86600cca6b8ba8");
        assert_eq!(h(sha1_hex, "abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(h(sha256_hex, "abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(format!("{:08x}", crc32fast::hash(b"hello")), "3610a686");
    }
}
