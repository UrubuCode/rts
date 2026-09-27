//! `m.get(k)`, `m.has(k)`, `m.set(k, v)`, `s.has(v)`, `s.add(v)`, `a.push(v)`
//! as one entry point each, decided in this crate.
//!
//! The rule `emit/math` states, one step further: these are METHODS on a
//! receiver nothing here has proved, so the compiler cannot know the member it
//! reaches. What it can know is that the program leaves `Map`, `Set` and
//! `Array` as the language defines them — never written, never passed as a
//! value, never reached through `.prototype`, never extended — which is
//! `primordial::only_a_base` with the prototype refusal. Under that proof a
//! receiver that IS a map has exactly the language's `get`, and a receiver that
//! is not one has whatever it has. So the entry checks the receiver's brand in
//! the runtime and answers a real instance from its table, with the property
//! read and the dispatch removed; anything else takes the method it actually
//! has, through `direct_call::through_the_method`, which is the call this
//! emitter would have made.
//!
//! Each of these cost 42 to 107 ns as a call for a body that is one hash probe
//! or one `Vec::push` (`bench/analytic.ts`, 2026-09-26). Why exactly these six:
//! they are the collection operations a program writes in a loop, and each is a
//! member whose name a plain object rarely shares — the fallback exists for the
//! one that does.

mod body;

pub(super) use body::emit;
pub(crate) use body::shape_of;
