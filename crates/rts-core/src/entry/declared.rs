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
    super::collections::FINALIZATION_REGISTRY_TYPES,
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
    super::iterator::ITERATOR_TYPES,
    super::list_iterator::LIST_ITERATOR_TYPES,
    // `Intl` and the seven services ON it. The namespace is what `global.rs`
    // registers; the services are properties of it rather than globals, and
    // they are listed because a declaration file has to name what a program can
    // reach — `new Intl.NumberFormat()` is reachable and `NumberFormat` alone
    // is not.
    super::intl::INTL_TYPES,
    super::intl::COLLATOR_TYPES,
    super::intl::DATE_TIME_FORMAT_TYPES,
    super::intl::LIST_FORMAT_TYPES,
    super::intl::NUMBER_FORMAT_TYPES,
    super::intl::PLURAL_RULES_TYPES,
    super::intl::RELATIVE_TIME_FORMAT_TYPES,
    super::intl::SEGMENTER_TYPES,
    super::math::MATH_TYPES,
    super::number::BOOLEAN_TYPES,
    super::number::NUMBER_TYPES,
    super::object_proto::OBJECT_PROTOTYPE_TYPES,
    super::promise::PROMISE_TYPES,
    super::proxy::PROXY_TYPES,
    super::reflect::REFLECT_TYPES,
];

/// Names the standard library declares with type parameters.
///
/// A `.d.ts` merging into `Map` without writing `<K, V>` is
/// `TS2428: all declarations of 'Map' must have identical type parameters` —
/// and one refused declaration takes the whole file with it. These seven are
/// what `tsc --noEmit --target es2022` actually rejected, listed rather than
/// guessed: `Generator` is NOT among them, because its parameters have
/// defaults, and a rule derived from "it looks generic" would have excluded it
/// wrongly.
///
/// They are skipped rather than parameterised. Writing `<K, V>` here means
/// matching the standard library's constraints and defaults EXACTLY, in every
/// TypeScript version, to add `any`-typed overloads to something it already
/// types precisely — cost with no reader.
const GENERIC_IN_LIB: &[&str] = &[
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    // `FinalizationRegistry<T>` in `lib.es2021.weakref.d.ts`, exactly as
    // `WeakRef<T>` above it — merging without repeating the parameter is TS2428
    // and takes the whole generated file down over one class.
    "FinalizationRegistry",
    "Promise",
    "Iterator",
];

/// Names the standard library declares as a NAMESPACE rather than as a variable
/// with an interface type.
///
/// `Math` and `JSON` are `interface Math` + `declare var Math: Math`; `Reflect`
/// is `declare namespace Reflect`. Merging into the wrong one of the two is
/// `TS2300: duplicate identifier`, which is how this list was found rather than
/// derived — the standard library is not consistent about it, so nothing here
/// can be.
const NAMESPACE_IN_LIB: &[&str] = &["Reflect"];

/// The declarations, as one `.d.ts`, on the global scope.
///
/// # Global, and what that costs
///
/// A program writes `Buffer.alloc(8)`, not `RtsProvided.Buffer.alloc(8)` —
/// these names ARE on `globalThis`, and a file that boxes them in a namespace
/// describes something the runtime does not have. That was the first version of
/// this and it was wrong.
///
/// Global by DECLARATION MERGING (`interface X` + `interface XConstructor` +
/// `declare var X`) rather than by `declare class X`, which is the form that
/// makes it possible at all. `declare class Date` beside the standard library's
/// own is `TS6200: definitions conflict`, for fifteen names at once; merging
/// adds to what is already there. Both measured with `tsc --noEmit`, at `es5`
/// and at `es2022`, rather than reasoned about.
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
    let mut out = String::from(
        "// GENERATED by `rts emit-types` from what `#[rtse::class]` declares.\n\
         // Do not edit: the signatures are derived from the Rust ones, and an\n\
         // edit here is undone by the next run.\n\
         //\n\
         // These names are GLOBAL — `Buffer.alloc(8)`, not `Rts.Buffer.alloc(8)`\n\
         // — because that is where the engine installs them. They are declared by\n\
         // interface merging rather than as classes: `declare class Date` beside\n\
         // the standard library's own is `TS6200: definitions conflict`, and\n\
         // merging adds to what is already there instead.\n\
         //\n\
         // INCOMPLETE, and knowingly. It covers what the attribute declares;\n\
         // `String`, `Array`, `Object`, `RegExp`, `Symbol`, the typed arrays and\n\
         // the global functions are installed by hand-written registrations, so\n\
         // nothing derives their signatures and they are absent rather than\n\
         // guessed at.\n\n",
    );
    out.extend(CLASSES.iter().map(declaration));
    out.push_str(&super::operators_declaration());
    for (specifier, class, values) in MODULES {
        out.push_str(&module(specifier, class, values));
    }
    out
}

