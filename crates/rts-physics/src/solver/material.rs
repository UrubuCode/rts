//! What a body and a static are MADE of: gravity, restitution, drag, friction,
//! and the implicit floor — per record, read from the tail of `world`.
//!
//! # Reuse-check
//!
//! Nothing in this workspace answers this; the only material anywhere is the
//! render one in `rts-ui`, which is a colour and a texture. This is a **port**
//! like the rest of the solver: the region, its defaults and the three formulas
//! below are `engine/rigid/gpurigid.ts`'s in the `rts-game` project, changed in
//! the same commit as this file (README rule 1).
//!
//! # Why a region at the tail of `world`, and not a fifth buffer
//!
//! Because the GPU backend cannot have one. Its device allows four storage
//! buffers per stage once a window is open, and the collide kernel already binds
//! four, so over there the materials went into the tail of `world`. A fifth array
//! here would make the surface of the two backends differ, and `rigid.step`
//! taking the same four arguments on both is what lets one program drive either.
//!
//! # Why absence means the LEGACY constants, not zero
//!
//! A `world` that stops at the static block is what every caller wrote before
//! this region existed — the bench, the solver's own tests, an older game build.
//! Reading zeros for them would switch gravity off in silence. So presence is
//! decided by LENGTH, once per step, and an absent region answers the constants
//! the solver had when they were constants: 9.8, no bounce, no drag, friction
//! 0.35, no floor. With those, every formula below reduces EXACTLY to the old
//! arithmetic (`0.35 / 0.35` is `1.0` in any float), which is what keeps the
//! measured `RUST x GPU = 0` parity meaning something.

/// Where the region starts: after the header and the FULL static capacity, not
/// after the statics in use — a fixed offset, so writing a material does not
/// depend on how many statics the scene has this frame.
pub(super) const MATERIALS_AT: usize = 4 + STATIC_CAPACITY * 8;
const STATIC_CAPACITY: usize = 256;
/// A static's record: restitution, friction, two unused.
const STATIC_RECORD: usize = 4;
/// A body's record: gravity, restitution, drag, friction, floor, three unused.
const BODY_RECORD: usize = 8;

/// The friction every surface had when friction was a constant. The two
/// friction constants of the solver were tuned against it, so it is the unit
/// they are scaled by.
const REFERENCE_FRICTION: f32 = 0.35;

/// One body's material.
#[derive(Clone, Copy)]
pub(super) struct Body {
    /// Downward acceleration, as a magnitude. Zero for a body nothing integrates.
    pub gravity: f32,
    pub restitution: f32,
    /// Fraction of velocity lost per second. Large enough and the body keeps no
    /// velocity at all — which is how a body with no integrator on the game side
    /// is pushed by contacts without ever coasting.
    pub drag: f32,
    pub friction: f32,
    /// The height the body's CENTRE rests at on the implicit floor. Anything at
    /// or below `NO_FLOOR` switches it off.
    pub floor: f32,
}

/// One static's material.
#[derive(Clone, Copy)]
pub(super) struct Fixed {
    pub restitution: f32,
    pub friction: f32,
}

const NO_FLOOR: f32 = -1.0e8;

const LEGACY_BODY: Body = Body {
    gravity: 9.8,
    restitution: 0.0,
    drag: 0.0,
    friction: REFERENCE_FRICTION,
    floor: -1.0e30,
};
const LEGACY_FIXED: Fixed = Fixed { restitution: 0.0, friction: REFERENCE_FRICTION };

/// The material region of one step's `world`, or its absence.
#[derive(Clone, Copy)]
pub(super) struct Materials<'a> {
    region: Option<&'a [f32]>,
}

impl<'a> Materials<'a> {
    /// Present only when the region holds a record for EVERY body: a region
    /// that is half there would give the first bodies a material and the rest
    /// the legacy one, which runs and is wrong.
    pub fn of(world: &'a [f32], bodies: usize) -> Self {
        let needed = MATERIALS_AT + STATIC_CAPACITY * STATIC_RECORD + bodies * BODY_RECORD;
        Self { region: (world.len() >= needed).then(|| &world[MATERIALS_AT..]) }
    }

    pub fn body(&self, body: usize) -> Body {
        let Some(region) = self.region else { return LEGACY_BODY };
        let at = STATIC_CAPACITY * STATIC_RECORD + body * BODY_RECORD;
        Body {
            gravity: region[at],
            restitution: region[at + 1],
            drag: region[at + 2],
            friction: region[at + 3],
            floor: region[at + 4],
        }
    }

    pub fn fixed(&self, fixed: usize) -> Fixed {
        let Some(region) = self.region else { return LEGACY_FIXED };
        let at = fixed.min(STATIC_CAPACITY - 1) * STATIC_RECORD;
        Fixed { restitution: region[at], friction: region[at + 1] }
    }
}

impl Body {
    /// Gravity, the speed cap's caller aside, then drag: the order the game's
    /// own integrator uses, so a body handed from one to the other keeps its arc.
    pub fn drag_factor(&self, dt: f32) -> f32 {
        (1.0 - self.drag * dt).max(0.0)
    }

    /// The implicit floor: a scene with no ground still has one. Bounces with
    /// the body's own restitution, and a bounce too small to see is absorbed so
    /// the body comes to rest instead of trembling.
    ///
    /// Answers whether the body is standing on it, because a body may only fall
    /// asleep on something — see the sleep counter in `solver::mod`.
    pub fn rest_on_floor(&self, height: &mut f32, fall: &mut f32) -> bool {
        if self.floor <= NO_FLOOR || *height >= self.floor {
            return false;
        }
        *height = self.floor;
        if *fall < 0.0 {
            *fall = -*fall * self.restitution;
        }
        if fall.abs() < 0.15 {
            *fall = 0.0;
        }
        true
    }
}

/// How much of the approach speed comes back. The mean of the two, and nothing
/// below one unit a second: a resting contact that bounced would feed itself
/// forever, which is the tremor the restitution cut exists to stop.
pub(super) fn bounce(approach: f32, a: f32, b: f32) -> f32 {
    match approach < -1.0 {
        true => (a + b) * 0.5,
        false => 0.0,
    }
}

/// A tuned friction constant, scaled by the pair's friction relative to the
/// one it was tuned at. `strength` is what is LOST per step at the reference
/// (0.08 on the ground, 0.10 in a stack); the geometric mean is what lets ice
/// dominate a pair without making it frictionless.
pub(super) fn friction_loss(strength: f32, a: f32, b: f32) -> f32 {
    let relative = ((a / REFERENCE_FRICTION) * (b / REFERENCE_FRICTION)).max(0.0).sqrt();
    (strength * relative).clamp(0.0, 1.0)
}
