//! The gather/Jacobi rigid-body step, over plain slices of `f32`.
//!
//! # Reuse-check
//!
//! Nothing in this workspace answers this. The nearest things are
//! `rts_cranelift`'s use of `rayon` — which parallelises compilation, not a
//! simulation — and `rts-ui`'s `scene`, which uploads geometry to a GPU and
//! computes nothing. So this is written, and it is written as a **port**: the
//! formulation is `engine/rigid/gpurigid.ts`'s WGSL kernel in the `rts-game`
//! project, which is the validated design, and the parity between the two
//! backends is the reason not to invent a second one.
//!
//! # Why gather/Jacobi rather than a sequential solver with locks
//!
//! Every body reads its neighbours and writes ONLY ITSELF. That is the whole
//! design and it buys three things at once: no write conflict, so it parallelises
//! with no partitioning into islands and no round-robin; the same arithmetic the
//! GPU backend performs, so the measured parity between them keeps meaning
//! something; and no atomics, which is what the GPU side measured as the
//! difference between a fast pass and a pessimised one.
//!
//! Jacobi needs a smaller relaxation than a sequential solver, and 0.30 per pair
//! plus a per-step displacement ceiling are the kernel's answer to a body buried
//! under dozens of neighbours summing corrections in one step. Those constants
//! are ported, not chosen here.
//!
//! # The one place this deliberately differs from the kernel, and it is a choice
//!
//! **Neighbours are read from a snapshot taken at the top of the sub-step.**
//!
//! The WGSL does not do that: a GPU thread reads `pos[j]` while another thread
//! writes it, so the kernel's neighbour state is whichever of the two got there
//! first — a benign race by intent, but a race. Reproducing it in Rust would be
//! unsound (`&` and `&mut` to the same element), so the choice was between an
//! `UnsafeCell` that reproduces the nondeterminism and a snapshot that removes
//! it. The snapshot is taken: it makes this backend deterministic and run to run
//! reproducible, and it costs one copy of `pos`+`vel` per sub-step, which is
//! 64 KB at 2000 bodies.
//!
//! What it means for parity is stated rather than hidden: this is TRUE Jacobi and
//! the kernel is Jacobi-with-races, so at high contact density the two can drift
//! apart over many steps even though every rule below is identical. That is a
//! trade for the owner of the backend contract to accept or refuse, not one this
//! module decides quietly.
//!
//! # The buffer layout is the kernel's, unchanged
//!
//! | buffer | x, y, z | w |
//! |---|---|---|
//! | `pos` | centre | sleep counter (>= 10 is asleep) |
//! | `vel` | velocity | shape: 0 sphere, 1 box |
//! | `ext` | half-extent | inverse mass (0 is immovable) |
//! | `world` | `[0]` = dt, static count, cell size; then static (centre, half-extent) pairs |
//!
//! Unchanged on purpose: a program can hand the same three `Float32Array`s to
//! either backend, and a conversion step between them would be one more place for
//! the two to disagree.

pub mod contact;
pub mod grid;

mod adapter;
pub use adapter::GatherBackend;

use contact::{V3, add, contact as narrow, dot, length, scale, sub};
use grid::Grid;
use rayon::prelude::*;

/// Gravity, and the terminal speed that stops a body tunnelling through a floor
/// in one step. Both the kernel's.
const GRAVITY: f32 = 9.8;
const SPEED_CAP: f32 = 48.0;
const SPEED_CAP_SQUARED: f32 = SPEED_CAP * SPEED_CAP;

/// Penetration left uncorrected, so resting contact does not jitter, and the
/// share of the remainder each correction takes.
const SLOP: f32 = 0.04;
const STATIC_RELAXATION: f32 = 0.85;
const PAIR_RELAXATION: f32 = 0.30;
/// How far one sub-step may move a body by position correction alone.
const CORRECTION_CAP: f32 = 0.25;

/// Sleep: this many steps below this speed, and a neighbour above this speed
/// wakes it. Squared, because the comparison is against `dot(v, v)`.
const SLEEP_STEPS: f32 = 10.0;
const SLEEP_SPEED_SQUARED: f32 = 0.2025;
const WAKE_SPEED_SQUARED: f32 = 0.64;

