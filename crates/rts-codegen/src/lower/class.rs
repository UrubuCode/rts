//! A class, as the three things it is.
//!
//! # What a class is, once the sugar is gone
//!
//! A constructor function, an object to hold the methods, and a link between them.
//! Every one of the three is already expressible — a closure, an object, a property
//! write — so this file builds them and nothing new was needed for it.
//!
//! What it deliberately does NOT do is decide where the prototype link lives. The
//! write is `prototype`, by name, exactly as a program would write it: the machine
//! decides what an object's prototype slot is, and rule 2 says that is not asked here.
//!
//! # Why `extends` is refused
//!
//! `class X extends mixin(Base) {}` is legal, so the parent is whatever the expression
//! answers at DEFINITION time — that part is easy. What is not is the rest of what
//! `extends` brings: `super()` must run before `this` exists in a derived constructor,
//! `super.m()` reads from the home object rather than from the receiver, and the
//! prototype chain has two links to set rather than one. Each is a decision, and a
//! class that inherited without them would compile and would get `super` wrong.
//!
//! # Why a method is not a property holding a function
//!
//! A method is installed with a HOME OBJECT, which is what `super.x` inside it reads
//! from. A function stored under a key has none, and `super` there is a syntax error —
//! the object literal's lowering refuses a method for this reason and the same applies
//! here. So a class with no `extends` and no `super` is what lowers: a method that
//! cannot reach a home object does not need one.

use rts_mir::cfg::ValueId;

use super::{Lowering, Unsupported};
use crate::domain::{JsConst, JsPrim};
use crate::syntax::{Class, ClassElement, ClassKey, Expr, ExprKind, MethodKind, PropertyKey};

impl Lowering<'_> {
    /// Lowers a class declaration, binding its name.
    pub(super) fn class(&mut self, class: &Class, at: &Expr) -> Result<bool, Unsupported> {
        let Some(name) = class.name else {
            return Err(Unsupported::Statement(
                "an anonymous class declaration is only reachable as a default export",
            ));
        };
        let held = self.class_value(class, at)?;
        let of = self.type_of(held);
        let target = Expr {
            kind: ExprKind::Ident(name),
            at: at.at,
        };
        self.bind(name, held, of, &target)?;
        Ok(false)
    }

    /// The class itself: a constructor, a prototype, and the link.
    ///
    /// UNDER ANOTHER STAGE'S LAYOUT the class is that stage's own: a helper it compiled
    /// returns it, numbered at the class's position, and this calls the helper with this
    /// activation's receiver -- `emit/through_mir.rs::helper_of` says why.
    pub(super) fn class_value(&mut self, class: &Class, at: &Expr) -> Result<ValueId, Unsupported> {
        if self.outer.is_some() {
            let Some(helper) = self.callees.of_position(class.at) else {
                return Err(Unsupported::Expression(
                    "a class whose helper nothing numbered",
                ));
            };
            let made = self.closure(helper, at);
            let receiver = match self.lexical_this {
                true => None,
                false => Some(self.prim(JsPrim::ThisValue, Vec::new(), at)),
            };
            return Ok(self.call(rts_mir::cfg::Callee::Dynamic(made), receiver, Vec::new(), at));
        }
        if class.heritage.is_some() {
            return Err(Unsupported::Expression(
                "extends brings super, a home object and a second prototype link",
            ));
        }

        // THE CONSTRUCTOR. A class with no constructor written still has one — the
        // implicit one — and this stage has no function to name for it, so it is
        // refused rather than invented: a closure naming no function would be a value
        // with nothing behind it.
        let mut constructor = None;
        let mut methods = Vec::new();
        for element in &class.body {
            match element {
                ClassElement::Method(method) => {
                    if !matches!(method.kind, MethodKind::Normal) {
                        return Err(Unsupported::Expression(
                            "an accessor in a class body is a property descriptor",
                        ));
                    }
                    if method.is_static {
                        return Err(Unsupported::Expression(
                            "a static member lives on the class rather than on instances",
                        ));
                    }
                    let ClassKey::Public(PropertyKey::Named(key)) = &method.key else {
                        return Err(Unsupported::Expression(
                            "a private or computed member key is not a name a layout can hold",
                        ));
                    };
                    let Some(id) = self.callees.of_position(method.function.at) else {
                        return Err(Unsupported::Expression(
                            "a class member needs the module's numbering",
                        ));
                    };
                    // THE TREE ANSWERS THIS, not a comparison written here:
                    // `Method::is_constructor` already states that a static member and
                    // an accessor are never the constructor however they are spelled,
                    // and a second copy of that rule is a second place for it to drift.
                    match method.is_constructor(self.names) {
                        true => constructor = Some(id),
                        false => methods.push((*key, id)),
                    }
                }
                ClassElement::Field(_) => {
                    return Err(Unsupported::Expression(
                        "a field is initialised per instance, which the constructor runs",
                    ));
                }
                ClassElement::StaticBlock(_) => {
                    return Err(Unsupported::Expression(
                        "a static block runs once, when the class is defined",
                    ));
                }
            }
        }
        let Some(constructor) = constructor else {
            return Err(Unsupported::Expression(
                "a class with no constructor written needs the implicit one, which is not a function of the module",
            ));
        };

        let held = self.closure(constructor, at);
        // THE PROTOTYPE, and the methods written into it in source order — which is the
        // order that decides its layout, the same reason an object literal keeps its
        // pairs in order.
        let prototype = self.prim(JsPrim::NewObject, Vec::new(), at);
        for (key, id) in methods {
            let index = self.domain.constant(JsConst::Key(key));
            let named = self.declared(index, at);
            let method = self.closure(id, at);
            self.prim(JsPrim::FieldWrite, vec![prototype, named, method], at);
        }
        // THE LINK, written by name exactly as a program would write it. Where an
        // object's prototype slot lives is the machine's answer, not this one's.
        let index = self
            .domain
            .constant(JsConst::WellKnown(crate::domain::WellKnown::Prototype));
        let named = self.declared(index, at);
        self.prim(JsPrim::FieldWrite, vec![held, named, prototype], at);
        Ok(held)
    }
}
