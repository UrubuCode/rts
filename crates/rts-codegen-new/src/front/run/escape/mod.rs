//! TIER-0 ESCAPE ANALYSIS — scalar-replace a `new C(...)` that never leaves the
//! function (`RTS_OPTIMIZATION.md` §5 Tier 4.1; `RTS_CLASS_IMPLEMENTATION.md` §7
//! C6, kernels M6 / H6).
//!
//! ## What it does
//!
//! For `const p = new Point(x, y)` where every use of `p` in the function body is
//! a READ of a declared field, the object is not built at all. Each field slot
//! becomes its own Cranelift `Variable`, and the allocation, the slot-0 shape-tag
//! store, the prototype link and the IC site all disappear:
//!
//! ```text
//!   const p = new Point(x, y);          →   v_p_x = <x coerced to the ctor's param repr>
//!   const d = p.x * p.x + p.y * p.y;        v_p_y = <y coerced to the ctor's param repr>
//!                                           d     = v_p_x*v_p_x + v_p_y*v_p_y
//! ```
//!
//! ## Why the front end, and why `Variable`
//!
//! Cranelift will never do this for us — the egraph RFC does not mention scalar
//! replacement of aggregates or escape analysis, and its optimizer cannot see
//! through the opaque allocation call in any case. But `cranelift-frontend`'s
//! `Variable` / SSA builder IS the mem2reg equivalent, and `rustc_codegen_cranelift`
//! does exactly this in its own front end, gated on an address-taken check. This
//! module is that address-taken check, spelled in JS terms.
//!
//! **GC interaction is a non-issue.** A `Variable` holding a handle is an ordinary
//! stack/register word, which the CONSERVATIVE scanner (`rts-natives`'s
//! `scan_all_roots`, which walks `rsp..stack_high` word by word) already treats as
//! a root. Nothing about moving a field out of a heap slot and into a register
//! changes what the collector can see. (Today's gate is narrower still — only
//! statically-numeric fields qualify, see [`recipe`] — so no scalar-replaced field
//! is even a candidate root.)
//!
//! ## The shape of the analysis
//!
//! Intraprocedural, no loops in the analysis itself, whitelist-based, one linear
//! pass over the function body. **This is TIER ZERO: conservative by
//! construction.** A wrong "does not escape" is a MISCOMPILE (a field write that
//! another alias should have seen, a `===` that must compare identity, a finalizer
//! that must run); a missed opportunity is only slower. So every rule here is
//! written to BAIL, and every construct the walker does not explicitly recognize
//! bails too.
//!
//! Two halves, deliberately separated:
//!
//! * [`recipe`] — a per-CLASS question, answered once at class-collection time:
//!   *is this constructor nothing but a sequence of `this.<field> = <pure numeric
//!   expr over the ctor params>`?* Anything else (a call, a `super()`, a read of
//!   `this`, a branch) means the constructor's effects cannot be replayed inline,
//!   so the class is permanently ineligible.
//! * [`scan`] — a per-LOCAL question, answered per function body: *does this
//!   local's value escape?* The bail list is in that module's doc.
//!
//! [`emit`] then does the lowering: the construction site and the field read.
//!
//! ## Ordering
//!
//! `RTS_CLASS_IMPLEMENTATION.md` §7 C6 records the ordering constraint as
//! **inline first, then EA, then SROA** — HotSpot does scalar replacement only
//! (never stack allocation) and reports the win is largely *enabled* by inlining.
//! This increment does the EA and the SROA together for the one shape where no
//! inlining is needed to see the whole story: the constructor's body is inlined by
//! [`recipe`] extracting it as a substitutable expression list. Method bodies are
//! NOT inlined, which is exactly why `p.method()` is a bail — the receiver would
//! be passed as `this` to a call, and this pass cannot see what that call does
//! with it.
//!
//! ## Calibration — do not over-claim
//!
//! Roslyn's self-build measured **16.1% of allocated objects not escaping** at run
//! time; Graal's partial escape analysis measured **−8.0% to −22.7% allocated
//! bytes**. The value probe's 138× is the *provably local* best case, NOT an
//! average, and must not be quoted as an expectation for this pass. The engine
//! number this is aimed at is `new P(x,y)` costing **632 ns/iter** against node's
//! **4.7 ns**, with the probe's EA rows putting a non-escaping field access at
//! **0.69–0.76 ns** against **1.50** for a heap one.
//!
//! TODO(measure): A/B this emission with `RTS_ESCAPE=1` / `RTS_ESCAPE=0` on the
//! object benchmark, `RTS_NO_PRELUDE_CACHE=1` on BOTH arms. No number here is a
//! measurement of THIS code.

mod emit;
mod recipe;
mod scan;

pub(crate) use emit::ScalarObj;
pub(crate) use recipe::ScalarCtor;
pub(in crate::front::run) use recipe::extract_scalar_ctor;
pub(in crate::front::run) use scan::scalar_locals;
