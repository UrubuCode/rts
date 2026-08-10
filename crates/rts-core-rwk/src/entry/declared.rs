//! What this engine provides, as TypeScript declarations.
//!
//! `rts emit-types` used to render the OLD engine's registry, which is a
//! different set of classes reached by a different mechanism — so the `.d.ts` a
//! project type-checked against described a compiler that no longer runs
//! anything. This is the same view over what `#[rtse::class]` declares here.
//!
//! # One source, generated view
//!
//! Nothing in this file states what a member accepts. The attribute derives the
//! signature from the Rust one and captures the `///` beside it, exactly as it
//! already derives the wrapper and the install list — so a member that changes
//! shape changes its declaration in the same edit, and there is no second
//! spelling to forget. The repository rule is `CLAUDE.md`'s "one source,
//! generated views"; this is that rule applied to the type surface.
//!
//! # What this file DOES own, and why it is a list
//!
//! [`CLASSES`] names every declared class once. A proc macro sees one item and
//! cannot see its neighbours — the same reason `register_*` is a function
//! something else calls — and a link-time collection would order itself however
//! the linker felt, which is neither deterministic nor visible in a diff (the
//! machine's rule 13).
//!
//! A hand-written list drifts, so it is checked rather than trusted:
//! [`tests::every_registered_class_is_declared_here`] reads `global.rs` and
//! fails if a name that registration can produce is missing from this list.

/// Which half of a class a member is installed on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// On the prototype: an instance method.
    Prototype,
    /// On the constructor.
    Static,
    /// What `new C()` runs.
    Construct,
    /// A value property of the prototype.
    Constant,
    /// A value property of the constructor.
    StaticConstant,
}

/// One member, as a program writes it.
#[derive(Clone, Copy, Debug)]
pub struct Member {
    /// TypeScript, derived from the Rust signature by `#[rtse::class]`.
    pub signature: &'static str,
    /// The `///` comments the author wrote, joined.
    pub doc: &'static str,
    /// Where it is installed.
    pub role: Role,
}

/// One declared class or namespace.
#[derive(Clone, Copy, Debug)]
pub struct Class {
    /// The name JavaScript knows it by.
    pub name: &'static str,
    /// The `///` comments above the `impl` block.
    pub doc: &'static str,
    /// Whether it is a namespace — an object with members and no constructor.
    pub namespace: bool,
    /// The class it inherits from, if the derivation found a name.
    ///
    /// Derived from the path `extends` names, so it can be wrong in a way
    /// nothing here would notice — which is why [`render`] prints it only when
    /// it matches a class in [`CLASSES`].
    pub extends: Option<&'static str>,
    /// Its members, in declaration order.
    pub members: &'static [Member],
}

/// Every class `#[rtse::class]` declares in this crate.
///
/// Declaration order inside a class is the author's; the order HERE is
/// alphabetical, because this list is read by a person looking for a name.
pub const CLASSES: &[Class] = &[
    super::bigint_class::BIG_INT_CLASS_TYPES,
    super::buffer::BUFFER_TYPES,
    super::buffers::ARRAY_BUFFER_TYPES,
    super::buffers::ATOMICS_TYPES,
    super::buffers::DATA_VIEW_TYPES,
    super::buffers::SHARED_ARRAY_BUFFER_TYPES,
    super::collections::MAP_TYPES,
    super::collections::SET_TYPES,
    super::collections::WEAK_MAP_TYPES,
    super::collections::WEAK_REF_TYPES,
    super::collections::WEAK_SET_TYPES,
    super::date::DATE_TYPES,
    super::error::AGGREGATE_ERROR_TYPES,
    super::error::ERROR_TYPES,
    super::error::EVAL_ERROR_TYPES,
    super::error::RANGE_ERROR_TYPES,
    super::error::REFERENCE_ERROR_TYPES,
    super::error::SYNTAX_ERROR_TYPES,
    super::error::TYPE_ERROR_TYPES,
    super::error::URI_ERROR_TYPES,
    super::function_proto::FUNCTION_TYPES,
    super::generator::GENERATOR_TYPES,
    super::json::JSON_TYPES,
    super::list_iterator::ITERATOR_TYPES,
    super::list_iterator::LIST_ITERATOR_TYPES,
    super::math::MATH_TYPES,
    super::number::BOOLEAN_TYPES,
    super::number::NUMBER_TYPES,
    super::object_proto::OBJECT_PROTOTYPE_TYPES,
    super::promise::PROMISE_TYPES,
    super::proxy::PROXY_TYPES,
    super::reflect::REFLECT_TYPES,
];

