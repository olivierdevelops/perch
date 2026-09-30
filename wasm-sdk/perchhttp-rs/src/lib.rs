//! Guest-side SDK for perch's host-provided HTTP (port of
//! `wasm-sdk/perchhttp/perchhttp.go`).
//!
//! ```ignore
//! let (body, status) = perchhttp::get("https://api.example.com/health")?;
//! ```
//!
//! The host (perch) must allow the destination via `wasm_allow_host` inside the
//! calling `wasm_run` block; perch also applies the outer HTTP policy (SSRF
//! guard, redirect rules, --allow-host) to every request and redirect hop.
//!
//! Build guests with `cargo build --target wasm32-wasip1`. The raw imports live
//! in the `perch` module and resolve only under `wasm_run`; on other targets
//! this crate compiles but every call returns [`Error::Refused`].
use std::fmt;

/// The host refused the request: no `wasm_allow_host` matched, or the outer
/// HTTPPolicy blocked it. The reason stays host-side by design (a hostile
/// module shouldn't be able to probe the policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Refused,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("perchhttp: refused by host")
    }
}

impl std::error::Error for Error {}

#[cfg(target_arch = "wasm32")]
mod sys {
    #[link(wasm_import_module = "perch")]
    extern "C" {
        pub fn http_get(url_ptr: *const u8, url_len: u32) -> i32;
        pub fn http_status(handle: i32) -> i32;
        pub fn http_body_len(handle: i32) -> i32;
        pub fn http_read_body(handle: i32, dst_ptr: *mut u8, dst_cap: u32) -> i32;
        pub fn http_close(handle: i32) -> i32;
    }
}

/// Off-wasm stand-ins so the crate builds (and tests compile) on the host.
#[cfg(not(target_arch = "wasm32"))]
mod sys {
    pub unsafe fn http_get(_: *const u8, _: u32) -> i32 {
        -1
    }
    pub unsafe fn http_status(_: i32) -> i32 {
        0
    }
    pub unsafe fn http_body_len(_: i32) -> i32 {
        -1
    }
    pub unsafe fn http_read_body(_: i32, _: *mut u8, _: u32) -> i32 {
        -1
    }
    pub unsafe fn http_close(_: i32) -> i32 {
        0
    }
}

/// Fetches `url` via the perch host's HTTP client. Returns the full response
/// body and the status code. The body is read in 32 KB chunks; the host caps
/// the total at 32 MB.
pub fn get(url: &str) -> Result<(Vec<u8>, i32), Error> {
    // SAFETY: pointers are valid for the lengths passed; the host copies/reads
    // within them only.
    unsafe {
        let handle = sys::http_get(url.as_ptr(), url.len() as u32);
        if handle < 0 {
            return Err(Error::Refused);
        }
        let result = read_all(handle);
        sys::http_close(handle);
        result
    }
}

unsafe fn read_all(handle: i32) -> Result<(Vec<u8>, i32), Error> {
    let status = sys::http_status(handle);
    let total = sys::http_body_len(handle);
    if total < 0 {
        return Err(Error::Refused);
    }
    let total = total as usize;
    let mut body = vec![0u8; total];
    const CHUNK: usize = 32 * 1024;
    let mut read = 0usize;
    while read < total {
        let want = (total - read).min(CHUNK);
        let n = sys::http_read_body(handle, body.as_mut_ptr().add(read), want as u32);
        if n <= 0 {
            break;
        }
        read += n as usize;
    }
    body.truncate(read);
    Ok((body, status))
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_stub_refuses() {
        assert_eq!(super::get("http://example.com/"), Err(super::Error::Refused));
    }
}
