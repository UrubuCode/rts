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
//! # Backend differences
//!
//! Sleep is decided over deterministic dynamic contact islands (`islands`),
//! including slow wake-up and loss of support. GPU/Scene parity for this sleep
//! policy is still pending. A full grid falls back to complete pair traversal;
//! its overflow counter remains visible. Identical fully sleeping worlds reuse
//! an exact input cache and perform no contact traversal.
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
//! A static's centre carries its ROUNDNESS in `w`: 0 is a box, 1 is a sphere of
//! radius `min(half-extent)`. Inverted from a body's shape field on purpose —
//! every writer before the field existed wrote 0 there and meant a box. What
//! bodies and statics are made of rides at the tail of `world`; see `material`.
//!
//! Unchanged on purpose: a program can hand the same three `Float32Array`s to
//! either backend, and a conversion step between them would be one more place for
//! the two to disagree.

pub mod contact;
pub mod grid;

mod adapter;
mod islands;
pub(crate) mod material;
mod step;
pub use adapter::GatherBackend;

use contact::{add, contact_oriented as narrow, dot, length, scale, sub, V3};
use grid::Grid;
use material::{bounce, friction_loss, Body, Materials};
use rayon::prelude::*;

/// The terminal speed that stops a body tunnelling through a floor in one step.
/// The kernel's. Gravity was here too, and is now each body's own (`material`).
const SPEED_CAP: f32 = 48.0;
const SPEED_CAP_SQUARED: f32 = SPEED_CAP * SPEED_CAP;

/// Penetration left uncorrected, so resting contact does not jitter, and the
/// share of the remainder each correction takes.
const SLOP: f32 = 0.04;
const STATIC_RELAXATION: f32 = 0.85;
const PAIR_RELAXATION: f32 = 0.30;
/// How far one sub-step may move a body by position correction alone.
const CORRECTION_CAP: f32 = 0.25;

/// Sleep: every body in an island must stay below this speed for this many
/// steps. A waking member or moving kinematic contact wakes the whole island.
const SLEEP_STEPS: f32 = 10.0;
const SLEEP_SPEED_SQUARED: f32 = 0.2025;

/// Below this height a body has left the world and is parked.
const FLOOR: f32 = -18.0;

/// What a vertical contact takes from horizontal motion per step — on the ground,
/// and from the difference to a supporting body — AT the reference friction.
/// `material::friction_loss` scales both by the pair's own.
const GROUND_FRICTION_LOSS: f32 = 0.08;
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
    /// Contact-block degree for the opt-in Mass Splitting Jacobi path.
    degrees: Vec<f32>,
    islands: islands::Islands,
    sleep_cache: islands::SleepCache,
}

impl Solver {
    /// A solver with no bodies in it yet; the scratch grows on first use.
    pub fn new() -> Self {
        Self {
            grid: Grid::new(),
            broad_phase: BroadPhase::Grid,
            positions: Vec::new(),
            velocities: Vec::new(),
            degrees: Vec::new(),
            islands: islands::Islands::default(),
            sleep_cache: islands::SleepCache::default(),
        }
    }

    /// Runs the steps that follow with the given broad phase. See [`BroadPhase`]
    /// for why the second one exists at all.
    pub fn with_broad_phase(mut self, broad_phase: BroadPhase) -> Self {
        self.broad_phase = broad_phase;
        self
    }

