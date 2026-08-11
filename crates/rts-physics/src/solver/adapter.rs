//! The gather solver, as a [`Backend`].
//!
//! # Why an adapter and not the trait on `Solver`
//!
//! `Solver` holds scratch and takes `&mut self`; a `Backend` is shared across
//! `rayon` workers and takes `&self`. Implementing the trait on `Solver`
//! directly would have forced either a lock around the scratch — on the hottest
//! path in the crate — or scratch allocated per step, which is what the struct
//! exists to avoid.
//!
//! So the adapter owns the `Solver` behind a `RefCell` and is `!Sync` by
//! construction… except a `Backend` must be `Sync`. The resolution is a
//! thread-local solver: each thread that steps gets its own scratch, which is
//! correct because scratch holds no body state between calls (README rule 4)
//! and is overwritten at the top of every sub-step. Two threads stepping two
//! scenes at once is then simply two solvers, and neither has to know.
//!
//! # What this adapter is NOT allowed to do
//!
//! Change an answer. Everything here is refusal, capability and plumbing; the
//! numbers come from `super::Solver` unchanged, because this crate is a port of
//! `gpurigid.ts` and a rule improved on this side is a parity failure on that
//! one (README rule 1).

use std::cell::RefCell;

use crate::backend::{Backend, Needs, ParityGroup, Scene, StepOutcome};
use crate::shape::ShapeKind;

thread_local! {
    /// One solver's scratch per thread. See the module doc for why this is not a
    /// field: the trait is `&self` and the scratch is `&mut`.
    static SCRATCH: RefCell<super::Solver> = RefCell::new(super::Solver::new());
}

/// The gather/Jacobi solver this crate is built around.
pub struct GatherBackend;

impl GatherBackend {
    /// The backend. Stateless: the scratch is thread-local, for the reason in
    /// the module doc.
    pub fn new() -> Self {
        Self
    }
}

impl Default for GatherBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for GatherBackend {
    fn name(&self) -> &'static str {
        "gather"
    }

    fn parity_group(&self) -> ParityGroup {
        // The WGSL kernel in `gpurigid.ts` is the other member, and
        // `claude-test-paridade-formas.ts` measures the two at 0.000000 over 13
        // supported contacts. That number is what this variant asserts.
        ParityGroup::Gather
    }

    fn supports(&self, needs: &Needs) -> bool {
        // Every one of these is `false` for a measured or stated reason, and
        //answering `true` to any of them would be the silent approximation
        // README rule 9 exists to forbid.
        //
        //   hull_against_hull  `docs/colisores.md` §3: 2.2 billion dot products
        //                      a frame at 2000 bodies. The solver degrades the
        //                      pair to a sphere, deliberately.
        //   continuous         there is no swept test; a fast body tunnels, and
        //                      the speed clamp is a net rather than a fix.
        //   angular            no torque, no angular velocity, anywhere.
        //   joints             none, and no articulation to hang one from.
        !needs.hull_against_hull && !needs.continuous && !needs.angular && !needs.joints
    }

    fn supports_shape(&self, kind: ShapeKind) -> bool {
        match kind {
            ShapeKind::Sphere | ShapeKind::Box => true,
            // A hull is READ — a body may carry one and it collides as the
            // sphere that fits inside it — but this answers `false` because the
            // question is whether the shape is understood, and answering `true`
            // would let a caller conclude the geometry is respected.
            ShapeKind::Hull(_) => false,
            ShapeKind::Capsule => false,
        }
    }

    fn step(&self, scene: &mut Scene<'_>, needs: &Needs) -> StepOutcome {
        if !self.supports(needs) {
            return StepOutcome::Unsupported {
                needs: "the gather solver has no hull-against-hull, continuous \
                        collision, angular velocity or joints",
            };
        }
        let count = scene.body_count();
        if count == 0 {
            return StepOutcome::Refused { why: "no bodies" };
        }
        if scene.world.len() < 4 {
            return StepOutcome::Refused { why: "world buffer shorter than its header" };
        }
        // `world[3]` is the sub-step count and the layout's own unused slot. A
        // zero here is a caller asking for nothing, which is not an error: it is
        // a frame the program chose not to advance, and answering `Advanced { 0 }`
        // says exactly that.
        let substeps = scene.world[3].max(0.0) as usize;

        SCRATCH.with(|cell| {
            let mut solver = cell.borrow_mut();
            solver.step(scene.pos, scene.vel, scene.ext, scene.world, substeps);
        });

        StepOutcome::Advanced { sub_steps: substeps as u32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_of(bodies: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
        let pos = vec![0.0; bodies * 4];
        let vel = vec![0.0; bodies * 4];
        let ext = vec![0.5; bodies * 4];
        // dt, statics, cell size, sub-steps
        let world = vec![1.0 / 60.0, 0.0, 1.0, 1.0];
        (pos, vel, ext, world)
    }

    #[test]
    fn a_scene_needing_hulls_is_refused_and_no_buffer_is_touched() {
        // The pairing of the two assertions is the point: a refusal that had
        // already half-simulated would leave a scene that runs and is wrong,
        // which README rule 5 says is worse than a call that plainly did not
        // happen.
        let (mut pos, mut vel, mut ext, mut world) = scene_of(2);
        pos[1] = 5.0;
        let before = pos.clone();
        let mut scene = Scene { pos: &mut pos, vel: &mut vel, ext: &mut ext, world: &mut world };
        let needs = Needs { hull_against_hull: true, ..Needs::default() };
        let outcome = GatherBackend::new().step(&mut scene, &needs);
        assert!(matches!(outcome, StepOutcome::Unsupported { .. }));
        assert_eq!(pos, before, "a refusal must not have advanced anything");
    }

    #[test]
    fn a_supported_scene_advances_and_reports_how_many_sub_steps_ran() {
        // Reported rather than assumed, because a caller comparing two backends
        // by final position has to know they ran the same number of steps before
        // concluding they disagree — which is exactly the defect that made a GPU
        // bench publish ms/frame for a backend advancing in 25 frames of 60.
        let (mut pos, mut vel, mut ext, mut world) = scene_of(2);
        let mut scene = Scene { pos: &mut pos, vel: &mut vel, ext: &mut ext, world: &mut world };
        let outcome = GatherBackend::new().step(&mut scene, &Needs::default());
        match outcome {
            StepOutcome::Advanced { sub_steps } => assert_eq!(sub_steps, 1),
            other => panic!("expected the step to run, got {other:?}"),
        }
    }

    #[test]
    fn a_hull_is_not_claimed_as_understood_even_though_a_body_may_carry_one() {
        // The distinction this pins: the solver READS a hull code and collides
        // the body as the sphere inside it. Answering `true` here would let a
        // caller conclude the geometry is respected, and it is not.
        let b = GatherBackend::new();
        assert!(b.supports_shape(ShapeKind::Sphere));
        assert!(b.supports_shape(ShapeKind::Box));
        assert!(!b.supports_shape(ShapeKind::Hull(1)));
    }
}