/// The declarations, as one `.d.ts`.
///
/// # What it does not describe, stated in the file itself
///
/// The classes reached some other way — `String`, `Array`, `Object`, `RegExp`,
/// `Symbol`, the typed arrays — and the global functions. They are installed by
/// hand-written registrations rather than by the attribute, so nothing derives
/// their signatures and inventing them here would be a second source of exactly
/// the kind this file exists to avoid. A header says so, because a `.d.ts` that
/// silently omits `Array` reads as an engine without one.
pub fn render() -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED by `rts emit-types` from what `#[rtse::class]` declares.\n\
         // Do not edit: the signatures are derived from the Rust ones, and an\n\
         // edit here is undone by the next run.\n\
         //\n\
         // INCOMPLETE, and knowingly. It covers what the attribute declares;\n\
         // `String`, `Array`, `Object`, `RegExp`, `Symbol`, the typed arrays and\n\
         // the global functions are installed by hand-written registrations, so\n\
         // nothing derives their signatures and they are absent rather than\n\
         // guessed at.\n\
         //\n\
         // Inside a namespace, and NOT global. `Date`, `Math`, `Error` and the\n\
         // rest are declared by the standard library every project already\n\
         // loads, and a second global declaration of them is `error TS6200:\n\
         // definitions conflict` — one file, every project, refusing to compile.\n\
         // Measured with `tsc --noEmit` rather than assumed.\n\
         //\n\
         // So this answers what THIS engine provides, which the standard library\n\
         // cannot: it promises the whole language, and the point of this file is\n\
         // which part of that is really here.\n\n\
         declare namespace RtsProvided {\n\n",
    );
    out.push_str(&indented(&body()));
    out.push_str("}\n");
    out
}

/// Every class, one after another, before the enclosing namespace indents them.
fn body() -> String {
    let mut out = String::new();
    for class in CLASSES {
        // A name with a dot in it is a PATH, not a declarable name:
        // `Object.prototype` says where its members are installed, and
        // `declare namespace Object.prototype` would either merge with the
        // `Object` a type checker already has or be refused outright — and one
        // refused declaration takes the whole file down with it. Named in a
        // comment so it is visibly absent rather than silently.
        if class.name.contains('.') {
            out.push_str(&format!(
                "// `{}` is installed on an object a type checker already \
                 declares; its members are\n// absent here rather than merged \
                 into it.\n\n",
                class.name
            ));
            continue;
        }
        out.push_str(&jsdoc(class.doc, ""));
        // No `declare` on any of these: inside an ambient namespace every member
        // is already ambient and already exported, and writing it there is an
        // error rather than a redundancy.
        match class.namespace {
            // A namespace is a value, not a type: `Math` has no constructor and
            // no instances, so `class Math` would let a program write
            // `new Math()` and be believed by the type checker where the engine
            // raises.
            true => out.push_str(&format!("namespace {} {{\n", class.name)),
            false => match inherits(class) {
                Some(parent) => {
                    out.push_str(&format!("class {} extends {parent} {{\n", class.name))
                }
                None => out.push_str(&format!("class {} {{\n", class.name)),
            },
        }
        for member in class.members {
            out.push_str(&jsdoc(member.doc, "  "));
            out.push_str(&format!("  {}\n", spelled(class, member)));
        }
        out.push_str("}\n\n");
    }
    out
}

/// The same text, two spaces in, leaving blank lines blank.
fn indented(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        match line.is_empty() {
            true => out.push('\n'),
            false => out.push_str(&format!("  {line}\n")),
        }
    }
    out
}

