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

/// Layout do cabeçalho do buffer world (8 floats / 2 vec4s)
pub const WORLD_HEADER_FLOATS: usize = 8;
pub const WORLD_PARAM_MASS_SPLITTING: usize = 7;
pub const WORLD_PARAM_CONTACT_SOFTNESS: usize = 6;
pub const CONTACT_SOFTNESS_CODE_BASE: u32 = 1_000_000;
pub const WORLD_PARAM_DT: usize = 0;
pub const WORLD_PARAM_NUM_STATICS: usize = 1;
pub const WORLD_PARAM_CELL_SIZE: usize = 2;
pub const WORLD_PARAM_SUBSTEPS: usize = 3;
pub const WORLD_PARAM_LAYOUT_VERSION: usize = 4;
pub const WORLD_PARAM_ANY_MASK: usize = 5;

/// Registro de estático no world (pos/round: 4 floats, half/pad: 4 floats = 8 floats / 2 vec4s)
pub const STATIC_RECORD_FLOATS: usize = 8;
pub const STATIC_CAPACITY: usize = 256;

/// Where the region starts: after the header and the FULL static capacity, not
/// after the statics in use — a fixed offset, so writing a material does not
/// depend on how many statics the scene has this frame.
pub(super) const MATERIALS_AT: usize = WORLD_HEADER_FLOATS + STATIC_CAPACITY * STATIC_RECORD_FLOATS;
/// A static's record: restitution, friction, two unused.
const STATIC_RECORD: usize = 4;
/// A body's record: gravity, restitution, drag, friction, floor, three unused.
const BODY_RECORD: usize = 8;

/// The friction every surface had when friction was a constant. The two
/// friction constants of the solver were tuned against it, so it is the unit
/// they are scaled by.
const REFERENCE_FRICTION: f32 = 0.35;

/// Static body type: does not move and is not moved.
pub const BODY_STATIC: f32 = 1.0;
/// Kinematic body type: moves purely by velocity, ignores forces and impulses.
pub const BODY_KINEMATIC: f32 = 2.0;
/// Dynamic body type: fully simulated under gravity, contacts and impulses.
pub const BODY_DYNAMIC: f32 = 3.0;
/// Canonical version of the physics world layout.
pub const PHYSICS_LAYOUT_VERSION: f32 = 1.0;

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
    /// 1.0 = static, 2.0 = kinematic, 3.0 = dynamic (0.0 unassigned maps to 3.0).
    pub body_type: f32,
    pub layer: u32,
    pub mask: u32,
}

/// One static's material.
#[derive(Clone, Copy)]
pub(super) struct Fixed {
    pub restitution: f32,
    pub friction: f32,
    pub layer: u32,
    pub mask: u32,
}

const NO_FLOOR: f32 = -1.0e8;

const LEGACY_BODY: Body = Body {
    gravity: 9.8,
    restitution: 0.0,
    drag: 0.0,
    friction: REFERENCE_FRICTION,
    floor: -1.0e30,
    body_type: BODY_DYNAMIC,
    layer: 1,
    mask: 0xFFFF_FFFF,
};

const LEGACY_FIXED: Fixed = Fixed {
    restitution: 0.0,
    friction: REFERENCE_FRICTION,
    layer: 1,
    mask: 0xFFFF_FFFF,
};

/// The material region of one step's `world`, or its absence.
#[derive(Clone, Copy)]
pub(super) struct Materials<'a> {
    region: Option<&'a [f32]>,
    any_mask: bool,
    bodies: usize,
}

impl<'a> Materials<'a> {
    /// Present only when the region holds a record for EVERY body: a region
    /// that is half there would give the first bodies a material and the rest
    /// the legacy one, which runs and is wrong.
    pub fn of(world: &'a [f32], bodies: usize) -> Self {
        let any_mask = if world.len() > WORLD_PARAM_ANY_MASK {
            world[WORLD_PARAM_ANY_MASK] > 0.5
        } else {
            false
        };
        let needed = MATERIALS_AT + STATIC_CAPACITY * STATIC_RECORD + bodies * BODY_RECORD;
        Self {
            region: (world.len() >= needed).then(|| &world[MATERIALS_AT..]),
            any_mask,
            bodies,
        }
    }

