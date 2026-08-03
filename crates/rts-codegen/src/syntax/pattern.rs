//! Destructuring, in both of the roles the language gives it.
//!
//! `let [a, b] = xs` and `[a, b] = xs` are written almost identically and are
//! not the same operation. The first introduces two names. The second writes to
//! two targets, and a target is any expression that can be assigned to — so
//! `({ a: obj.x } = src)` is legal, and a type that only holds names cannot say
//! it.
//!
//! One tree serves both, with the difference at the leaf: [`Pattern::Name`]
//! introduces, [`Pattern::Target`] writes. Everything above the leaf — nesting,
//! defaults, rest, holes — is shared, because it genuinely is.
//!
//! # What this shape makes impossible
//!
//! Two early errors are absent from the type rather than described in it:
//!
//! - **Rest comes last.** It is a separate field, not an element that could sit
//!   anywhere in the list.
//! - **Rest takes no default.** It is a bare [`Pattern`], not an [`Element`].
//!
//! A rule that cannot be written down wrong does not need a checker, and does
//! not need anyone to remember it.
//!
//! # The two halves read from different things
//!
//! An array pattern **iterates**: it calls `[Symbol.iterator]()` and steps.
//! Which is why `const [a] = new Set([1])` works, why a hole still consumes a
//! step, and why the iterator has to be closed when the pattern took fewer
//! elements than were on offer.
//!
//! An object pattern **reads properties**. No iterator, no order requirement
//! beyond the source order of the keys themselves.

use super::PropertyKey;
use super::expr::Expr;
use crate::names::Name;

/// A destructuring pattern.
#[derive(Clone, PartialEq, Debug)]
pub enum Pattern {
    /// A name to introduce. The binding leaf.
    Name(Name),

    /// A place to write. The assignment leaf.
    ///
    /// Any expression that can be assigned to — `obj.x`, `arr[i]`, `obj[k()]`.
    /// Only reachable from a destructuring **assignment**; a declaration whose
    /// pattern contains one of these is not a declaration.
    Target(Box<Expr>),

    /// `{ a, b: c, ...rest }`.
    Object(ObjectPattern),

    /// `[a, , b, ...rest]`.
    Array(ArrayPattern),
}

impl Pattern {
    /// Whether this pattern only introduces names, and so may be a declaration,
    /// a parameter, or a `catch` binding.
    ///
    /// False as soon as any leaf anywhere inside it is a [`Pattern::Target`].
    /// Checked rather than assumed because the two roles share this type: the
    /// parser reinterprets one cover grammar into either, and reinterpreting
    /// into the wrong one is the mistake this answers.
    pub fn is_valid_binding(&self) -> bool {
        match self {
            Pattern::Name(_) => true,
            Pattern::Target(_) => false,
            Pattern::Object(o) => {
                o.properties
                    .iter()
                    .all(|p| p.value.pattern.is_valid_binding())
                    && o.rest.as_ref().is_none_or(|r| r.is_valid_binding())
            }
            Pattern::Array(a) => {
                a.elements
                    .iter()
                    .flatten()
                    .all(|e| e.pattern.is_valid_binding())
                    && a.rest.as_ref().is_none_or(|r| r.is_valid_binding())
            }
        }
    }

    /// Whether reading a value through this pattern consumes an iterator.
    ///
    /// True for an array pattern at any depth. It decides whether the lowering
    /// owes an `IteratorClose` on every early exit — the obligation a
    /// straightforward implementation forgets, and whose absence means a
    /// generator's `finally` never runs.
    pub fn iterates(&self) -> bool {
        match self {
            Pattern::Array(_) => true,
            Pattern::Name(_) | Pattern::Target(_) => false,
            Pattern::Object(o) => {
                o.properties.iter().any(|p| p.value.pattern.iterates())
                    || o.rest.as_ref().is_some_and(|r| r.iterates())
            }
        }
    }

    /// Every name this pattern introduces, in source order.
    ///
    /// Empty for a pure assignment pattern. Used to populate a scope, so the
    /// order matters: it is the order the initialisers run in, which is the
    /// order their side effects happen in.
    pub fn bound_names(&self, out: &mut Vec<Name>) {
        match self {
            Pattern::Name(name) => out.push(*name),
            Pattern::Target(_) => {}
            Pattern::Object(o) => {
                for property in &o.properties {
                    property.value.pattern.bound_names(out);
                }
                if let Some(rest) = &o.rest {
                    rest.bound_names(out);
                }
            }
            Pattern::Array(a) => {
                for element in a.elements.iter().flatten() {
                    element.pattern.bound_names(out);
                }
                if let Some(rest) = &a.rest {
                    rest.bound_names(out);
                }
            }
        }
    }
}

/// One slot of a pattern, and what it falls back to.
#[derive(Clone, PartialEq, Debug)]
pub struct Element {
    /// Where the value goes.
    pub pattern: Pattern,

