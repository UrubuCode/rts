//! Classes.
//!
//! The construct that decides most of what a compiler can do, because it is the
//! only place a program says the shape of an object *before* one exists. A field
//! list is a layout; a method list is a dispatch table; an `extends` is an edge
//! in a hierarchy that can be closed. Everything the machine layer's shapes and
//! caches are for begins here.
//!
//! # Order is semantics, not tidiness
//!
//! A class body runs in a sequence that programs can observe, so the tree keeps
//! elements in source order and the lowering honours it:
//!
//! 1. **At definition**, once: the heritage is evaluated, then each static
//!    element — field initialiser or static block — runs in written order.
//! 2. **At construction**, per instance: private methods are installed first,
//!    then instance field initialisers run in written order. In a derived class
//!    this happens when `super()` returns, not at entry.
//!
//! A computed key is evaluated once, at definition, in written order — before
//! any initialiser runs, including its own.
//!
//! # `this` in a derived constructor does not exist yet
//!
//! Until `super()` returns there is no `this`, and reading it is a
//! `ReferenceError` rather than `undefined`. That is why [`Class::is_derived`]
//! is a question worth asking of the tree: it changes what the constructor's
//! prologue is allowed to assume.

use rts_cranelift::fault::Position;

use super::PropertyKey;
use super::expr::Expr;
use super::stmt::{Function, Stmt};
use crate::names::Name;

/// A class, declared or written as an expression.
///
/// One shape for both, because they differ in whether the name is introduced
/// into the surrounding scope and in nothing else that a lowering acts on.
#[derive(Clone, PartialEq, Debug)]
pub struct Class {
    /// What it is called. Absent for `export default class {}`.
    ///
    /// A named class expression also binds this name *inside* its own body, in a
    /// scope nothing outside can see — which is how a method can refer to the
    /// class even when the outer binding is later reassigned.
    pub name: Option<Name>,

    /// `extends expr` — an expression, not a name.
    ///
    /// `class X extends mixin(Base) {}` is legal, so the parent is whatever this
    /// evaluates to at definition time. `extends null` is also legal and makes
    /// instances have no prototype at all.
    pub heritage: Option<Expr>,

    /// The body, in source order. The order is the semantics.
    pub body: Vec<ClassElement>,

    /// Where it was written.
    pub at: Position,
}

impl Class {
    /// Whether this class has a parent, and so whether its constructor must call
    /// `super()` before it may touch `this`.
    pub fn is_derived(&self) -> bool {
        self.heritage.is_some()
    }

    /// Its constructor, if it declares one.
    ///
    /// Not a separate field: the spec makes it an ordinary method whose key is
    /// the name `constructor`, and a class may have none, in which case one is
    /// supplied — `super(...args)` for a derived class, empty otherwise.
    pub fn constructor(&self, names: &crate::names::Names) -> Option<&Method> {
        self.body.iter().find_map(|element| match element {
            ClassElement::Method(m) if m.is_constructor(names) => Some(m),
            _ => None,
        })
    }

    /// Everything that runs once, when the class is defined, in order.
    pub fn static_elements(&self) -> impl Iterator<Item = &ClassElement> {
        self.body.iter().filter(|e| e.runs_at_definition())
    }

    /// Everything that runs per instance, in order.
    pub fn instance_elements(&self) -> impl Iterator<Item = &ClassElement> {
        self.body.iter().filter(|e| e.runs_per_instance())
    }
}

/// One member of a class body.
#[derive(Clone, PartialEq, Debug)]
pub enum ClassElement {
    /// A method, getter, setter, or the constructor.
    Method(Method),

    /// A field, with or without an initialiser.
    Field(Field),

    /// `static { … }` — statements run once, at definition.
    ///
    /// `this` inside is the class itself. It cannot `return`, and it exists so
    /// that a static initialiser can do something a single expression cannot —
    /// including reaching the class's own private names, which is the only way
    /// to give something outside the class access to them.
    StaticBlock(Vec<Stmt>),
}

impl ClassElement {
    /// Whether this runs once, when the class is defined.
    pub fn runs_at_definition(&self) -> bool {
        match self {
            ClassElement::StaticBlock(_) => true,
            ClassElement::Field(f) => f.is_static,
            // A method is installed at definition, but nothing of it *runs*.
            ClassElement::Method(_) => false,
        }
    }

    /// Whether this runs once per instance created.
    pub fn runs_per_instance(&self) -> bool {
        matches!(self, ClassElement::Field(f) if !f.is_static)
    }

    /// The key this element is installed under.
    pub fn key(&self) -> Option<&ClassKey> {
        match self {
            ClassElement::Method(m) => Some(&m.key),
            ClassElement::Field(f) => Some(&f.key),
            ClassElement::StaticBlock(_) => None,
        }
    }
}

/// How a class member is named.
///
/// Private names are a separate case rather than a flag, because they are not
/// property keys at all: they are not reachable by `obj[k]`, not listed by any
/// reflection, and their scope is the class body. Treating `#x` as a property
/// named `"#x"` is the mistake this shape prevents.
#[derive(Clone, PartialEq, Debug)]
pub enum ClassKey {
    /// An ordinary property key, computed or not.
    Public(PropertyKey),
    /// `#name` — reachable only from inside the class body.
    Private(Name),
}