/// Below this height a body has left the world and is parked.
const FLOOR: f32 = -18.0;

/// Ground friction on a vertical contact, and how much of a supporting body's
/// horizontal motion a stacked one takes up.
const GROUND_FRICTION: f32 = 0.92;
const STACK_FRICTION: f32 = 0.10;

/// How steep a normal must be to count as vertical — the branch that separates
/// the stacking rules from an ordinary impulse.
const VERTICAL: f32 = 0.5;

/// How a body finds the neighbours it might touch.
///
/// Two of these exist for ONE reason and it is not a tuning knob: a measurement.
/// The question "how much of this solver's speed is the algorithm and how much is
/// the language and the threads" has exactly one honest answer, which is to run
/// the same solver both ways. [`BroadPhase::Everything`] is what this was before
/// there was a grid, and it is the arm of that comparison —
/// `examples/onde_custa.rs` is its only caller.
///
/// [`BroadPhase::Grid`] is what a program gets: both the GPU kernel this ports
/// and the TypeScript solver it is compared against have a spatial hash, so
/// shipping without one would make every number a measurement of the missing
/// grid rather than of anything else.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BroadPhase {
    /// The spatial hash. What every caller but the bench wants.
    Grid,
    /// Every body considers every other. O(n²), and the reference arm.
    Everything,
}

/// Scratch that outlives a step, so a sub-step allocates nothing.
pub struct Solver {
    grid: Grid,
    broad_phase: BroadPhase,
    /// The neighbour state every body reads. See the module doc.
    positions: Vec<f32>,
    velocities: Vec<f32>,
}

impl Solver {
    /// A solver with no bodies in it yet; the scratch grows on first use.
    pub fn new() -> Self {
        Self {
            grid: Grid::new(),
            broad_phase: BroadPhase::Grid,
            positions: Vec::new(),
            velocities: Vec::new(),
        }
    }

    /// Runs the steps that follow with the given broad phase. See [`BroadPhase`]
    /// for why the second one exists at all.
    pub fn with_broad_phase(mut self, broad_phase: BroadPhase) -> Self {
        self.broad_phase = broad_phase;
        self
    }

    /// Advances `substeps` fixed steps in place.
    ///
    /// The three body buffers are four floats per body and must agree about how
    /// many there are; the shortest decides, because a caller that resized one
    /// and not the others would otherwise read another body's extent as its own.
    pub fn step(
        &mut self,
        pos: &mut [f32],
        vel: &mut [f32],
        ext: &[f32],
        world: &[f32],
        substeps: usize,
    ) {
        let count = pos.len().min(vel.len()).min(ext.len()) / 4;
        if count == 0 || world.len() < 4 {
            return;
        }
        let dt = world[0];
        let statics = (world[1].max(0.0) as usize).min((world.len().saturating_sub(4)) / 8);
        let size = cell_size(world);

        for _ in 0..substeps {
            self.positions.clear();
            self.positions.extend_from_slice(&pos[..count * 4]);
            self.velocities.clear();
            self.velocities.extend_from_slice(&vel[..count * 4]);
            self.grid.build(&self.positions, count, size);

            let scene = Scene {
                positions: &self.positions,
                velocities: &self.velocities,
                extents: ext,
                world,
                grid: &self.grid,
                broad_phase: self.broad_phase,
                count,
                statics,
                size,
                dt,
            };
            pos[..count * 4]
                .par_chunks_mut(4)
                .zip(vel[..count * 4].par_chunks_mut(4))
                .enumerate()
                .for_each(|(body, (out_pos, out_vel))| scene.solve(body, out_pos, out_vel));
        }
    }
}

/// The cell size the broad phase uses.
///
/// It comes from the caller in `world[2]` for one reason: two bodies touch only
/// when their centres are closer than `hi + hj` on every axis, so a cell at least
/// `2 × largest half-extent` wide makes the 27-cell scan EXACT rather than an
/// approximation. Smaller loses contacts and changes the physics; larger only
/// wastes candidates. The caller knows the largest extent without scanning
/// anything, which is why it is passed rather than derived here.
fn cell_size(world: &[f32]) -> f32 {
    match world[2].is_finite() {
        true => world[2].max(0.001),
        false => 1.0,
    }
}

