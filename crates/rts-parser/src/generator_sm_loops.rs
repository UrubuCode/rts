//! `break` / `continue` as STATE TARGETS for the generator state machine.
//!
//! ## Why this is not "just relax the gate"
//!
//! `generator_sm::try_build` used to refuse ANY body containing `break` or
//! `continue`. The comment there records why, and it is worth repeating because
//! the failure mode is silent: the machine turns a loop body into STATES, so a
//! `break` emitted verbatim inside a state has no loop to leave. The machine
//! ignored the cut and the generator ran FOREVER — no error, just a hang. A
//! silent infinite loop is worse than an honest bail.
//!
//! So the gate is replaced by MODELLING, not by removal. Every loop the machine
//! splits into states pushes a [`LoopTarget`] recording the two states a jump
//! can land on:
//!
//! * `brk`  — the loop's `after` state (`break`);
//! * `cont` — the state that re-tests the condition (`continue`). For `while`
//!   that is the header; for `for` it is a DEDICATED state holding the update
//!   expression, created before the body so `continue` still runs the update.
//!
//! `break`/`continue` then lower to an ordinary `Trans::Goto`, which is exactly
//! what the machine already uses for structured control flow.
//!
//! ## Labels
//!
//! `L: while (…) { … break L; }` resolves by scanning the stack from the top for
//! a matching label instead of taking the innermost entry, so a labelled jump
//! out of a NESTED loop lands on the right `after`.
//!
//! ## The try-region rule (an explicit bail, not a guess)
//!
//! A jump that leaves a `try` region must run that region's `finally` first, and
//! the machine's finally is driven by `ENTER_TRY`/`END_FINALLY` around a state
//! path — a raw `Goto` straight to the loop exit would SKIP it. Rather than
//! emit code that silently drops a `finally`, a target entered at a shallower
//! try depth than the jump is REFUSED ([`resolve`] returns `None`), which aborts
//! the build and falls back to the eager buffer — the honest outcome.
//!
//! Modelling it properly means routing the jump through each intervening
//! finally with the pending-completion machinery (`pending_kind`/`pending_val`
//! already exist in `GenStateData`), and is a later increment.

/// What kind of construct a [`LoopTarget`] describes. A `switch` is a `break`
/// target but NOT a `continue` target — `continue` inside a switch belongs to
/// the enclosing LOOP, and treating the switch as the innermost target would
/// send it to the switch's exit and silently turn a re-iteration into a fall-out.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TargetKind {
    Loop,
    Switch,
}

/// One enclosing loop the machine has split into states.
pub struct LoopTarget {
    /// Loop or switch — see [`TargetKind`].
    pub kind: TargetKind,
    /// Label on the loop statement (`L: while …`), if any.
    pub label: Option<String>,
    /// State a `break` jumps to (the loop's `after`).
    pub brk: usize,
    /// State a `continue` jumps to (`while` header / `for` update state).
    pub cont: usize,
    /// `try`-region nesting depth at the point the loop was ENTERED. A jump from
    /// a deeper depth would have to cross a `finally` — see the module docs.
    pub try_depth: usize,
}

/// The state a `break`/`continue` should jump to, or `None` when the jump is not
/// modelled and the caller must abort the build.
///
/// `None` covers three honest refusals:
/// * no enclosing state-split loop (e.g. a `break` belonging to a `switch`,
///   which this machine does not lower);
/// * a label naming no enclosing loop;
/// * a jump that would cross a `try` region (see the module docs).
pub fn resolve(
    stack: &[LoopTarget],
    label: Option<&str>,
    is_break: bool,
    try_depth: usize,
) -> Option<usize> {
    // A `continue` never resolves to a `switch`: only a LOOP can be re-entered.
    let eligible = |t: &&LoopTarget| is_break || t.kind == TargetKind::Loop;
    let target = match label {
        Some(l) => stack
            .iter()
            .rev()
            .find(|t| t.label.as_deref() == Some(l) && eligible(t))?,
        None => stack.iter().rev().find(eligible)?,
    };
    if target.try_depth < try_depth {
        return None;
    }
    Some(if is_break { target.brk } else { target.cont })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(label: Option<&str>, brk: usize, cont: usize, try_depth: usize) -> LoopTarget {
        LoopTarget {
            kind: TargetKind::Loop,
            label: label.map(str::to_string),
            brk,
            cont,
            try_depth,
        }
    }

    #[test]
    fn innermost_wins_without_label() {
        let s = vec![t(None, 1, 2, 0), t(None, 3, 4, 0)];
        assert_eq!(resolve(&s, None, true, 0), Some(3));
        assert_eq!(resolve(&s, None, false, 0), Some(4));
    }

    #[test]
    fn label_selects_the_outer_loop() {
        let s = vec![t(Some("L"), 1, 2, 0), t(None, 3, 4, 0)];
        assert_eq!(resolve(&s, Some("L"), true, 0), Some(1));
        assert_eq!(resolve(&s, Some("L"), false, 0), Some(2));
    }

    #[test]
    fn no_loop_or_unknown_label_refuses() {
        assert_eq!(resolve(&[], None, true, 0), None);
        let s = vec![t(Some("L"), 1, 2, 0)];
        assert_eq!(resolve(&s, Some("OUTRO"), true, 0), None);
    }

    /// The jump would leave a `try` whose `finally` must still run — refuse
    /// rather than emit a `Goto` that skips it.
    #[test]
    fn crossing_a_try_region_refuses() {
        let s = vec![t(None, 1, 2, 0)];
        assert_eq!(resolve(&s, None, true, 1), None);
        // Same depth as the loop: no region is crossed, so the jump is fine.
        assert_eq!(resolve(&s, None, true, 0), Some(1));
    }
}

#[cfg(test)]
mod switch_tests {
    use super::*;

    fn sw(brk: usize) -> LoopTarget {
        LoopTarget {
            kind: TargetKind::Switch,
            label: None,
            brk,
            cont: usize::MAX,
            try_depth: 0,
        }
    }

    fn lp(brk: usize, cont: usize) -> LoopTarget {
        LoopTarget {
            kind: TargetKind::Loop,
            label: None,
            brk,
            cont,
            try_depth: 0,
        }
    }

    /// `while (…) { switch (…) { case 1: continue; } }` — the `continue` belongs
    /// to the WHILE. Taking the innermost target would jump to the switch's exit.
    #[test]
    fn continue_skips_an_enclosing_switch() {
        let s = vec![lp(1, 2), sw(9)];
        assert_eq!(resolve(&s, None, false, 0), Some(2));
        assert_eq!(resolve(&s, None, true, 0), Some(9));
    }

    /// A `continue` with no loop at all (only a switch) is not modelled.
    #[test]
    fn continue_with_only_a_switch_refuses() {
        assert_eq!(resolve(&[sw(9)], None, false, 0), None);
    }
}