impl ClassKey {
    /// Whether this key is evaluated when the class is defined.
    ///
    /// True only for a computed public key. It runs once, in source order,
    /// before any initialiser — including the one it names.
    pub fn is_computed(&self) -> bool {
        matches!(self, ClassKey::Public(PropertyKey::Computed(_)))
    }

    /// Whether this member is private to the class body.
    pub fn is_private(&self) -> bool {
        matches!(self, ClassKey::Private(_))
    }
}

/// What kind of method this is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MethodKind {
    /// An ordinary method.
    Normal,
    /// `get k()`.
    Getter,
    /// `set k(v)`.
    Setter,
}

/// A method, getter, setter, or constructor.
#[derive(Clone, PartialEq, Debug)]
pub struct Method {
    /// Its name.
    pub key: ClassKey,
    /// Which kind.
    pub kind: MethodKind,
    /// The function.
    pub function: Box<Function>,
    /// Whether it lives on the class rather than on instances.
    pub is_static: bool,
}

impl Method {
    /// Whether this is the constructor.
    ///
    /// A static member named `constructor` is not one — it is a static method
    /// with an unfortunate name, and treating it as the constructor would make
    /// `new` run the wrong function.
    pub fn is_constructor(&self, names: &crate::names::Names) -> bool {
        if self.is_static || self.kind != MethodKind::Normal {
            return false;
        }
        match &self.key {
            ClassKey::Public(PropertyKey::Named(name)) => names.text(*name) == "constructor",
            _ => false,
        }
    }
}

/// A field declaration.
#[derive(Clone, PartialEq, Debug)]
pub struct Field {
    /// Its name.
    pub key: ClassKey,

    /// What it starts as.
    ///
    /// Absent means `undefined`, and the field still exists — declaring `x;`
    /// with no initialiser is not the same as never declaring it, because the
    /// property is created either way and that is what fixes the layout.
    ///
    /// The initialiser is an expression evaluated per instance, with `this`
    /// bound to that instance, in written order.
    pub value: Option<Expr>,

    /// Whether it lives on the class rather than on instances.
    pub is_static: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::syntax::{FunctionBody, PropertyKey};

    fn function(at: Position) -> Box<Function> {
        Box::new(Function {
            name: None,
            parameters: vec![],
            rest_parameter: None,
            body: FunctionBody::Block(vec![]),
            returns: None,
            captures_this: false,
            is_async: false,
            is_generator: false,
            at,
        })
    }

    fn method(names: &mut Names, text: &str, is_static: bool) -> Method {
        Method {
            key: ClassKey::Public(PropertyKey::Named(names.intern(text))),
            kind: MethodKind::Normal,
            function: function(Position::UNKNOWN),
            is_static,
        }
    }

    #[test]
    fn a_static_member_named_constructor_is_not_the_constructor() {
        let mut names = Names::new();
        let instance = method(&mut names, "constructor", false);
        let statik = method(&mut names, "constructor", true);

        assert!(instance.is_constructor(&names));
        assert!(
            !statik.is_constructor(&names),
            "new would otherwise run the wrong function"
        );
    }

    #[test]
    fn a_getter_named_constructor_is_not_one_either() {
        let mut names = Names::new();
        let mut getter = method(&mut names, "constructor", false);
        getter.kind = MethodKind::Getter;
        assert!(!getter.is_constructor(&names));
    }

    #[test]
    fn a_private_name_is_not_a_property_key() {
        let mut names = Names::new();
        let private = ClassKey::Private(names.intern("x"));
        let public = ClassKey::Public(PropertyKey::Named(names.intern("x")));

        assert_ne!(private, public, "#x is not the property named \"#x\"");
        assert!(private.is_private());
        assert!(!private.is_computed(), "a private name is never computed");
    }

    #[test]
    fn statics_run_at_definition_and_fields_run_per_instance() {
        let mut names = Names::new();
        let key = |n: &mut Names, t: &str| ClassKey::Public(PropertyKey::Named(n.intern(t)));

        let instance_field = ClassElement::Field(Field {
            key: key(&mut names, "a"),
            value: None,
            is_static: false,
        });
        let static_field = ClassElement::Field(Field {
            key: key(&mut names, "b"),
            value: None,
            is_static: true,
        });
        let block = ClassElement::StaticBlock(vec![]);
        let m = ClassElement::Method(method(&mut names, "m", false));

        assert!(instance_field.runs_per_instance());
        assert!(!instance_field.runs_at_definition());
        assert!(static_field.runs_at_definition());
        assert!(block.runs_at_definition());
        assert!(
            !m.runs_at_definition() && !m.runs_per_instance(),
            "a method is installed, not run"
        );
    }

    #[test]
    fn a_class_with_heritage_owes_a_super_call_before_it_may_touch_this() {
        let mut names = Names::new();
        let base = Class {
            name: Some(names.intern("Base")),
            heritage: None,
            body: vec![],
            at: Position::UNKNOWN,
        };
        let derived = Class {
            heritage: Some(Expr::new(
                crate::syntax::ExprKind::Ident(names.intern("Base")),
                Position::UNKNOWN,
            )),
            ..base.clone()
        };

        assert!(!base.is_derived());
        assert!(derived.is_derived());
    }
}
