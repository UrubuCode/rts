//! What shape a body has, as a value both sides of the boundary agree about.
//!
//! # Why this is not just the number in `vel.w`
//!
//! The buffer layout encodes shape as a float — `0` sphere, `1` box, and
//! `2 + hullId` for a hull, which is what `hullpack.ts` writes and what this
//! crate's solver reads. That encoding is the wire format and it stays; what it
//! is not is a good type to write a `match` against, because `2 + hullId` means
//! a `match` arm has to be a range and a backend that forgets that reads hull 3
//! as an unknown shape and refuses a scene that is fine.
//!
//! So the float is decoded once, here, into something a backend can be exhaustive
//! over. The two directions are inverses and there is a test that says so — the
//! encoding is an agreement with a file in another repository, and an agreement
//! with no test is a comment.

/// The kinds of shape a backend may be asked about.
///
/// A hull carries its id rather than its planes because the planes belong to the
/// MESH: a thousand instances of a model share one hull and differ only in
/// placement. Carrying the planes in the shape value would put a thousand copies
/// of them wherever shapes are stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeKind {
    /// A sphere, radius in the first half-extent.
    Sphere,
    /// An axis-aligned box.
    Box,
    /// The convex hull registered under this id. Never `0` — the encoding
    /// reserves `0` for "no hull", so a `Hull(0)` cannot be constructed by
    /// [`ShapeKind::decode`].
    Hull(u32),
    /// A capsule: declared in the TypeScript component and implemented by no
    /// backend yet. Present here so the number stays reserved — a backend
    /// answering `supports_shape(Capsule) == false` is the honest state, and it
    /// is better than the number being free for something else to take.
    Capsule,
}

/// The wire code for "sphere", and the base a hull id is offset from.
const CODE_SPHERE: f32 = 0.0;
const CODE_BOX: f32 = 1.0;
const HULL_BASE: f32 = 2.0;

impl ShapeKind {
    /// Decode the `w` of a `vel` record.
    ///
    /// Answers `None` for anything that is not a shape this build knows, rather
    /// than defaulting to a sphere. A default here is precisely the silent
    /// substitution README rule 9 forbids: a body meant to be a hull would fall
    /// through a floor as a point-sized sphere and nothing would say why.
    pub fn decode(code: f32) -> Option<Self> {
        if code == CODE_SPHERE {
            return Some(ShapeKind::Sphere);
        }
        if code == CODE_BOX {
            return Some(ShapeKind::Box);
        }
        if code >= HULL_BASE {
            let id = code - HULL_BASE;
            // A non-integral code is a corrupted buffer, not a hull between two
            // ids. Refusing is what makes a wrong write visible at the call
            // instead of three frames later as strange physics.
            if id.fract() != 0.0 {
                return None;
            }
            return Some(ShapeKind::Hull(id as u32 + 1));
        }
        None
    }

    /// The `w` this kind writes back.
    pub fn encode(self) -> f32 {
        match self {
            ShapeKind::Sphere => CODE_SPHERE,
            ShapeKind::Box => CODE_BOX,
            ShapeKind::Hull(id) => HULL_BASE + (id - 1) as f32,
            // A capsule has no wire code yet, and giving it one here would let a
            // buffer be written that no backend can read. It encodes as a sphere
            // — the shape that fits inside it — and the LOSS is why nothing
            // constructs a capsule today.
            ShapeKind::Capsule => CODE_SPHERE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_survives_the_trip_through_the_wire_code() {
        // The agreement is with `hullpack.ts` in another repository, so a test
        // that only checked one direction would let the two drift apart while
        // both looked right on their own side.
        for kind in [
            ShapeKind::Sphere,
            ShapeKind::Box,
            ShapeKind::Hull(1),
            ShapeKind::Hull(7),
            ShapeKind::Hull(255),
        ] {
            assert_eq!(ShapeKind::decode(kind.encode()), Some(kind), "{kind:?}");
        }
    }

    #[test]
    fn the_hull_ids_start_at_one_because_zero_means_no_hull() {
        // `hullpack.ts::hullShapeCode` adds 2 to the id and `hullIdOfShape`
        // answers 0 for a primitive. A `Hull(0)` here would encode to the box
        // code and read back as a box — a shape changing kind on a round trip.
        assert_eq!(ShapeKind::decode(2.0), Some(ShapeKind::Hull(1)));
        assert!(!matches!(ShapeKind::decode(2.0), Some(ShapeKind::Hull(0))));
    }

    #[test]
    fn an_unknown_code_is_refused_rather_than_read_as_a_sphere() {
        // The failure this prevents: a body meant to be a hull falls through the
        // floor as a point sphere, and nothing anywhere says why.
        assert_eq!(ShapeKind::decode(-1.0), None);
        assert_eq!(ShapeKind::decode(0.5), None);
        assert_eq!(ShapeKind::decode(2.5), None);
        assert_eq!(ShapeKind::decode(f32::NAN), None);
    }
}
