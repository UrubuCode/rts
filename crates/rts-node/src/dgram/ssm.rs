//! `socket.addSourceSpecificMembership`/`dropSourceSpecificMembership` —
//! validation only. See the module doc's "Not implemented" list for why the
//! real IGMPv3/MLDv2 join is not attempted from here.
//!
//! # Reuse-check
//!
//! `std::net::UdpSocket` has no source-specific join at all (only the plain
//! `join_multicast_v4`/`v6` [`super::membership`] already uses), so there is
//! no existing call to reuse. A real join needs a raw `setsockopt` with
//! `IP_ADD_SOURCE_MEMBERSHIP`/`MCAST_JOIN_SOURCE_GROUP`, whose `struct
//! ip_mreq_source` field ORDER differs between Linux and Windows — exactly the
//! "answer that runs but is wrong" class CLAUDE.md's honesty floor ranks
//! worse than an absence, and neither success path is exercised by any
//! fixture in this crate's suite, so nothing here would catch a wrong layout.
//! What IS real: the argument validation Node performs before it ever reaches
//! the syscall — parsing the source and group in the socket's own family, and
//! refusing a mismatch between the two — which is what
//! `tests/node_dgram_options.test.ts`'s two SSM cases assert.
//!
//! # What a caller offering VALID input sees
//!
//! `Error [ENOSYS]`, not a silent no-op. A method that validated real input
//! and then quietly did nothing would be the hollow-name case CLAUDE.md
//! refuses; a loud, distinct failure past validation says the join itself is
//! unimplemented rather than pretending to have joined.

use rts_core::entry;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Validates `(source, group)` against the socket's family, raising `EINVAL`
/// under `syscall`'s name (`"addSourceSpecificMembership"` or
/// `"dropSourceSpecificMembership"`) and answering `false` on any mismatch —
/// a malformed address in either argument, or the two parsing in different
/// families. `true` means the input was well-formed; the caller still owes an
/// answer for what happens next (see this module's own doc: not a real join).
pub(super) fn validate(syscall: &str, is_udp6: bool, source: u64, group: u64) -> bool {
    let Some(source_text) = entry::text_of(source) else {
        crate::errors::system_error(syscall, "EINVAL");
        return false;
    };
    let Some(group_text) = entry::text_of(group) else {
        crate::errors::system_error(syscall, "EINVAL");
        return false;
    };
    let ok = if is_udp6 {
        source_text.parse::<Ipv6Addr>().is_ok() && group_text.parse::<Ipv6Addr>().is_ok()
    } else {
        source_text.parse::<Ipv4Addr>().is_ok() && group_text.parse::<Ipv4Addr>().is_ok()
    };
    if !ok {
        crate::errors::system_error(syscall, "EINVAL");
    }
    ok
}