/// The modules a program imports by specifier, and the namespace each one is.
///
/// Apart from [`CLASSES`] because these are not on the global scope: a program
/// reaches `serialize` through `import { serialize } from "rts:serde"`, and a
/// declaration putting it on `globalThis` would describe a name the runtime
/// does not have. The members are the attribute's, as everywhere else here.
///
/// The third column is what the attribute cannot derive: a value export that is
/// not a function. `rts:serde`'s `version` and `upgrade` are SYMBOLS, and
/// `#[rtse::class]` knows two kinds of constant — a number and a string — so
/// these two lines are written out, beside the code that makes the symbols
/// (`pickle::namespace`), and pinned by a test that the module answers them.
pub const MODULES: &[(&str, Class, &[&str])] = &[(
    "rts:serde",
    super::pickle::SERDE_TYPES,
    &[
        "/** `static [version] = n` on a class: the schema version its instances are written under. */\n  export const version: unique symbol;",
        "/** `static [upgrade](fields, fromVersion)` on a class: migrates older fields before an instance revives. */\n  export const upgrade: unique symbol;",
    ],
)];

/// One module, as an ambient `declare module` whose members are exports.
fn module(specifier: &str, class: &Class, values: &[&str]) -> String {
    let mut out = jsdoc(class.doc, "");
    out.push_str(&format!("declare module \"{specifier}\" {{\n"));
    for member in class.members {
        out.push_str(&jsdoc(member.doc, "  "));
        out.push_str(&format!("  export {}\n", as_namespace_member(member)));
    }
    for value in values {
        out.push_str(&format!("  {value}\n"));
    }
    out.push_str("}\n\n");
    out
}

