//! Shared `hickory-resolver` client infrastructure — the private Tokio
//! runtime, the process-wide (OS-configured) resolver singleton, and the
//! Node error-code mapping every DNS-protocol query in this module goes
//! through: the module-level `resolve*`/`resolve`/`reverse` natives (each
//! keyed to the OS's own configured servers, [`resolver`]) and each
//! `dns.Resolver` instance (each keyed to its own list, [`build_resolver`]).
//!
//! Split out of `resolve.rs` once decoding grew past `resolve4` alone — this
//! is the part every record-type decoder needs and none of them owns.

use hickory_resolver::TokioResolver;
use hickory_resolver::config::{ConnectionConfig, NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::net::NetError;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::runtime::Runtime;

/// The private runtime every `hickory-resolver` call in this module drives
/// through — `hickory-resolver` only offers `.await`-shaped lookups, and this
/// module's own contract (see the crate's `mod.rs` doc, "Synchronous, wearing
/// a callback") is that a caller's `dns.resolve4(host, cb)` has already
/// called `cb` by the time the native returns. Current-thread: nothing here
/// runs concurrently with itself, and a caller already blocks until the one
/// query settles.
pub(super) fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("building a current-thread Tokio runtime for node:dns")
    })
}

/// The process-wide resolver every module-level `resolve*`/`resolve`/
/// `reverse` native queries through — built once from the OS's own
/// configuration (`/etc/resolv.conf` on Unix, the resolver the Windows
/// registry names — `hickory-resolver`'s `system-config` feature). A build
/// failure is cached and reported to every subsequent caller rather than
/// retried silently.
///
/// NOT consulted: `dns.setServers()` — see `state.rs`'s [`super::state::DnsState`]
/// doc for why, the same divergence real Node has between the module
/// functions and a `dns.Resolver` instance (which DOES honor its own
/// `setServers()`, through [`build_resolver`] below).
pub(super) fn resolver() -> Result<&'static TokioResolver, NetError> {
    static RESOLVER: OnceLock<Result<TokioResolver, NetError>> = OnceLock::new();
    RESOLVER
        .get_or_init(|| runtime().block_on(async { TokioResolver::builder_tokio()?.build() }))
        .as_ref()
        .map_err(Clone::clone)
}

/// The Node error code closest to what `err` represents.
/// `ENOTFOUND` for "the name does not exist"/"no records exist for it", the
/// two cases a program's `err.code === 'ENOTFOUND'` check means to catch,
/// `ETIMEOUT` for a query that never got an answer, and `ESERVFAIL` as the
/// named fallback for every other protocol failure (malformed response,
/// refused query, transport error) — matching what real Node's `query*`
/// family reports when the failure is not one of the first two shapes.
pub(super) fn node_code(err: &NetError) -> &'static str {
    if err.is_nx_domain() || err.is_no_records_found() {
        "ENOTFOUND"
    } else if matches!(err, NetError::Timeout) {
        "ETIMEOUT"
    } else {
        "ESERVFAIL"
    }
}

/// Builds a resolver scoped to its OWN server list — what makes
/// `dns.Resolver` a real independently-configured resolver rather than a
/// second name for the process-wide one in [`resolver`] above (see
/// `resolver_class.rs`'s module doc for why an inert `setServers()` was not
/// acceptable there). `servers` entries are the `ip`/`ip:port`/`[ipv6]`/
/// `[ipv6]:port` shapes [`super::common::parse_server_addr`] already parses
/// for `state.rs::set_servers`'s validation; an unparseable one is skipped
/// here rather than failing the whole build — the same "no throw available
/// mid-list" posture that function already takes.
///
/// An empty `servers` list builds from the OS configuration, same as
/// [`resolver`] — a fresh `new dns.Resolver()` that has never called
/// `setServers()` queries the system's own servers in real Node, and this is
/// the closest this crate can come without discovering that list itself (see
/// `state.rs`'s doc on why `getServers()` cannot report it). In that case
/// `local_v4`/`local_v6` go UNAPPLIED — the system config is an already-built
/// opaque `ResolverConfig` this function does not re-derive a per-server
/// `ConnectionConfig` from, so there is nothing here to attach a bind address
/// to; `resolver_class.rs`'s `setLocalAddress` doc names this as the one case
/// its own binding stays inert, the same "no throw available" posture named
/// rather than silently doing nothing.
pub(super) fn build_resolver(servers: &[String], timeout_ms: Option<u32>, tries: Option<u32>, local_v4: Option<IpAddr>, local_v6: Option<IpAddr>) -> Result<TokioResolver, NetError> {
    if servers.is_empty() {
        return TokioResolver::builder_tokio()?.build();
    }
    let name_servers: Vec<NameServerConfig> = servers
        .iter()
        .filter_map(|entry| super::common::parse_server_addr(entry))
        .map(|(ip, port)| name_server_config(ip, port, local_v4, local_v6))
        .collect();
    let config = ResolverConfig::from_parts(None, Vec::new(), name_servers);
    let mut options = ResolverOpts::default();
    if let Some(ms) = timeout_ms {
        options.timeout = Duration::from_millis(u64::from(ms));
    }
    if let Some(tries) = tries {
        options.attempts = tries as usize;
    }
    let provider = TokioRuntimeProvider::new();
    TokioResolver::builder_with_config(config, provider).with_options(options).build()
}

/// One configured server, over both UDP and TCP (real Node's c-ares backend
/// tries both too) at its explicit port, or the standard port 53 when the
/// server string carried none — bound to `local_v4`/`local_v6` (whichever
/// matches the server's own address family) when
/// `dns.Resolver#setLocalAddress` named one.
fn name_server_config(ip: IpAddr, port: Option<u16>, local_v4: Option<IpAddr>, local_v6: Option<IpAddr>) -> NameServerConfig {
    let mut udp = ConnectionConfig::udp();
    let mut tcp = ConnectionConfig::tcp();
    if let Some(port) = port {
        udp.port = port;
        tcp.port = port;
    }
    let bind = match ip {
        IpAddr::V4(_) => local_v4,
        IpAddr::V6(_) => local_v6,
    };
    if let Some(local) = bind {
        let bind_addr = SocketAddr::new(local, 0);
        udp.bind_addr = Some(bind_addr);
        tcp.bind_addr = Some(bind_addr);
    }
    NameServerConfig::new(ip, true, vec![udp, tcp])
}
