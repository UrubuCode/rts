//! What `declared.rs` promises about the declarations it renders.
//!
//! Its own file because the renderer reached the crate's 500-line ceiling
//! (rule 6 of `crates/rts-core/README.md`) when the modules a program imports
//! by specifier — `rts:serde` — were added to what it describes.

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
fn a_namespace_gets_no_constructor_interface() {
    // `new Math()` is a TypeError in the language. A `MathConstructor` with
    // a `new` in it tells a type checker the opposite.
    let text = render();
    assert!(
        text.contains("declare var Math: Math;") && !text.contains("MathConstructor"),
        "Math is a value with members, not something to construct; got:\n{text}"
    );
}

#[test]
fn every_class_reaches_the_global_scope() {
    // The point of the file. A previous version wrapped everything in
    // `declare namespace RtsProvided`, which type-checks cleanly and
    // describes an engine where `Buffer` is not a global — which is not this
    // one.
    let text = render();
    for class in CLASSES {
        if class.name.contains('.') || GENERIC_IN_LIB.contains(&class.name) {
            continue;
        }
        // Either binding form puts the name on the global scope; which one
        // depends on how the standard library already spells it
        // (`NAMESPACE_IN_LIB`).
        let bound = text.contains(&format!("declare var {}: ", class.name))
            || text.contains(&format!("declare namespace {} {{", class.name));
        assert!(
            bound,
            "{} is installed on globalThis and must be declared there",
            class.name
        );
    }
}

#[test]
fn a_parameter_is_optional_because_a_call_may_omit_it() {
    // `Buffer.alloc(8)` runs — the wrapper's unused slots arrive as
    // `undefined`. A declaration demanding all three refuses a program the
    // engine executes, which is the failure a `.d.ts` cannot have.
    let text = render();
    assert!(
        text.contains("?: "),
        "derived parameters are optional; got:\n{text}"
    );
    assert!(
        !text.contains("(size: number"),
        "no parameter should be spelled as required; got:\n{text}"
    );
}

#[test]
fn a_name_the_standard_library_parameterises_is_left_alone() {
    // Measured, not assumed: `interface Map` beside lib's `Map<K, V>` is
    // TS2428 and takes the whole file down with it.
    let text = render();
    assert!(
        !text.contains("interface Map {"),
        "Map is generic in the standard library; merging into it is TS2428"
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

#[test]
fn a_module_is_declared_under_its_specifier_and_not_on_the_global_scope() {
    // `serialize` is reached through `import … from "rts:serde"`. Declaring it
    // as a global would tell a type checker a name exists that does not.
    let text = render();
    assert!(
        text.contains("declare module \"rts:serde\" {")
            && text.contains("export function serialize(")
            && text.contains("export function deserialize(")
            && !text.contains("declare var serde"),
        "rts:serde must be a module declaration; got:\n{text}"
    );
}
