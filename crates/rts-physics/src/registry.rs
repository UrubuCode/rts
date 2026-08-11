//! Which backends this build has, and how one is chosen.
//!
//! # Why a build-time list and not a runtime plug-in
//!
//! A backend is chosen by name from a list fixed when the binary was compiled.
//! The alternative — loading one at run time — was rejected for the reason the
//! whole crate is shaped around: a backend runs on `rayon` workers over the
//! program's own buffers, and a dynamically loaded one would be doing that with
//! code the build never saw. The failure mode is a corrupted scene, and the
//! benefit is a flexibility nobody asked for.
//!
//! So each backend is a Cargo feature. Which are present is a **build fact**,
//! which is the same thing `rts-core`'s rule 1 says decides where anything
//! lives.
//!
//! # An absent backend fails BY NAME, and never falls back
//!
//! This is the rule that matters most here, and it is the workspace's, not this
//! module's: *a surface that cannot do what its name means does not ship.*
//!
//! Asking for `rapier` in a build without it answers a refusal that names it and
//! lists what is present. It does **not** quietly run the gather solver instead,
//! and the reason is not tidiness: the two are in different parity groups, so
//! the substitution would produce a program that runs, produces plausible
//! numbers, and lands somewhere else than the same program on the machine next
//! to it. An absent name fails at the call; a hollow one fails in production.
//!
//! The one place a fallback is correct is when the caller asks for *any* backend
//! — [`Selection::Any`] — because there the caller has said the choice is not
//! theirs.

use crate::backend::{Backend, Needs, ParityGroup};

/// How a caller picks.
pub enum Selection<'a> {
    /// This backend or a refusal. What a program that depends on a specific
    /// solver's behaviour asks for.
    Named(&'a str),
    /// Whichever present backend satisfies `Needs`, preferring the earliest in
    /// [`all`]. What a program that just wants physics asks for.
    Any,
}

/// Why a selection produced nothing, in a form a caller can act on.
///
/// Two variants and not one because they send someone to different places: a
/// missing name is a build problem, and an unmet need is a design problem.
#[derive(Debug)]
pub enum SelectionError {
    /// No backend by that name in this build.
    Unknown {
        /// What was asked for, in a form the caller can print.
        asked: &'static str,
    },
    /// The backend exists and cannot do what the scene needs.
    Lacking {
        /// The backend that was found.
        backend: &'static str,
        /// The first capability it lacks.
        needs: &'static str,
    },
    /// `Any` was asked and nothing present satisfies the needs.
    NoneCapable {
        /// The first capability nothing present satisfies.
        needs: &'static str,
    },
}

/// Every backend compiled into this build, in preference order.
///
/// The gather solver is first because it is the one with a parity guarantee and
/// the one every target has. A third-party engine is never preferred by default:
/// choosing it silently would change where a scene lands, and that is a decision
/// for the program, not for the order of a list.
pub fn all() -> Vec<Box<dyn Backend>> {
    let mut backends: Vec<Box<dyn Backend>> = Vec::new();
    backends.push(Box::new(crate::solver::GatherBackend::new()));

    // Each of these is a Cargo feature, absent by default. The `cfg` and nothing
    // else is what makes "which backends exist" a build fact.
    //
    // They are listed here, commented, rather than left to be discovered:
    // someone adding Rapier should find the line that says where it goes and
    // what it must answer, not infer the shape from the one implementation that
    // happens to exist.
    //
    //   #[cfg(feature = "rapier")]
    //   backends.push(Box::new(crate::rapier::RapierBackend::new()));
    //     — ParityGroup::Independent. Hull-against-hull, joints, CCD. Pure Rust,
    //       runs on wasm, uses the same `rayon` this crate already does, which
    //       is why it is the first third-party engine worth the work.
    //
    //   #[cfg(feature = "physx")]
    //   backends.push(Box::new(crate::physx::PhysXBackend::new()));
    //     — ParityGroup::Independent. Articulations and vehicles. Needs a C++
    //       toolchain, does NOT build for wasm, and its GPU path needs CUDA and
    //       therefore an NVIDIA card — so it can never be a default or a
    //       fallback, only a choice a program makes.

    backends
}

