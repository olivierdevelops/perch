//! gzip / ungzip (compression.go) plus the gzip-header check shared with the
//! archive and bundle readers.
use crate::group_b::util::*;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use perch_interpreter::{err, handler, Bindings, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};

type Args<'a> = perch_interpreter::Args<'a>;

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert("gzip".into(), handler(op_gzip));
    m.insert("ungzip".into(), handler(op_ungzip));
}

/// `gzip.NewReader`: validates the header up front so failures carry Go's text
/// (`EOF` for empty input, `gzip: invalid header` for a bad magic).
pub fn gzip_reader<R: Read>(r: R) -> Result<MultiGzDecoder<BufReader<R>>> {
    let mut br = BufReader::new(r);
    let head = br.fill_buf().map_err(|e| err(go_io_msg(&e)))?;
    if head.is_empty() {
        return Err(err("EOF"));
    }
    if head[0] != 0x1f || (head.len() > 1 && head[1] != 0x8b) {
        return Err(err("gzip: invalid header"));
    }
    if head.len() < 10 {
        return Err(err("unexpected EOF"));
    }
    Ok(MultiGzDecoder::new(br))
}

/// Maps a decompression read error to Go's wording.
pub fn gz_read_err(e: &std::io::Error) -> perch_interpreter::Error {
    match e.kind() {
        std::io::ErrorKind::UnexpectedEof => err("unexpected EOF"),
        _ => err(go_io_msg(e)),
    }
}

fn op_gzip(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src"]), b);
    let dst = resolve(&arg_string(a, &["dst"]), b);
    let mut inp = open_file(&src)?;
    let out = create_file(&dst)?;
    let mut gz = GzEncoder::new(out, Compression::default());
    copy_stream(&mut gz, &mut inp, &src)?;
    let _ = gz.finish();
    Ok(Value::Null)
}

fn op_ungzip(_i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let src = resolve(&arg_string(a, &["src"]), b);
    let dst = resolve(&arg_string(a, &["dst"]), b);
    let inp = open_file(&src)?;
    let mut gz = gzip_reader(inp)?;
    let mut out = create_file(&dst)?;
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        let n = match gz.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return Err(gz_read_err(&e)),
        };
        std::io::Write::write_all(&mut out, &buf[..n]).map_err(|e| err(format!("write: {}", go_io_msg(&e))))?;
    }
    Ok(Value::Null)
}
