//! What a class body may not contain.
//!
//! These are rules about a *set of members*, which is why they are here rather
//! than on any node. "At most one constructor" is not a property a method has;
//! it is a property the body has, and there is nothing for a `Method` to carry
//! that would say it.
//!
//! The three names that behave unlike any other are `constructor`, `prototype`
//! and `#constructor`, and each is special for its own reason rather than by a
//! shared rule:
//!
//! - `constructor` is the function `new` runs. A second one would silently win
//!   or silently lose, and a field of that name would overwrite it with a value.
//! - `prototype` on the class object already exists and is not writable, so a
//!   static member of that name has nowhere to go.
//! - `#constructor` is reserved outright, so that no private name can be made to
//!   look like the one member that is not a property at all.

use crate::names::Names;
use crate::syntax::{Class, ClassElement, ClassKey, MethodKind, PropertyKey};

/// Check one class body, and say what is wrong with it.
pub(super) fn check(class: &Class, names: &Names) -> Option<String> {
    let mut constructors = 0;

    for element in &class.body {
        let statically = match element {
            ClassElement::Method(method) => method.is_static,
            ClassElement::Field(field) => field.is_static,
            ClassElement::StaticBlock(_) => continue,
        };

        let key = element.key().expect("a method or a field is named");
        if let ClassKey::Private(name) = key
            && names.text(*name) == "#constructor"
        {
            return Some("`#constructor` cannot name a class member".to_owned());
        }

        let Some(text) = static_text(key, names) else {
            // A computed key is not known until the class is defined, and the
            // rules below are early errors — so a computed key is exempt from
            // all of them. `class C { ["constructor"]() {} }` is legal.
            continue;
        };

        if statically && text == "prototype" {
            return Some("a static class member cannot be named `prototype`".to_owned());
        }

        if statically || text != "constructor" {
            continue;
        }

        match element {
            // The name is only the constructor when it is a plain method: a
            // getter, a setter, a generator or an async function named
            // `constructor` is not a constructor with a modifier, it is a
            // member that cannot exist.
            ClassElement::Method(method) => {
                if method.kind != MethodKind::Normal {
                    return Some("the constructor cannot be a getter or a setter".to_owned());
                }
                if method.function.is_async || method.function.is_generator {
                    return Some(
                        "the constructor cannot be an async function or a generator".to_owned(),
                    );
                }
                constructors += 1;
                if constructors > 1 {
                    return Some("a class has more than one constructor".to_owned());
                }
            }
            // A field named `constructor` would overwrite the function `new`
            // runs with a value, per instance.
            ClassElement::Field(_) => {
                return Some("a class field cannot be named `constructor`".to_owned());
            }
            ClassElement::StaticBlock(_) => unreachable!("skipped above"),
        }
    }

    None
}

/// The text of a key that is known before the class is defined.
///
/// `None` for a computed key and for a private name. Both are deliberate: a
/// computed key is not decided until definition time, and a private name is not
/// a property key at all, so neither can collide with `constructor` or
/// `prototype`.
fn static_text<'a>(key: &ClassKey, names: &'a Names) -> Option<&'a str> {
    match key {
        ClassKey::Public(PropertyKey::Named(name)) => Some(names.text(*name)),
        ClassKey::Public(PropertyKey::Computed(_)) | ClassKey::Private(_) => None,
    }
}
