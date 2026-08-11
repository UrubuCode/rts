//! What a rigid-body backend is, so that more than one can exist.
//!
//! # Why a trait and not a `match`
//!
//! The crate shipped with one solver and `rts:rigid` called it directly. Adding
//! a second engine — Rapier, PhysX, Jolt, or the WGSL kernel the game already
//! has — as a branch in `surface.rs` would put three unrelated decisions in one
//! function: which engine, whether it can do the job, and what the answer means
//! for parity. The first is a choice, the second is a fact about the engine, and
//! the third is a fact about the *pair* of engines. A trait separates them.
//!
//! # The two boundaries, and why both exist
//!
//! There is a narrow place and a wide place to plug an engine in, and they buy
//! different things. Offering only one would have forced every integration to be
//! all-or-nothing.
//!
//! **[`NarrowPhase`] answers "do these two shapes touch, and how".** A backend
//! at this boundary replaces geometry only: the integration, the impulse, the
//! restitution cut and the support inheritance stay in [`crate::solver`]. This is
//! how a library like Parry earns its place without the project adopting an
//! engine — the shapes the gather model measured as unaffordable
//! (hull-against-hull: 2.2 billion dot products a frame) become available, and
//! **parity survives**, because the response never changed.
//!
//! **[`Backend`] answers "advance this scene".** A backend at this boundary owns
//! everything, which is the only shape a real engine can be plugged in as —
//! Rapier will not hand out its narrow phase and let someone else integrate,
//! because its accuracy comes from warm-starting contacts its own solver
//! generated. What it costs is parity, and [`ParityGroup`] is where that is said
//! out loud rather than discovered.
//!
//! # Capability is asked, never assumed
//!
//! [`Backend::supports`] exists because the alternative is a program that is
//! correct on one machine and wrong on another. The gather solver degrades a
//! hull-against-hull pair to a sphere — documented, deliberate, and measured as
//! the only affordable choice — while Rapier would resolve it properly. A caller
//! that needs the second and silently gets the first has no way to find out.
//!
//! So a request names what it needs, a backend answers whether it has it, and a
//! step that cannot run answers [`StepOutcome::Unsupported`] with the reason. The
//! refusal is the feature; `rules 5 and 9` of this crate's README are the same
//! rule at two levels.

use crate::shape::ShapeKind;

/// Which backends may be compared against each other by final position.
///
/// Two backends in the same group must land in the same place for the same
/// scene, and a divergence between them is a bug — that is what makes
/// `RUST × GPU = 0.000000` a test rather than a coincidence. Two backends in
/// **different** groups are not comparable at all, and a test that compares them
/// is measuring the difference between two solvers, which is not a defect.
///
/// This exists as a value and not as a comment because the mistake it prevents
/// is silent: someone extends the parity test with a third arm, it fails by
/// 0.4 units, and the day is spent looking for a bug in a backend that is
/// working exactly as designed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParityGroup {
    /// The gather/Jacobi formulation: this crate's solver and the WGSL kernel it
    /// is a port of. Same constants, same case order, same buffer layout.
    Gather,
    /// A solver with its own formulation, compared against nothing. Every
    /// third-party engine lands here, and that is not a demotion — it is
    /// answering a question with joints and continuous collision in it.
    Independent,
}

/// What a caller needs a backend to be able to do.
///
/// Asked BEFORE a step, so that "this backend cannot do what your scene needs"
/// is an answer and not a wrong simulation. Each field is something a real
/// backend in this workspace genuinely differs on today.
#[derive(Clone, Copy, Default, Debug)]
pub struct Needs {
    /// A convex hull must collide as a hull against another hull, rather than
    /// degrading to a sphere. The gather solver answers `false`; the reason is
    /// measured and is in `docs/colisores.md` §3.
    pub hull_against_hull: bool,
    /// A fast body must not tunnel through a thin one (continuous collision).
    pub continuous: bool,
    /// Bodies rotate and carry angular velocity.
    pub angular: bool,
    /// Joints, articulations, motors.
    pub joints: bool,
}

/// The result of asking a backend to advance a scene.
///
/// `Unsupported` carries a reason because a refusal a caller cannot act on is
/// only marginally better than a wrong answer: "it did not run" sends someone
/// looking at the buffers, while "this backend has no continuous collision"
/// sends them to the backend list.
#[derive(Debug)]
pub enum StepOutcome {
    /// The scene advanced. Carries how many sub-steps actually ran, which is not
    /// always what was asked: a backend with an internal budget may run fewer,
    /// and a caller comparing two backends by final position needs to know that
    /// before concluding they disagree.
    Advanced {
        /// How many sub-steps ran.
        sub_steps: u32,
    },
    /// The arguments were not a scene this backend can read — a length that is
    /// not a whole number of records, two buffers over the same bytes. Rule 5:
    /// nothing was touched.
    Refused {
        /// What about the arguments could not be read.
        why: &'static str,
    },
    /// The scene is readable and the backend cannot simulate it. This is the
    /// variant that exists so nothing has to approximate in silence.
    Unsupported {
        /// Which capability the scene needed and this backend lacks.
        needs: &'static str,
    },
}