/// The parent, but only when this engine declares it — see [`Class::extends`].
fn inherits(class: &Class) -> Option<&'static str> {
    let parent = class.extends?;
    CLASSES
        .iter()
        .any(|other| other.name == parent)
        .then_some(parent)
}

/// One member, in the syntax the body it sits in accepts.
///
/// A namespace body takes `export function`/`export const`; a class body takes
/// a bare signature, `static`, or `constructor`. The same member is spelled
/// differently in the two, which is why this takes the class.
fn spelled(class: &Class, member: &Member) -> String {
    if class.namespace {
        return match member.role {
            Role::Constant | Role::StaticConstant => format!("export const {};", member.signature),
            _ => format!("export function {};", member.signature),
        };
    }
    match member.role {
        // The derived signature is `convert(value: any): any` — the Rust name,
        // which a constructor does not have. Only its parameters survive.
        Role::Construct => format!("constructor({});", parameters(member.signature)),
        Role::Static | Role::StaticConstant => format!("static {};", member.signature),
        Role::Prototype | Role::Constant => format!("{};", member.signature),
    }
}

/// What is between the outermost parentheses of a signature.
fn parameters(signature: &str) -> &str {
    let opened = signature.find('(');
    let closed = signature.rfind(')');
    match (opened, closed) {
        (Some(open), Some(close)) if close > open => &signature[open + 1..close],
        _ => "",
    }
}

/// A doc comment, as a JSDoc block, or nothing when there is none.
fn jsdoc(doc: &str, indent: &str) -> String {
    if doc.is_empty() {
        return String::new();
    }
    let mut out = format!("{indent}/**\n");
    for line in doc.lines() {
        out.push_str(&format!("{indent} * {line}\n"));
    }
    out.push_str(&format!("{indent} */\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list in this file is hand-written, so it is the thing that drifts.
    /// `global.rs`'s match is what a program actually reaches, and every arm of
    /// it that names a `register_*` produced by the attribute must appear here —
    /// otherwise a class exists at run time and is absent from the types, which
    /// is the failure mode a generated `.d.ts` is supposed to make impossible.
    #[test]
    fn every_registered_class_is_declared_here() {
        let source = include_str!("global.rs");
        let mut missing = Vec::new();
        for line in source.lines() {
            let Some((left, right)) = line.split_once("=>") else {
                continue;
            };
            if !right.contains("register_") {
                continue;
            }
            let Some(name) = left.split('"').nth(1) else {
                continue;
            };
            if !CLASSES.iter().any(|class| class.name == name) {
                missing.push(name.to_owned());
            }
        }
        assert!(
            missing.is_empty(),
            "declared by `#[rtse::class]` and reachable as a global, but absent \
             from `CLASSES`, so `rts emit-types` would not describe it: {missing:?}"
        );
    }

    #[test]
    fn a_namespace_is_declared_as_one_and_not_as_a_class() {
        // `new Math()` is a TypeError in the language. A `.d.ts` saying
        // `declare class Math` tells a type checker the opposite.
        let text = render();
        assert!(
            text.contains("namespace Math {") && !text.contains("class Math"),
            "Math has no constructor; got:\n{text}"
        );
    }

    #[test]
    fn a_constructor_is_spelled_as_one() {
        let text = render();
        assert!(
            !text.contains("convert("),
            "a constructor keeps its parameters and loses the Rust name it was \
             written under; got:\n{text}"
        );
    }

    #[test]
    fn an_extends_naming_something_undeclared_is_dropped() {
        // Not a hypothetical: `extends` is derived from a function path, and the
        // derivation is the part that can be wrong. A `.d.ts` naming a type it
        // does not declare fails to compile, which would take the whole file
        // down over one class.
        for class in CLASSES {
            if let Some(parent) = inherits(class) {
                assert!(
                    CLASSES.iter().any(|other| other.name == parent),
                    "{} claims to extend {parent}, which is not declared",
                    class.name
                );
            }
        }
    }
}