    /// What is used when the value read was `undefined`.
    ///
    /// `undefined` specifically, not absent and not falsy: `[a = 1] = [null]`
    /// binds `null`, because `null` is a value that was there. The default is
    /// evaluated only when it is needed, so `[a = f()] = [1]` never calls `f`.
    pub default: Option<Expr>,
}

impl Element {
    /// A slot with no default.
    pub fn new(pattern: Pattern) -> Self {
        Self {
            pattern,
            default: None,
        }
    }

    /// A slot that falls back to `default`.
    pub fn with_default(pattern: Pattern, default: Expr) -> Self {
        Self {
            pattern,
            default: Some(default),
        }
    }
}

/// `[a, , b, ...rest]`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ArrayPattern {
    /// The slots, in order. `None` is a hole.
    ///
    /// A hole still takes a step from the iterator and binds nothing — which is
    /// why it is an absent element rather than an omitted one. Dropping holes
    /// would shift every element after them onto the wrong value.
    pub elements: Vec<Option<Element>>,

    /// `...rest`, which gathers whatever the iterator has left.
    ///
    /// A bare pattern: rest takes no default, and this type cannot express one.
    pub rest: Option<Box<Pattern>>,
}

/// `{ a, b: c, ...rest }`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ObjectPattern {
    /// The properties read, in source order.
    pub properties: Vec<PatternProperty>,

    /// `...rest`, an object of the own enumerable properties not already named.
    ///
    /// The language restricts this further than the type does: an object rest
    /// target may not itself be a pattern. Left to a check rather than the type
    /// because the restriction is on the *spelling*, and the same `Pattern`
    /// value is legal in every other position.
    pub rest: Option<Box<Pattern>>,
}

/// One property read by an object pattern.
#[derive(Clone, PartialEq, Debug)]
pub struct PatternProperty {
    /// Which property is read.
    ///
    /// May be computed, and a computed key is an expression that runs — in
    /// source order, before the property it names is read.
    pub key: PropertyKey,

    /// Where it goes, and what it falls back to.
    pub value: Element,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::syntax::{Expr, ExprKind};
    use rts_cranelift::fault::Position;

    fn name(names: &mut Names, text: &str) -> Pattern {
        Pattern::Name(names.intern(text))
    }

    #[test]
    fn a_pattern_holding_a_target_is_not_a_binding() {
        let mut names = Names::new();
        let object = Expr::new(ExprKind::Ident(names.intern("obj")), Position::UNKNOWN);

        let binding = Pattern::Array(ArrayPattern {
            elements: vec![Some(Element::new(name(&mut names, "a")))],
            rest: None,
        });
        let assignment = Pattern::Array(ArrayPattern {
            elements: vec![Some(Element::new(Pattern::Target(Box::new(Expr::new(
                ExprKind::Member {
                    object: Box::new(object),
                    property: names.intern("x"),
                    optional: false,
                },
                Position::UNKNOWN,
            )))))],
            rest: None,
        });

        assert!(binding.is_valid_binding());
        assert!(
            !assignment.is_valid_binding(),
            "({{ a: obj.x }} = src) is legal, and `let` cannot say it"
        );
    }

    #[test]
    fn a_hole_takes_a_step_and_binds_nothing() {
        let mut names = Names::new();
        let pattern = Pattern::Array(ArrayPattern {
            elements: vec![None, Some(Element::new(name(&mut names, "second")))],
            rest: None,
        });

        let mut bound = Vec::new();
        pattern.bound_names(&mut bound);
        assert_eq!(bound.len(), 1, "one name from two slots");
        assert!(pattern.iterates());
    }

    #[test]
    fn an_object_pattern_reads_properties_until_it_nests_an_array_one() {
        let mut names = Names::new();
        let key = PropertyKey::Named(names.intern("a"));

        let flat = Pattern::Object(ObjectPattern {
            properties: vec![PatternProperty {
                key: key.clone(),
                value: Element::new(name(&mut names, "a")),
            }],
            rest: None,
        });
        assert!(!flat.iterates(), "properties are read, not stepped");

        let nested = Pattern::Object(ObjectPattern {
            properties: vec![PatternProperty {
                key,
                value: Element::new(Pattern::Array(ArrayPattern::default())),
            }],
            rest: None,
        });
        assert!(
            nested.iterates(),
            "the array inside still owes an IteratorClose"
        );
    }

    #[test]
    fn names_come_out_in_the_order_their_initialisers_run() {
        let mut names = Names::new();
        let first = names.intern("first");
        let second = names.intern("second");
        let rest = names.intern("rest");

        let pattern = Pattern::Array(ArrayPattern {
            elements: vec![
                Some(Element::new(Pattern::Name(first))),
                Some(Element::new(Pattern::Name(second))),
            ],
            rest: Some(Box::new(Pattern::Name(rest))),
        });

        let mut bound = Vec::new();
        pattern.bound_names(&mut bound);
        assert_eq!(bound, vec![first, second, rest]);
    }
}