/// The four buffers, as slices, exactly as the surface hands them over.
///
/// A struct rather than four arguments because a backend that takes them
/// positionally will one day take them in the wrong order, and `pos` and `vel`
/// have the same type and the same length. The layout of each is in the crate
/// README and is the GPU backend's, unchanged.
pub struct Scene<'a> {
    /// centre.xyz, sleep counter in w
    pub pos: &'a mut [f32],
    /// velocity.xyz, shape code in w
    pub vel: &'a mut [f32],
    /// half-extent.xyz, inverse mass in w
    pub ext: &'a mut [f32],
    /// dt, static count, cell size, sub-steps; then the static pairs
    pub world: &'a mut [f32],
}

impl Scene<'_> {
    /// How many bodies, from the buffer length. One place, because deriving it
    /// per backend is how two of them disagree about where the last body is.
    pub fn body_count(&self) -> usize {
        self.pos.len() / 4
    }
}

/// A whole rigid-body engine: buffers in, the same buffers advanced.
///
/// Implemented by this crate's gather solver, and the shape any third-party
/// engine would be adapted to. A backend may keep a world between calls (README
/// rule 4, as narrowed) — what it may not keep is anything the collector owns.
pub trait Backend: Send + Sync {
    /// How this backend is named in a request and in a diagnostic. Lowercase,
    /// stable, and the same string a program passes to select it: a display name
    /// that differs from the selector is one more thing to keep in step.
    fn name(&self) -> &'static str;

    /// Which set this backend may be compared against by final position.
    fn parity_group(&self) -> ParityGroup;

    /// Whether this backend can simulate a scene needing `needs`.
    ///
    /// Answered without looking at the scene, because it is a fact about the
    /// backend. A backend that can *sometimes* do something answers `false`:
    /// "sometimes" is what produces the machine-dependent program this whole
    /// arrangement exists to prevent.
    fn supports(&self, needs: &Needs) -> bool;

    /// Which shapes this backend understands. A shape it does not know is a
    /// refusal, never a substitution.
    fn supports_shape(&self, kind: ShapeKind) -> bool;

    /// Advance the scene. See [`StepOutcome`] for what the answers mean.
    fn step(&self, scene: &mut Scene<'_>, needs: &Needs) -> StepOutcome;
}

/// Contact between two shapes, in world space, in the convention the solver
/// uses: the normal points from A to B.
///
/// The same four numbers `hullContactLocal` answers on the TypeScript side and
/// `contact.rs` answers here — deliberately, so that a narrow-phase backend
/// slots in where those already sit rather than beside them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Contact {
    /// Normal x, pointing from A to B.
    pub nx: f32,
    /// Normal y.
    pub ny: f32,
    /// Normal z.
    pub nz: f32,
    /// How far the two overlap along the normal.
    pub depth: f32,
}

/// Geometry only: whether two placed shapes touch, and how.
///
/// A backend here does **not** integrate, does not apply an impulse and does not
/// decide what a resting contact means. That is what lets a library be adopted
/// for its collision detection alone, and it is why parity survives across this
/// boundary while it cannot across [`Backend`].
pub trait NarrowPhase: Send + Sync {
    /// How this narrow phase is named in a diagnostic.
    fn name(&self) -> &'static str;

    /// Whether this narrow phase handles the pair. Asked per pair and expected
    /// to be cheap — a `match` on two enums, not a computation.
    fn handles(&self, a: ShapeKind, b: ShapeKind) -> bool;

    /// The contact, or `None` when the two do not touch.
    ///
    /// `a` and `b` are placements the caller owns; the backend reads and never
    /// writes them, which is what makes this callable from `rayon` workers over
    /// a shared scene without a lock.
    fn contact(&self, a: &Placement<'_>, b: &Placement<'_>) -> Option<Contact>;
}

/// One shape, placed. What a narrow phase needs and nothing more.
pub struct Placement<'a> {
    /// Which shape this is.
    pub kind: ShapeKind,
    /// World centre.
    pub centre: [f32; 3],
    /// Half-extent, or radius in `[0]` for a sphere.
    pub half: [f32; 3],
    /// Yaw, which is the only rotation the scene graph composes today.
    pub yaw: f32,
    /// Planes as `nx, ny, nz, d`, for a hull. Empty for a primitive.
    pub planes: &'a [f32],
}