/// Everything one body reads while solving itself. Shared across threads by
/// reference, and every field of it is immutable for the length of a sub-step —
/// which is what makes the parallelism free of any synchronisation at all.
struct Scene<'a> {
    positions: &'a [f32],
    velocities: &'a [f32],
    extents: &'a [f32],
    world: &'a [f32],
    grid: &'a Grid,
    broad_phase: BroadPhase,
    count: usize,
    statics: usize,
    size: f32,
    dt: f32,
}

impl Scene<'_> {
    /// Every body that might touch the one at `p`.
    ///
    /// Written once because both places that need it — waking a sleeping body and
    /// solving its pairs — must agree about what "near" means. The kernel this
    /// ports has these as two copies of one triple loop, which is two places for
    /// a broad-phase change to be made in only one of.
    #[inline]
    fn near(&self, p: V3, visit: impl FnMut(usize)) {
        match self.broad_phase {
            BroadPhase::Grid => self.grid.near(p, self.size, visit),
            BroadPhase::Everything => (0..self.count).for_each(visit),
        }
    }
    #[inline]
    fn position(&self, body: usize) -> V3 {
        triple(self.positions, body)
    }

    #[inline]
    fn velocity(&self, body: usize) -> V3 {
        triple(self.velocities, body)
    }

    #[inline]
    fn extent(&self, body: usize) -> V3 {
        triple(self.extents, body)
    }

    #[inline]
    fn shape(&self, body: usize) -> f32 {
        self.velocities[body * 4 + 3]
    }

    /// One body's whole sub-step: wake or skip, integrate, statics, pairs,
    /// clamp, park, sleep. Writes only its own four-float slots.
    fn solve(&self, body: usize, out_pos: &mut [f32], out_vel: &mut [f32]) {
        let mut p = self.position(body);
        let mut v = self.velocity(body);
        let mut sleep = self.positions[body * 4 + 3];
        let shape = self.shape(body);
        let h = self.extent(body);
        let inverse_mass = self.extents[body * 4 + 3];

        if sleep >= SLEEP_STEPS {
            if !self.disturbed(body, p, h, shape) {
                // Unchanged, and written out rather than left alone: the caller's
                // buffer is the destination, and the snapshot it was read from is
                // a different allocation.
                write(out_pos, p, sleep);
                write(out_vel, v, shape);
                return;
            }
            sleep = 0.0;
        }

        v[1] -= GRAVITY * self.dt;
        let speed = dot(v, v);
        if speed > SPEED_CAP_SQUARED {
            v = scale(v, SPEED_CAP / speed.sqrt());
        }
        p = add(p, scale(v, self.dt));

        self.against_statics(&mut p, &mut v, h, shape);

        let before = p;
        self.against_bodies(body, &mut p, &mut v, &mut sleep, h, shape, inverse_mass);
        // The per-step ceiling on positional correction, which is what keeps a
        // deep pile from exploding: a buried body sums the corrections of dozens
        // of neighbours in one Jacobi pass.
        let correction = sub(p, before);
        let moved = length(correction);
        if moved > CORRECTION_CAP {
            p = add(before, scale(correction, CORRECTION_CAP / moved));
        }

        if p[1] < FLOOR {
            p[1] = FLOOR;
            v = [0.0; 3];
        }

        // NaN quarantine. A contaminated body is parked rather than left in
        // place, because NaN never sleeps — every comparison against it is false
        // — and the next gather would spread it to every neighbour that touches
        // it.
        if p.iter().chain(v.iter()).any(|x| x.is_nan()) {
            p = [0.0, FLOOR, 0.0];
            v = [0.0; 3];
            sleep = SLEEP_STEPS;
        }

        sleep = match dot(v, v) < SLEEP_SPEED_SQUARED {
            true => sleep + 1.0,
            false => 0.0,
        };

        write(out_pos, p, sleep);
        write(out_vel, v, shape);
    }

    /// Whether a sleeping body has a FAST neighbour touching it.
    ///
    /// Through the grid, like everything else. Scanning every body instead would
    /// make a scene at rest — the case sleeping exists to make cheap — the one
    /// that pays a full n² every step.
    fn disturbed(&self, body: usize, p: V3, h: V3, shape: f32) -> bool {
        let mut woken = false;
        self.near(p, |other| {
            if woken || other == body {
                return;
            }
            let vj = self.velocity(other);
            if dot(vj, vj) <= WAKE_SPEED_SQUARED {
                return;
            }
            let hit = narrow(
                p,
                h,
                shape,
                self.position(other),
                self.extent(other),
                self.shape(other),
            );
            woken = hit.is_some();
        });
        woken
    }

    /// The immovable geometry of the scene, which does not give: the whole
    /// correction is the moving body's, and only the velocity component entering
    /// the static is removed.
    fn against_statics(&self, p: &mut V3, v: &mut V3, h: V3, shape: f32) {
        for k in 0..self.statics {
            let centre = triple(self.world, 1 + k * 2);
            let half = triple(self.world, 2 + k * 2);
            // A static is always a box: nothing writes a spherical one. A
            // dynamic sphere on a floor therefore takes the sphere-box case,
            // which is the most visible contact there is.
            let Some((normal, depth)) = narrow(*p, h, shape, centre, half, 1.0) else {
                continue;
            };
            *p = add(*p, scale(normal, (depth - SLOP).max(0.0) * STATIC_RELAXATION));
            let approach = dot(*v, normal);
            if approach < 0.0 {
                *v = sub(*v, scale(normal, approach));
            }
            if normal[1] > VERTICAL {
                v[0] *= GROUND_FRICTION;
                v[2] *= GROUND_FRICTION;
            }
        }
    }

    /// The dynamic pairs: this body applies only its own half of each.
    ///
    /// The cell is recomputed from the position AFTER integration and statics,
    /// not from the one the grid was built with — a pair the step itself created
    /// would otherwise be missed until the next one.
    fn against_bodies(
        &self,
        body: usize,
        p: &mut V3,
        v: &mut V3,
        sleep: &mut f32,
        h: V3,
        shape: f32,
        inverse_mass: f32,
    ) {
        let mut position = *p;
        let mut velocity = *v;
        let mut counter = *sleep;
        self.near(*p, |other| {
            if other == body {
                return;
            }
            let Some((normal, depth)) = narrow(
                position,
                h,
                shape,
                self.position(other),
                self.extent(other),
                self.shape(other),
            ) else {
                return;
            };
            let other_inverse_mass = self.extents[other * 4 + 3];
            let share = inverse_mass / (inverse_mass + other_inverse_mass).max(0.0001);
            let theirs = self.velocity(other);
            // Relative velocity along the normal. Negative is approaching.
            let approach = dot(sub(velocity, theirs), normal);

            if normal[1] > VERTICAL || normal[1] < -VERTICAL {
                if approach < -1.0 {
                    // A real impact: a normal impulse with no restitution —
                    // stone does not bounce.
                    velocity = sub(velocity, scale(normal, approach * share));
                    counter = 0.0;
                } else if approach < 0.5 && normal[1] > VERTICAL {
                    // SUPPORT INHERITANCE: resting on something and descending
                    // slowly, so the support's vertical velocity is taken rather
                    // than an impulse applied — an impulse here is the limit
                    // cycle that makes a column vibrate forever.
                    velocity[1] = theirs[1];
                    velocity[0] += (theirs[0] - velocity[0]) * STACK_FRICTION;
                    velocity[2] += (theirs[2] - velocity[2]) * STACK_FRICTION;
                }
            } else if approach < 0.0 {
                velocity = sub(velocity, scale(normal, approach * share));
                if approach < -1.0 {
                    counter = 0.0;
                }
            }
            position = add(
                position,
                scale(normal, (depth - SLOP).max(0.0) * PAIR_RELAXATION * share),
            );
        });
        *p = position;
        *v = velocity;
        *sleep = counter;
    }
}

/// The xyz of a four-float record.
#[inline]
fn triple(buffer: &[f32], record: usize) -> V3 {
    [
        buffer[record * 4],
        buffer[record * 4 + 1],
        buffer[record * 4 + 2],
    ]
}

/// One four-float record: a vector and the field beside it.
#[inline]
fn write(out: &mut [f32], value: V3, fourth: f32) {
    out[0] = value[0];
    out[1] = value[1];
    out[2] = value[2];
    out[3] = fourth;
}

#[cfg(test)]
mod tests;
