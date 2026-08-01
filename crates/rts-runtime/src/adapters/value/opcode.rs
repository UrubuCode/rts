//! The ONE encoding for "a namespace static/method reached as a first-class
//! FUNCTION VALUE" (`const k = Object.keys`, `arr.map(Math.abs)`).
//!
//! A namespace that supports the value form declares an ORDERED table of member
//! names and hands it to [`encode`]/[`decode`]. The engine's member-read path
//! resolves a property name to an op code; the namespace's uniform-ABI thunk
//! decodes the same code against the same table. Because both directions go
//! through here, a reordering of the table cannot desynchronize them.
//!
//! ## Why the code is 1-BASED, and why this is the only place that knows it
//!
//! The uniform invoker ([`super::funcops::__rtsadp_fn_invoke`]) reserves
//! `env == 0` as the "this function captures nothing" sentinel and rewrites it to
//! the `undefined` word before the thunk ever sees it. So a 0-based code makes
//! the FIRST entry of any table arrive as a huge index, miss the lookup and
//! return `undefined` — while `typeof` still answers `"function"`.
//!
//! That is not hypothetical: it shipped once, in the `Math` table, where
//! `typeof Math.abs` said `"function"` and `Math.abs(-9)` gave `undefined`.
//! `abs` is simply the alphabetically first member, so it was the only one
//! affected — the kind of bug a spot-check of any other member misses.
//!
//! The rule was then re-derived and re-commented independently in each namespace
//! that adopted the pattern (`Math`, `Object`, …). Four copies of an invariant is
//! four chances for the fifth author to get it wrong, so it lives here now: a
//! namespace adds a table and never restates the off-by-one.
//!
//! **Every table must have a test that exercises its FIRST entry specifically.**
//! It is the entry the reserved-0 rule protects, and the only one whose failure
//! this encoding can still cause.

/// The env-slot op code for `name` within `table`, or `None` when `name` is not a
/// member. The code is the position **plus one** — see the module docs.
pub fn encode(table: &[&str], name: &str) -> Option<i64> {
    table.iter().position(|m| *m == name).map(|i| i as i64 + 1)
}

/// The member `op` denotes within `table`, or `None` when the code is out of
/// range — which includes the reserved `0`, so a stripped env can never be
/// mistaken for the first entry.
pub fn decode(table: &'static [&'static str], op: u64) -> Option<&'static str> {
    table.get((op as usize).checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &[&str] = &["first", "second", "third"];

    /// The invariant the whole module exists for: the FIRST entry must survive a
    /// round trip, because a 0-based encoding is exactly where it would not.
    #[test]
    fn first_entry_round_trips() {
        let op = encode(T, "first").expect("first is a member");
        assert_eq!(op, 1, "the first member must not encode to the reserved 0");
        assert_eq!(decode(T, op as u64), Some("first"));
    }

    #[test]
    fn every_entry_round_trips() {
        for name in T {
            let op = encode(T, name).expect("member");
            assert_eq!(decode(T, op as u64), Some(*name), "round trip for {name}");
        }
    }

    /// A stripped env (the "captures nothing" sentinel) must decode to nothing,
    /// never to the first entry.
    #[test]
    fn reserved_zero_decodes_to_none() {
        assert_eq!(decode(T, 0), None);
    }

    #[test]
    fn out_of_range_decodes_to_none() {
        assert_eq!(decode(T, 4), None);
        assert_eq!(decode(T, u64::MAX), None);
    }

    #[test]
    fn non_member_does_not_encode() {
        assert_eq!(encode(T, "absent"), None);
    }
}