/// Pick a backend, or say precisely why not.
pub fn select(
    backends: &[Box<dyn Backend>],
    selection: &Selection<'_>,
    needs: &Needs,
) -> Result<usize, SelectionError> {
    match selection {
        Selection::Named(asked) => {
            let found = backends.iter().position(|b| b.name() == *asked);
            match found {
                None => Err(SelectionError::Unknown {
                    // The asked-for name is not `'static` and the error is, so
                    // what is reported is that it was unknown rather than which
                    // string it was. The caller has the string; what it does not
                    // have is the list, and `present` answers that.
                    asked: "the requested backend is not in this build",
                }),
                Some(i) if !backends[i].supports(needs) => Err(SelectionError::Lacking {
                    backend: backends[i].name(),
                    needs: describe(needs),
                }),
                Some(i) => Ok(i),
            }
        }
        Selection::Any => backends
            .iter()
            .position(|b| b.supports(needs))
            .ok_or(SelectionError::NoneCapable { needs: describe(needs) }),
    }
}

/// The names present in this build, for a diagnostic that tells a caller what it
/// COULD have asked for. A refusal that lists no alternatives makes someone go
/// read the build file.
pub fn present(backends: &[Box<dyn Backend>]) -> Vec<&'static str> {
    backends.iter().map(|b| b.name()).collect()
}

/// Which backends may be compared against each other by final position.
///
/// A test that compares two backends must ask this first. Comparing across
/// groups measures the difference between two solvers, which is not a defect and
/// not a number worth having — see README rule 10.
pub fn comparable(backends: &[Box<dyn Backend>], group: ParityGroup) -> Vec<&'static str> {
    backends
        .iter()
        .filter(|b| b.parity_group() == group)
        .map(|b| b.name())
        .collect()
}

/// The first unmet need, as a phrase for an error message.
///
/// One need and not all of them because a caller fixes them one at a time, and a
/// message listing four things is one nobody reads to the end.
fn describe(needs: &Needs) -> &'static str {
    if needs.hull_against_hull {
        return "hull-against-hull contact";
    }
    if needs.continuous {
        return "continuous collision";
    }
    if needs.angular {
        return "angular velocity and torque";
    }
    if needs.joints {
        return "joints";
    }
    "nothing in particular"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_build_has_the_gather_solver_and_it_is_first() {
        // First on purpose: it is the one with a parity guarantee and the one
        // every target has. A third-party engine becoming the default by being
        // earlier in a list would change where scenes land without a decision.
        let backends = all();
        assert_eq!(backends[0].name(), "gather");
        assert_eq!(backends[0].parity_group(), ParityGroup::Gather);
    }

    #[test]
    fn asking_for_a_backend_this_build_lacks_is_refused_and_not_substituted() {
        // The failure this pins is the expensive one: a silent substitution puts
        // a program in a different parity group than it asked for, so it runs,
        // produces plausible numbers, and lands somewhere else than the same
        // program on another machine.
        let backends = all();
        let r = select(&backends, &Selection::Named("rapier"), &Needs::default());
        assert!(matches!(r, Err(SelectionError::Unknown { .. })));
    }

    #[test]
    fn a_need_the_gather_solver_lacks_is_refused_by_name_rather_than_approximated() {
        // Hull-against-hull is the live case: the gather solver degrades it to a
        // sphere, which `docs/colisores.md` measured as the only affordable
        // choice — and which a caller that needs the real thing must be told
        // about rather than left to discover.
        let backends = all();
        let needs = Needs { hull_against_hull: true, ..Needs::default() };
        match select(&backends, &Selection::Named("gather"), &needs) {
            Err(SelectionError::Lacking { backend, needs }) => {
                assert_eq!(backend, "gather");
                assert_eq!(needs, "hull-against-hull contact");
            }
            other => panic!("expected a refusal naming the gap, got {other:?}"),
        }
    }

    #[test]
    fn any_falls_back_only_when_the_caller_said_the_choice_was_not_theirs() {
        // `Any` is the ONE place a substitution is correct, because the caller
        // has said so. With a need nothing satisfies, it still refuses.
        let backends = all();
        assert!(select(&backends, &Selection::Any, &Needs::default()).is_ok());
        let needs = Needs { joints: true, ..Needs::default() };
        assert!(matches!(
            select(&backends, &Selection::Any, &needs),
            Err(SelectionError::NoneCapable { .. })
        ));
    }
}