    #[inline]
    fn orientation(&self, at: usize) -> [f32; 4] {
        let Some(region) = self.region else {
            return [0.0, 0.0, 0.0, 1.0];
        };
        let base = STATIC_CAPACITY * STATIC_RECORD + self.bodies * BODY_RECORD;
        if region.len() < base + (STATIC_CAPACITY + self.bodies) * 4
            || at + 4 > (STATIC_CAPACITY + self.bodies) * 4
        {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let q = base + at;
        [region[q], region[q + 1], region[q + 2], region[q + 3]]
    }

    #[inline]
    pub fn body_orientation(&self, body: usize) -> [f32; 4] {
        self.orientation(STATIC_CAPACITY * 4 + body * 4)
    }

    #[inline]
    pub fn fixed_orientation(&self, fixed: usize) -> [f32; 4] {
        self.orientation(fixed.min(STATIC_CAPACITY - 1) * 4)
    }

    #[inline]
    pub fn any_mask(&self) -> bool {
        self.any_mask
    }

    #[inline]
    pub fn layer_mask(&self, body: usize) -> (u32, u32) {
        let Some(region) = self.region else {
            return (LEGACY_BODY.layer, LEGACY_BODY.mask);
        };
        let at = STATIC_CAPACITY * STATIC_RECORD + body * BODY_RECORD;
        let layer = region[at + 6].to_bits();
        let mask = region[at + 7].to_bits();
        (layer, mask)
    }

    /// Só restituição e atrito do corpo: o que o laço de contato lê do OUTRO
    /// corpo. É o caminho quente da cena densa (um acesso por contato), e
    /// `body()` monta os oito campos e decide o `body_type` para descartar seis
    /// deles. Trocar um pelo outro tirou metade da regressão da issue #5.
    #[inline]
    pub fn restitution_friction(&self, body: usize) -> (f32, f32) {
        let Some(region) = self.region else {
            return (LEGACY_BODY.restitution, LEGACY_BODY.friction);
        };
        let at = STATIC_CAPACITY * STATIC_RECORD + body * BODY_RECORD;
        (region[at + 1], region[at + 3])
    }

    pub fn body(&self, body: usize) -> Body {
        let Some(region) = self.region else {
            return LEGACY_BODY;
        };
        let at = STATIC_CAPACITY * STATIC_RECORD + body * BODY_RECORD;
        let raw_type = region[at + 5];
        let body_type = if raw_type == BODY_STATIC {
            BODY_STATIC
        } else if raw_type == BODY_KINEMATIC {
            BODY_KINEMATIC
        } else {
            BODY_DYNAMIC
        };
        let layer = region[at + 6].to_bits();
        let mask = region[at + 7].to_bits();
        Body {
            gravity: region[at],
            restitution: region[at + 1],
            drag: region[at + 2],
            friction: region[at + 3],
            floor: region[at + 4],
            body_type,
            layer,
            mask,
        }
    }

    #[inline]
    pub fn fixed_layer_mask(&self, fixed: usize) -> (u32, u32) {
        let Some(region) = self.region else {
            return (LEGACY_FIXED.layer, LEGACY_FIXED.mask);
        };
        let at = fixed.min(STATIC_CAPACITY - 1) * STATIC_RECORD;
        let layer = region[at + 2].to_bits();
        let mask = region[at + 3].to_bits();
        (layer, mask)
    }

    pub fn fixed(&self, fixed: usize) -> Fixed {
        let Some(region) = self.region else {
            return LEGACY_FIXED;
        };
        let at = fixed.min(STATIC_CAPACITY - 1) * STATIC_RECORD;
        let layer = region[at + 2].to_bits();
        let mask = region[at + 3].to_bits();
        Fixed {
            restitution: region[at],
            friction: region[at + 1],
            layer,
            mask,
        }
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
    let relative = ((a / REFERENCE_FRICTION) * (b / REFERENCE_FRICTION))
        .max(0.0)
        .sqrt();
    (strength * relative).clamp(0.0, 1.0)
}

#[cfg(test)]
mod orientation_tests {
    use super::*;

    #[test]
    fn appended_orientations_are_read_and_legacy_worlds_default_to_identity() {
        let bodies = 2;
        let material_words = STATIC_CAPACITY * STATIC_RECORD + bodies * BODY_RECORD;
        let old_len = MATERIALS_AT + material_words;
        let mut old = vec![0.0; old_len];
        let legacy = Materials::of(&old, bodies);
        assert_eq!(legacy.body_orientation(0), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(legacy.fixed_orientation(0), [0.0, 0.0, 0.0, 1.0]);

        old.resize(old_len + (STATIC_CAPACITY + bodies) * 4, 0.0);
        let tail = material_words;
        old[MATERIALS_AT + tail + STATIC_CAPACITY * 4 + 4
            ..MATERIALS_AT + tail + STATIC_CAPACITY * 4 + 8]
            .copy_from_slice(&[0.0, 0.25, 0.0, 0.96824586]);
        old[MATERIALS_AT + tail + 8..MATERIALS_AT + tail + 12]
            .copy_from_slice(&[0.0, 0.5, 0.0, 0.8660254]);
        let oriented = Materials::of(&old, bodies);
        assert_eq!(oriented.fixed_orientation(2), [0.0, 0.5, 0.0, 0.8660254]);
        assert_eq!(oriented.body_orientation(1), [0.0, 0.25, 0.0, 0.96824586]);
    }
}
