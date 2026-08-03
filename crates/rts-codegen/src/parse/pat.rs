//! SWC patterns into ours, in both roles.
//!
//! A binding pattern and an assignment pattern are the same shape with a
//! different leaf, so the two entry points here differ only in what they do when
//! they reach the bottom: a binding introduces a name, an assignment target
//! writes to a place.

use swc_common::Spanned;
use swc_ecma_ast as swc;

use super::expr::{expr, property_key};
use super::{Cx, Result, position, unsupported};
use crate::syntax::{ArrayPattern, Element, ObjectPattern, Pattern, PatternProperty};

/// A binding pattern — the leaf introduces a name.
pub(crate) fn binding(cx: &mut Cx, pattern: &swc::Pat) -> Result<Pattern> {
    Ok(match pattern {
        swc::Pat::Ident(ident) => Pattern::Name(cx.name(&ident.id.sym)),

        swc::Pat::Array(array) => Pattern::Array(ArrayPattern {
            elements: array
                .elems
                .iter()
                .map(|element| match element {
                    None => Ok(None),
                    // A rest element is pulled out below, so anything left here
                    // is an ordinary slot.
                    Some(swc::Pat::Rest(_)) => Ok(None),
                    Some(slot) => binding_element(cx, slot).map(Some),
                })
                .collect::<Result<_>>()?,
            rest: rest_of(cx, &array.elems)?,
        }),

        swc::Pat::Object(object) => {
            let mut properties = Vec::new();
            let mut rest = None;
            for property in &object.props {
                match property {
                    swc::ObjectPatProp::Assign(assign) => properties.push(PatternProperty {
                        key: crate::syntax::PropertyKey::Named(cx.name(&assign.key.sym)),
                        value: Element {
                            pattern: Pattern::Name(cx.name(&assign.key.sym)),
                            default: match &assign.value {
                                Some(value) => Some(expr(cx, value)?),
                                None => None,
                            },
                        },
                    }),
                    swc::ObjectPatProp::KeyValue(pair) => properties.push(PatternProperty {
                        key: property_key(cx, &pair.key)?,
                        value: binding_element(cx, &pair.value)?,
                    }),
                    swc::ObjectPatProp::Rest(spread) => {
                        rest = Some(Box::new(binding(cx, &spread.arg)?));
                    }
                }
            }
            Pattern::Object(ObjectPattern { properties, rest })
        }

        swc::Pat::Assign(assign) => {
            // A default at the top of a pattern belongs to the slot holding it,
            // and the slot is built by the caller. Reaching one here means it
            // had no slot, which the grammar does not produce.
            return unsupported("a default with no slot to attach to", position(assign.span));
        }

        swc::Pat::Rest(rest) => {
            return unsupported("a rest element outside a list", position(rest.span));
        }

        swc::Pat::Expr(place) => {
            return unsupported(
                "an expression where a binding was expected",
                position(place.span()),
            );
        }

        swc::Pat::Invalid(invalid) => {
            return unsupported("a pattern SWC could not read", position(invalid.span));
        }
    })
}

/// A slot: a pattern and the default it falls back to.
pub(crate) fn binding_element(cx: &mut Cx, pattern: &swc::Pat) -> Result<Element> {
    Ok(match pattern {
        swc::Pat::Assign(assign) => Element {
            pattern: binding(cx, &assign.left)?,
            default: Some(expr(cx, &assign.right)?),
        },
        other => Element::new(binding(cx, other)?),
    })
}

/// The rest element of an array pattern, if it has one.
fn rest_of(cx: &mut Cx, elements: &[Option<swc::Pat>]) -> Result<Option<Box<Pattern>>> {
    for element in elements.iter().flatten() {
        if let swc::Pat::Rest(rest) = element {
            return Ok(Some(Box::new(binding(cx, &rest.arg)?)));
        }
    }
    Ok(None)
}

/// `[a, obj.b] = xs` — the leaf writes to a place.
pub(crate) fn array_target(cx: &mut Cx, array: &swc::ArrayPat) -> Result<Pattern> {
    let mut elements = Vec::new();
    let mut rest = None;
    for element in &array.elems {
        match element {
            None => elements.push(None),
            Some(swc::Pat::Rest(spread)) => {
                rest = Some(Box::new(target(cx, &spread.arg)?));
            }
            Some(slot) => elements.push(Some(target_element(cx, slot)?)),
        }
    }
    Ok(Pattern::Array(ArrayPattern { elements, rest }))
}

/// `({ a: obj.x } = src)`.
pub(crate) fn object_target(cx: &mut Cx, object: &swc::ObjectPat) -> Result<Pattern> {
    let mut properties = Vec::new();
    let mut rest = None;
    for property in &object.props {
        match property {
            swc::ObjectPatProp::Assign(assign) => properties.push(PatternProperty {
                key: crate::syntax::PropertyKey::Named(cx.name(&assign.key.sym)),
                value: Element {
                    pattern: Pattern::Name(cx.name(&assign.key.sym)),
                    default: match &assign.value {
                        Some(value) => Some(expr(cx, value)?),
                        None => None,
                    },
                },
            }),
            swc::ObjectPatProp::KeyValue(pair) => properties.push(PatternProperty {
                key: property_key(cx, &pair.key)?,
                value: target_element(cx, &pair.value)?,
            }),
            swc::ObjectPatProp::Rest(spread) => {
                rest = Some(Box::new(target(cx, &spread.arg)?));
            }
        }
    }
    Ok(Pattern::Object(ObjectPattern { properties, rest }))
}

fn target_element(cx: &mut Cx, pattern: &swc::Pat) -> Result<Element> {
    Ok(match pattern {
        swc::Pat::Assign(assign) => Element {
            pattern: target(cx, &assign.left)?,
            default: Some(expr(cx, &assign.right)?),
        },
        other => Element::new(target(cx, other)?),
    })
}

/// The assignment-role leaf: a name is still a name, everything else is a place.
fn target(cx: &mut Cx, pattern: &swc::Pat) -> Result<Pattern> {
    Ok(match pattern {
        swc::Pat::Ident(ident) => Pattern::Name(cx.name(&ident.id.sym)),
        swc::Pat::Array(array) => array_target(cx, array)?,
        swc::Pat::Object(object) => object_target(cx, object)?,
        swc::Pat::Expr(place) => Pattern::Target(Box::new(expr(cx, place)?)),
        other => return binding(cx, other),
    })
}