    /// Total bodies dropped due to grid cell capacity overflow in the most recent step.
    pub fn grid_overflows(&self) -> usize {
        self.grid.overflows()
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
        if count == 0
            || world.len() < material::WORLD_HEADER_FLOATS
            || world[material::WORLD_PARAM_LAYOUT_VERSION] != material::PHYSICS_LAYOUT_VERSION
        {
            return;
        }
        let dt = world[material::WORLD_PARAM_DT];
        let statics = (world[material::WORLD_PARAM_NUM_STATICS].max(0.0) as usize).min(
            (world.len().saturating_sub(material::WORLD_HEADER_FLOATS))
                / material::STATIC_RECORD_FLOATS,
        );
        let size = cell_size(world);
        if self.sleep_cache.matches(pos, vel, ext, world) {
            return;
        }

        for _ in 0..substeps {
            self.positions.clear();
            self.positions.extend_from_slice(&pos[..count * 4]);
            self.velocities.clear();
            self.velocities.extend_from_slice(&vel[..count * 4]);
            self.grid.build(&self.positions, count, size);

            let materials = Materials::of(world, count);
            let any_mask = materials.any_mask();
            let mass_splitting = world[material::WORLD_PARAM_MASS_SPLITTING] > 0.5;
            let packed_softness = world[material::WORLD_PARAM_CONTACT_SOFTNESS].max(0.0) as u32;
            let softness_enabled = packed_softness >= material::CONTACT_SOFTNESS_CODE_BASE;
            let mut softness_gamma = 0.0;
            let mut softness_bias_rate = 0.0;
            if softness_enabled {
                let packed = packed_softness - material::CONTACT_SOFTNESS_CODE_BASE;
                let frequency = (packed / 1000) as f32 * 0.01;
                let damping_ratio = (packed % 1000) as f32 * 0.01;
                let omega = 6.28318530718 * frequency;
                let stiffness = omega * omega;
                let damping = 2.0 * damping_ratio * omega;
                let denominator = damping + dt * stiffness;
                if denominator > 0.000001 {
                    softness_gamma = 1.0 / (dt * denominator);
                    softness_bias_rate = stiffness / denominator;
                }
            }
            self.degrees.clear();
            if mass_splitting {
                self.degrees.resize(count, 1.0);
                for body in 0..count {
                    if self.velocities[body * 4 + 3] <= 0.5 || ext[body * 4 + 3] <= 0.0 {
                        continue;
                    }
                    let p = triple(&self.positions, body);
                    let h = triple(ext, body);
                    let my_shape = self.velocities[body * 4 + 3];
                    let (my_layer, my_mask) = materials.layer_mask(body);
                    let mut degree = 0usize;
                    visit_near(&self.grid, self.broad_phase, count, size, p, |other| {
                        if other == body
                            || self.velocities[other * 4 + 3] <= 0.5
                            || ext[other * 4 + 3] <= 0.0
                        {
                            return;
                        }
                        let (other_layer, other_mask) = materials.layer_mask(other);
                        if any_mask
                            && ((my_mask & other_layer) == 0 || (other_mask & my_layer) == 0)
                        {
                            return;
                        }
                        if narrow(
                            p,
                            h,
                            my_shape,
                            materials.body_orientation(body),
                            triple(&self.positions, other),
                            triple(ext, other),
                            self.velocities[other * 4 + 3],
                            materials.body_orientation(other),
                        )
                        .is_some()
                        {
                            degree += 1;
                        }
                    });
                    self.degrees[body] = degree.max(1) as f32;
                }
            }

            let scene = Scene {
                positions: &self.positions,
                velocities: &self.velocities,
                extents: ext,
                world,
                materials,
                degrees: &self.degrees,
                mass_splitting,
                softness_enabled,
                softness_gamma,
                softness_bias_rate,
                grid: &self.grid,
                broad_phase: self.broad_phase,
                count,
                statics,
                size,
                dt,
            };
            self.islands.prepare(&scene);
            // O filtro de máscara é escolhido aqui, uma vez por sub-passo, e não
            // testado por candidato: cada instância de `solve` já nasce com ou
            // sem ele.
            let bodies = pos[..count * 4]
                .par_chunks_mut(4)
                .zip(vel[..count * 4].par_chunks_mut(4))
                .enumerate();
            match any_mask {
                true => bodies.for_each(|(body, (out_pos, out_vel))| {
                    scene.solve::<true>(body, self.islands.counters[body], out_pos, out_vel)
                }),
                false => bodies.for_each(|(body, (out_pos, out_vel))| {
                    scene.solve::<false>(body, self.islands.counters[body], out_pos, out_vel)
                }),
            }
            self.islands.finish(pos, vel);
            if self
                .sleep_cache
                .capture_if_asleep(pos, vel, ext, world, materials, count)
            {
                break;
            }
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
    let size = match world[material::WORLD_PARAM_CELL_SIZE].is_finite() {
        true => world[material::WORLD_PARAM_CELL_SIZE].max(0.001),
        false => 1.0,
    };
    size + 2.0 * islands::CONTACT_MARGIN
}

/// Overflow must not disconnect an island or silently discard a solver pair.
/// Keep its counter visible, but visit all bodies for correctness on that step.
fn visit_near(
    grid: &Grid,
    broad_phase: BroadPhase,
    count: usize,
    size: f32,
    p: V3,
    visit: impl FnMut(usize),
) {
    if broad_phase == BroadPhase::Everything || grid.overflows() != 0 {
        (0..count).for_each(visit);
    } else {
        grid.near(p, size, visit);
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
    materials: Materials<'a>,
    degrees: &'a [f32],
    mass_splitting: bool,
    softness_enabled: bool,
    softness_gamma: f32,
    softness_bias_rate: f32,
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
        visit_near(self.grid, self.broad_phase, self.count, self.size, p, visit);
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
    /// `FILTRO` é o `any_mask` escolhido uma vez por sub-passo.
    fn solve<const FILTRO: bool>(
        &self,
        body: usize,
        mut sleep: f32,
        out_pos: &mut [f32],
        out_vel: &mut [f32],
    ) {
        let mut p = self.position(body);
        let mut v = self.velocity(body);
        let shape = self.shape(body);
        let h = self.extent(body);
        let inverse_mass = self.extents[body * 4 + 3];
        let mine = self.materials.body(body);

        if mine.body_type == material::BODY_STATIC {
            // Static: does not move at all, zero velocity
            write(out_pos, p, SLEEP_STEPS);
            write(out_vel, [0.0; 3], shape);
            return;
        }

        if mine.body_type == material::BODY_KINEMATIC {
            // Kinematic: moves purely by its velocity, ignores gravity, drag, floor, statics and impulses
            p = add(p, scale(v, self.dt));
            write(out_pos, p, 0.0);
            write(out_vel, v, shape);
            return;
        }

        if sleep >= SLEEP_STEPS {
            // The island decision already includes every touching dynamic and
            // moving anchor. Sleeping bodies perform no solver work.
            write(out_pos, p, sleep);
            write(out_vel, v, shape);
            return;
        }

        v[1] -= mine.gravity * self.dt;
        let speed = dot(v, v);
        if speed > SPEED_CAP_SQUARED {
            v = scale(v, SPEED_CAP / speed.sqrt());
        }
        v = scale(v, mine.drag_factor(self.dt));
        p = add(p, scale(v, self.dt));

        // Whether anything touched this body this step, which is what decides
        // if it may fall asleep. See the counter below.
        let mut supported = mine.rest_on_floor(&mut p[1], &mut v[1]);
        supported |= self.against_statics::<FILTRO>(
            &mut p,
            &mut v,
            h,
            shape,
            self.materials.body_orientation(body),
            inverse_mass,
            mine,
        );

        let before = p;
        supported |= self.against_bodies::<FILTRO>(
            body,
            &mut p,
            &mut v,
            &mut sleep,
            h,
            shape,
            inverse_mass,
            mine,
        );
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

        // SLEEP NEEDS SUPPORT, not just a low speed.
        //
        // Speed alone puts a body to sleep IN MID-AIR: at the top of a bounce it
        // is slow for as many steps as the arc is shallow, and ten of them is a
        // hop of a few centimetres. It then hangs there, because a sleeping body
        // is only woken by a fast neighbour and empty space is not one.
        //
        // This was unreachable while restitution was a constant zero — nothing
        // bounced, so a body was only ever slow on the ground. The material
        // region made it reachable and a dropped box with bounce 0.5 hung at
        // y = 1.135 for as long as anyone watched. Touching something is the
        // condition that was always meant: a body at rest is resting ON
        // something.
        sleep = match (supported || mine.gravity.abs() <= 0.000001 || p[1] <= FLOOR)
            && dot(v, v) < SLEEP_SPEED_SQUARED
        {
            true => sleep + 1.0,
            false => 0.0,
        };

        write(out_pos, p, sleep);
        write(out_vel, v, shape);
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
