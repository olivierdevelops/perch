//! Bridges the interpreter's stdio sinks/sources into WASI stdio. Writes go
//! straight to the sink (no background task) so wasm output interleaves
//! deterministically with host output and is never lost at exit; reads are
//! lazy (nothing is consumed from stdin until the module reads).
use bytes::Bytes;
use perch_interpreter::{SharedReader, SharedWriter};
use std::io::{Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use wasmtime_wasi::cli::{IsTerminal, StdinStream, StdoutStream};
use wasmtime_wasi_io::async_trait;
use wasmtime_wasi_io::poll::Pollable;
use wasmtime_wasi_io::streams::{InputStream, OutputStream, StreamError};

/// WASI stdout/stderr backed by an interpreter [`SharedWriter`].
pub struct Sink(pub SharedWriter);

impl IsTerminal for Sink {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for Sink {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(SyncAsyncWriter(self.0.clone()))
    }
    fn p2_stream(&self) -> Box<dyn OutputStream> {
        Box::new(SinkStream(self.0.clone()))
    }
}

struct SyncAsyncWriter(SharedWriter);

impl AsyncWrite for SyncAsyncWriter {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Poll::Ready(self.0.write_all(buf).map(|_| buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

struct SinkStream(SharedWriter);

#[async_trait]
impl Pollable for SinkStream {
    async fn ready(&mut self) {}
}

impl OutputStream for SinkStream {
    fn write(&mut self, bytes: Bytes) -> Result<(), StreamError> {
        self.0.write_all(&bytes).map_err(|e| StreamError::LastOperationFailed(e.into()))
    }
    fn flush(&mut self) -> Result<(), StreamError> {
        self.0.flush().map_err(|e| StreamError::LastOperationFailed(e.into()))
    }
    fn check_write(&mut self) -> Result<usize, StreamError> {
        Ok(1 << 20)
    }
}

/// WASI stdin backed by the interpreter's [`SharedReader`]. The process's real
/// stdin uses wasmtime's lazy worker-thread implementation; any other reader
/// (tests, piped ops) is read on demand, blocking like Go's direct reader.
pub struct Source(pub SharedReader);

impl IsTerminal for Source {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdinStream for Source {
    fn async_stream(&self) -> Box<dyn AsyncRead + Send + Sync> {
        if self.0.is_process_stdin() {
            return std::io::stdin().async_stream();
        }
        Box::new(SyncAsyncReader(self.0.clone()))
    }
    fn p2_stream(&self) -> Box<dyn InputStream> {
        if self.0.is_process_stdin() {
            return std::io::stdin().p2_stream();
        }
        Box::new(SourceStream(self.0.clone()))
    }
}

struct SyncAsyncReader(SharedReader);

impl AsyncRead for SyncAsyncReader {
    fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let n = match self.0.read(buf.initialize_unfilled()) {
            Ok(n) => n,
            Err(e) => return Poll::Ready(Err(e)),
        };
        buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

struct SourceStream(SharedReader);

#[async_trait]
impl Pollable for SourceStream {
    async fn ready(&mut self) {}
}

impl InputStream for SourceStream {
    fn read(&mut self, size: usize) -> Result<Bytes, StreamError> {
        let mut buf = vec![0u8; size.clamp(1, 64 * 1024)];
        match self.0.read(&mut buf) {
            Ok(0) => Err(StreamError::Closed),
            Ok(n) => {
                buf.truncate(n);
                Ok(Bytes::from(buf))
            }
            Err(e) => Err(StreamError::LastOperationFailed(e.into())),
        }
    }
}