/// One class, as the two interfaces and the variable that put it on the global
/// scope.
///
/// `interface X` is what an INSTANCE has, `interface XConstructor` is what the
/// name itself has, and `declare var X: XConstructor` is the binding. That is
/// the shape the standard library uses for every one of these, which is exactly
/// why it merges: same names, same roles.
fn declaration(class: &Class) -> String {
    // A name with a dot in it is a PATH, not a declarable name:
    // `Object.prototype` says where its members are installed, and no
    // declaration form takes it. Named in a comment so it is visibly absent
    // rather than silently.
    if class.name.contains('.') {
        return format!(
            "// `{}` is installed on an object the standard library already \
             declares; its members\n// are absent here rather than merged into \
             it.\n\n",
            class.name
        );
    }
    if GENERIC_IN_LIB.contains(&class.name) {
        return format!(
            "// `{}` is declared with type parameters by the standard library, \
             which already types it\n// precisely. Merging without repeating them \
             is TS2428, so it is left alone.\n\n",
            class.name
        );
    }

    let name = class.name;
    let mut out = String::new();
    let instance: Vec<&Member> = class
        .members
        .iter()
        .filter(|member| matches!(member.role, Role::Prototype | Role::Constant))
        .collect();
    let statics: Vec<&Member> = class
        .members
        .iter()
        .filter(|member| matches!(member.role, Role::Static | Role::StaticConstant))
        .collect();
    let construct = class
        .members
        .iter()
        .find(|member| matches!(member.role, Role::Construct));

    // A namespace has no instances and nothing to construct, so its members sit
    // on the interface the global variable is typed with — which is the shape
    // the standard library gives `Math` and `JSON` too.
    if class.namespace {
        out.push_str(&jsdoc(class.doc, ""));
        if NAMESPACE_IN_LIB.contains(&name) {
            out.push_str(&format!("declare namespace {name} {{\n"));
            for member in class.members {
                out.push_str(&jsdoc(member.doc, "  "));
                out.push_str(&format!("  {}\n", as_namespace_member(member)));
            }
            out.push_str("}\n\n");
            return out;
        }
        out.push_str(&format!("interface {name} {{\n"));
        for member in class.members {
            out.push_str(&jsdoc(member.doc, "  "));
            out.push_str(&format!("  {}\n", spelled(class, member)));
        }
        out.push_str("}\n");
        out.push_str(&format!("declare var {name}: {name};\n\n"));
        return out;
    }

    out.push_str(&jsdoc(class.doc, ""));
    match inherits(class) {
        Some(parent) => out.push_str(&format!("interface {name} extends {parent} {{\n")),
        None => out.push_str(&format!("interface {name} {{\n")),
    }
    for member in &instance {
        out.push_str(&jsdoc(member.doc, "  "));
        out.push_str(&format!("  {}\n", spelled(class, member)));
    }
    out.push_str("}\n");

    out.push_str(&format!("interface {name}Constructor {{\n"));
    // The derived signature is `convert(value: any): any` — the Rust name, which
    // a constructor does not have. Only its parameters survive, and what it
    // answers is an instance by definition.
    if let Some(member) = construct {
        out.push_str(&jsdoc(member.doc, "  "));
        out.push_str(&format!(
            "  new ({}): {name};\n",
            parameters(member.signature)
        ));
    }
    for member in &statics {
        out.push_str(&jsdoc(member.doc, "  "));
        out.push_str(&format!("  {}\n", spelled(class, member)));
    }
    out.push_str("}\n");
    out.push_str(&format!("declare var {name}: {name}Constructor;\n\n"));
    out
}

/// The parent, but only when this engine declares it — see [`Class::extends`].
fn inherits(class: &Class) -> Option<&'static str> {
    let parent = class.extends?;
    CLASSES
        .iter()
        .any(|other| other.name == parent && !GENERIC_IN_LIB.contains(&other.name))
        .then_some(parent)
}

/// One member inside an interface body: a signature and a semicolon.
///
/// No `static` and no `export` keyword anywhere. Which interface a member is IN
/// is what says whether it belongs to an instance or to the name — that is the
/// whole reason there are two — so a modifier saying it again would be a second
/// statement of the same fact, and the one that can disagree.
///
/// A constant is `readonly`, and that is not decoration: the standard library
/// declares `Math.PI` and `Number.MAX_SAFE_INTEGER` readonly, and a merge that
/// omits it is `TS2687: all declarations of 'PI' must have identical modifiers`
/// — seventeen of them, measured. It is also true, which is why the fix is this
/// and not a skip: nothing in the engine writes them.
fn spelled(class: &Class, member: &Member) -> String {
    let constant = matches!(member.role, Role::Constant | Role::StaticConstant);
    // A constant on the CONSTRUCTOR or on a namespace is readonly; one on the
    // PROTOTYPE is not. `Math.PI` and `Number.MAX_SAFE_INTEGER` cannot be
    // assigned and the standard library says so, but `Error.prototype.message`
    // can be and it says that too — so a blanket `readonly` is TS2687 on the
    // second pair exactly as omitting it was on the first. Where they belong is
    // what separates them, and this layer already knows that.
    match constant && (class.namespace || member.role == Role::StaticConstant) {
        true => format!("readonly {};", member.signature),
        false => format!("{};", member.signature),
    }
}

/// The same member inside a `declare namespace` body, where the keyword is part
/// of the declaration rather than implied by the surrounding interface.
fn as_namespace_member(member: &Member) -> String {
    match member.role {
        Role::Constant | Role::StaticConstant => format!("const {};", member.signature),
        _ => format!("function {};", member.signature),
    }
}
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
#[path = "declared_tests.rs"]
mod tests;
