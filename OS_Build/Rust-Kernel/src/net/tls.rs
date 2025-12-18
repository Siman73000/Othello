#![allow(dead_code)]

//! TLS support placeholder.
//!
//! Native TLS is not implemented yet in-kernel. For now, HTTPS fetching is
//! supported via the optional host-side HTTPS proxy (see tools/https_proxy.py).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsError {
    NotImplemented,
}

pub fn native_tls_supported() -> bool { false }
