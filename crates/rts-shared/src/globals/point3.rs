//! `RtsePoint3` — an INHERITANCE proof for `#[rtse::class(..., extends = "...")]`.
//!
//! `RtsePoint3 extends RtsePoint`: the `extends` arg links the Registry parent so
//! `x instanceof RtsePoint` (and `instanceof Object`, via the proto-walk
//! fall-through) resolves for a `RtsePoint3` instance — the runtime instanceof
//! consults `class_registry::is_descendant_of` over the `Entry::Rtse` class-id.
//! (Inherited METHODS are done by composition + forwarding in the author's struct;
//! the concrete-downcast `Entry::Rtse` model has no parent-method-on-child dispatch.
//! This proof uses own fields to isolate the NEW engine feature: instanceof.)

/// A 3D point. Own fields (no `RtsePoint` composition here — this proof exercises
/// the `extends`/instanceof link, not forwarding).
#[rtse::class("RtsePoint3")]
#[derive(Clone)]
pub struct Point3 {
    x: f64,
    y: f64,
    z: f64,
}

#[rtse::class("RtsePoint3", extends = "RtsePoint")]
impl Point3 {
    #[rtse::ctor]
    fn new(x: f64, y: f64, z: f64) -> Self {
        Point3 { x, y, z }
    }

    #[rtse::method]
    fn sum3(self: &Point3) -> f64 {
        self.x + self.y + self.z
    }
}
