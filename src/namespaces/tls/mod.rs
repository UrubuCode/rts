//! `tls` namespace — TLS 1.2/1.3 client sync via rustls (issue #238).
//!
//! Wraps um TcpStream do namespace `net` numa conexao TLS. Trust store
//! e webpki-roots (bundle Mozilla embutido), nao depende do trust
//! store do SO.

pub mod abi;
pub mod client;
pub mod io;
