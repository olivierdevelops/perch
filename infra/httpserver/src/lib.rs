//! Hosts a loaded perch program behind an HTML UI and a small set of JSON /
//! NDJSON endpoints.
//!
//! Endpoints:
//!   GET  /                   the UI
//!   GET  /api/program        Program metadata (name, version, commands, globals)
//!   POST /api/exec           run a command, stream NDJSON (out/err/status)
//!   POST /api/check          validate the program, return issues
//!   POST /api/scan           static capability + risk audit
//!   POST /api/simulate       simulate a command against a hypothetical env
//!                            (CLI flags via SimEnv; optional fixture JSON body)
mod api;
mod gofmt;
mod gojson;
mod http;
mod template;

use perch_domain::Program;
use perch_interpreter::Handler;
use std::collections::{HashMap, HashSet};
use std::net::TcpListener;
use std::sync::Arc;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Returns the set of known op kinds (used by `/api/check`).
pub type KnownOpsFn = Box<dyn Fn() -> HashSet<String> + Send + Sync>;

/// Holds the wiring to serve a Program. `config_path` and `known_ops` enable the
/// pre-flight endpoints (check, scan, simulate); if `known_ops` is `None`,
/// `/api/check` degrades to "no known-ops table available."
#[derive(Default)]
pub struct Server {
    pub handlers: HashMap<String, Handler>,
    /// Path to the loaded .perch file.
    pub config_path: String,
    /// For validate.
    pub known_ops: Option<KnownOpsFn>,
}

impl Server {
    /// Listens on host:port and serves `p`. `config_path` is the path of the
    /// .perch file that produced `p` — used in the UI header and for context in
    /// pre-flight endpoints. Blocks until the listener fails.
    pub fn serve(&mut self, p: &Program, host: &str, port: i64, config_path: &str) -> Result<(), Error> {
        if !config_path.is_empty() {
            self.config_path = config_path.to_string();
        }
        let shared = Arc::new(api::Shared::new(
            p.clone(),
            self.handlers.clone(),
            self.config_path.clone(),
            self.known_ops.take(),
        ));
        let addr = format!("{host}:{port}");
        let listener = bind(host, port, &addr)?;
        eprintln!("{} perch UI on http://{addr}", chrono::Local::now().format("%Y/%m/%d %H:%M:%S"));
        serve_on(listener, shared)
    }
}

fn bind(host: &str, port: i64, addr: &str) -> Result<TcpListener, Error> {
    let target = if host.is_empty() { format!("0.0.0.0:{port}") } else { addr.to_string() };
    TcpListener::bind(&target).map_err(|e| {
        let why = match e.kind() {
            std::io::ErrorKind::AddrInUse => "bind: address already in use".to_string(),
            std::io::ErrorKind::PermissionDenied => "bind: permission denied".to_string(),
            std::io::ErrorKind::AddrNotAvailable => "bind: cannot assign requested address".to_string(),
            _ => e.to_string(),
        };
        Error::from(format!("listen tcp {addr}: {why}"))
    })
}

fn serve_on(listener: TcpListener, shared: Arc<api::Shared>) -> Result<(), Error> {
    let handler: http::Handler = Arc::new(move |req| api::route(&shared, req));
    http::serve_listener(listener, handler)?;
    Ok(())
}

#[cfg(test)]
mod tests;
