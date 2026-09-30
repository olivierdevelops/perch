//! Shared IO sinks. Go's `io.Writer` fields are freely shared across goroutines;
//! these wrappers give the interpreter the same shape (cheap clone, `&self` writes).
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, MutexGuard};

/// A cloneable, thread-safe output sink (stdout, stderr, or a test buffer).
#[derive(Clone)]
pub struct SharedWriter {
    inner: Arc<Mutex<Box<dyn Write + Send>>>,
    kind: StdKind,
}

/// Which process stream (if any) a [`SharedWriter`] wraps, so subprocess ops can
/// hand the child the real fd (Go passes `*os.File` straight through).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdKind {
    Stdout,
    Stderr,
    Other,
}

impl SharedWriter {
    pub fn new(w: Box<dyn Write + Send>) -> Self {
        SharedWriter { inner: Arc::new(Mutex::new(w)), kind: StdKind::Other }
    }
    pub fn stdout() -> Self {
        SharedWriter { kind: StdKind::Stdout, ..Self::new(Box::new(std::io::stdout())) }
    }
    pub fn stderr() -> Self {
        SharedWriter { kind: StdKind::Stderr, ..Self::new(Box::new(std::io::stderr())) }
    }
    /// The process stream this writer wraps, or `Other`.
    pub fn std_kind(&self) -> StdKind {
        self.kind
    }
    /// Discards everything (Go's `io.Discard`).
    pub fn discard() -> Self {
        Self::new(Box::new(std::io::sink()))
    }
    /// Writes all bytes and flushes (Go writers are unbuffered).
    pub fn write_all(&self, buf: &[u8]) -> std::io::Result<()> {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.write_all(buf)?;
        g.flush()
    }
    pub fn write_str(&self, s: &str) -> std::io::Result<()> {
        self.write_all(s.as_bytes())
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).flush()
    }
}

/// A cloneable, thread-safe input source.
#[derive(Clone)]
pub struct SharedReader {
    inner: Arc<Mutex<Box<dyn Read + Send>>>,
    is_stdin: bool,
}

impl SharedReader {
    pub fn new(r: Box<dyn Read + Send>) -> Self {
        SharedReader { inner: Arc::new(Mutex::new(r)), is_stdin: false }
    }
    pub fn stdin() -> Self {
        SharedReader { is_stdin: true, ..Self::new(Box::new(std::io::stdin())) }
    }
    /// True when this is the process's real stdin (subprocesses can inherit it).
    pub fn is_process_stdin(&self) -> bool {
        self.is_stdin
    }
    pub fn empty() -> Self {
        Self::new(Box::new(std::io::empty()))
    }
    /// Exclusive access to the underlying reader.
    pub fn lock(&self) -> MutexGuard<'_, Box<dyn Read + Send>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Read for SharedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.lock().read(buf)
    }
}

/// In-memory capture buffer; hand `writer()` to an interpreter, read back with
/// `contents()`. Mostly for tests.
#[derive(Clone, Default)]
pub struct SharedBuf(Arc<Mutex<Vec<u8>>>);

struct BufWriter(Arc<Mutex<Vec<u8>>>);

impl Write for BufWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuf {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn writer(&self) -> SharedWriter {
        SharedWriter::new(Box::new(BufWriter(self.0.clone())))
    }
    pub fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(|e| e.into_inner())).into_owned()
    }
}
