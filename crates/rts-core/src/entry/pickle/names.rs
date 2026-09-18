//! Which classes and functions a stream may name, and what each name is here.
//!
//! # Where the registry lives, and why there
//!
//! On the heap, as two ordinary things a program cannot reach:
//!
//! - every declared class and top-level function carries its own name as a
//!   hidden property, [`MARKER`] — `"module\0name"` — which is how the WRITER
//!   asks "what is this called" in one property read;
//! - the global object carries [`REGISTRY`], an object keyed by the plain name
//!   whose value is the list of every declaration of that name, which is how
//!   the READER asks "what does this name mean here".
//!
//! Both keys are in the `@@` space, which enumeration, `JSON.stringify` and
//! `Object.getOwnPropertySymbols` all skip — the same place `@@collectionCursor`
//! and `@@finalizationCleanup` keep internal state for the same reason.
//!
//! The alternatives were a field on `Context` and a side table beside it. A
//! field was measured this week to cost +8% to +29% per call even when nothing
//! reads it — it moves the fields every native call does read — and a side
//! table holding values is a new entry on a hand-written root list, which is
//! the class `docs/engine/lost-roots.md` exists to warn about. Here there is no
//! list to be missing from: the global object is a root, the registry is a
//! property of it, and each declaration is an element of an array in it. The
//! collector reaches all of it by the ordinary edges it already follows.
//!
//! # Bounded, and by what
//!
//! One entry per (module, name) the program DECLARES. A class written inside a
//! loop replaces its own entry each pass rather than adding one — the bound
//! `lost-roots.md` asks every root source to state, stated about the right
//! thing: declarations in the source text, not evaluations.

use super::super::clone::ClassName;
use super::super::rooted::Rooted;
use super::super::{Context, with_current};
use crate::object::Key;
use crate::text::Str;
use crate::value::Value;

/// The property on the global object holding the registry.
const REGISTRY: &str = "@@serdeNames";

/// The property on a declared class or function holding its own name.
const MARKER: &str = "@@serdeName";

/// Records a declaration the compiler saw: a named class, or a named function
/// at the top level of a module — under the module's key and the name it was
/// declared with, both literals of the program.
///
/// # Why the compiler says so, rather than the runtime noticing
///
/// Because only the compiler knows either half. A class is a constructor
/// function here — `emit/class.rs` says why there is no class in the runtime —
/// and nothing about a function at run time says which file declared it, or
/// that it was declared at the top level rather than inside another function.
/// The module key is the host's (`emit::Ctx::module_key`), so it survives the
/// program being compiled on another machine or ahead of time.
///
/// An entry point rather than a property write the compiler emits, because the
/// registry is a structure — a list per name, replaced by module — and an
/// emitted sequence of reads and writes over it would be the same rule stated
/// in IR at every declaration.
///
/// `space` is the number the class's own `#private` names carry, `-1` for none
/// — see [`portable`] for what it is for.
#[rtse::entry]
pub fn serde_declare(target: u64, module: i64, name: i64, space: i64) -> u64 {
    with_current(|context| declare(context, target, module, name, space));
    target
}

fn declare(context: &mut Context, target: u64, module: i64, name: i64, space: i64) {
    let Some(cell) = Value(target).as_slot() else {
        return;
    };
    let (Some(module), Some(name)) = (
        super::super::modules::literal_text(context, module),
        super::super::modules::literal_text(context, name),
    ) else {
        return;
    };
    // Every value below is held here until it is reachable from the global
    // object: each step allocates — the marker's text, the registry, a list,
    // a spill for a new property — and a value named only by a Rust local is
    // what `docs/engine/lost-roots.md` records the collector freeing.
    let mut held = Rooted::with(vec![target]);
    let spelled = match space {
        0.. => format!("{module}\0{name}\0{space}"),
        _ => format!("{module}\0{name}"),
    };
    let spelled = context.intern_value(Str::from_str(&spelled)).bits();
    held.values().push(spelled);
    let marker = context.well_known(MARKER);
    super::super::objects::put(context, cell, marker, spelled);
    super::super::native::hidden(context, cell, marker);
    let Some(registry) = registry(context, true) else {
        return;
    };
    held.values().push(Value::from_slot(registry).bits());
    let key = Key::Name(context.interner.intern_str(&name, &mut context.keys));
    let list = super::super::objects::own_property(context, registry, key).and_then(|list| list.as_slot());
    let Some(list) = list else {
        let made = super::super::array::built_in(context, vec![target]);
        held.values().push(made);
        super::super::objects::put(context, registry, key, made);
        return;
    };
    // The same (module, name) declared again — a class written inside a loop,
    // a module evaluated twice — REPLACES its entry, which is what bounds the
    // registry by the declarations in the source rather than by evaluations.
    let candidates = context.elements_at(list).cloned().unwrap_or_default();
    for (at, candidate) in candidates.iter().enumerate() {
        let same = Value(*candidate)
            .as_slot()
            .and_then(|candidate| declared_as(context, candidate))
            .is_some_and(|declared| declared.module.to_rust_lossy() == module);
        if same {
            if let Some(elements) = context.elements_at_mut(list) {
                elements[at] = target;
            }
            return;
        }
    }
    let count = candidates.len() + 1;
    if let Some(elements) = context.elements_at_mut(list) {
        elements.push(target);
    }
    super::super::array::set_length(context, list, count);
}

/// What a class or function was declared as, if the program declared it where
/// the pickle can name it.
pub(in crate::entry) fn declared_as(context: &mut Context, cell: u32) -> Option<ClassName> {
    let key = context.well_known(MARKER);
    let spelled = super::super::objects::own_property(context, cell, key)?;
    let text = context.text_at(spelled.as_slot()?)?.to_rust()?;
    let mut parts = text.split('\0');
    let (module, name) = (parts.next()?, parts.next()?);
    Some(ClassName {
        module: std::rc::Rc::new(Str::from_str(module)),
        name: std::rc::Rc::new(Str::from_str(name)),
        prototype: 0,
        version: version_of(context, cell),
    })
}

/// The number a declared class's own `#private` names carry, if it has any.
fn space_of(context: &mut Context, cell: u32) -> Option<u32> {
    let key = context.well_known(MARKER);
    let spelled = super::super::objects::own_property(context, cell, key)?;
    let text = context.text_at(spelled.as_slot()?)?.to_rust()?;
    text.split('\0').nth(2)?.parse().ok()
}

/// The private-name number of each class on a prototype chain, nearest first:
/// the class whose prototype this is, then its parent, up to `Object`.
pub(in crate::entry) fn spaces(context: &mut Context, prototype: u32) -> Vec<Option<u32>> {
    let mut found = Vec::new();
    let mut at = Some(prototype);
    let root = super::super::object_proto::prototype_of(context);
    for _ in 0..super::super::objects::CHAIN_LIMIT {
        let Some(cell) = at.filter(|cell| Some(*cell) != root) else {
            break;
        };
        let key = context.well_known("constructor");
        let constructor = super::super::objects::own_property(context, cell, key).and_then(|found| found.as_slot());
        found.push(constructor.and_then(|constructor| space_of(context, constructor)));
        at = super::super::objects::inherited_from(context, cell);
    }
    found
}

/// The spelling a private field is WRITTEN under: `@@#^<depth>#name`, its
/// class's distance from the instance's own class, rather than the
/// `@@#<n>#name` it has in memory.
///
/// # Why the key is rewritten at all
///
/// `<n>` is the order in which the parser met the class — `Cx::private_name`
/// numbers them so that `#x` in a class and `#x` in its subclass are two
/// fields. It is right inside one compilation and meaningless outside it: add
/// a class above this one in the file, or a module before it, and the same
/// field is `@@#3#x` where the stream says `@@#2#x`, and the revived instance
/// has an `#x` its methods never read. The depth in the class chain is what
/// stays true across two versions of a program, which is what a save file
/// is read by. A private name whose class is not in the chain the registry
/// knows keeps its memory spelling, and revives only in the same program.
///
/// `spaces` is [`spaces`] of the instance's prototype, which the caller
/// computes once per class rather than once per instance.
pub(in crate::entry) fn portable(context: &mut Context, spaces: &[Option<u32>], fields: Vec<(Key, u64)>) -> Vec<(Key, u64)> {
    fields
        .into_iter()
        .map(|(key, held)| {
            let Some((space, name)) = private_parts(context, key) else {
                return (key, held);
            };
            match space.parse::<u32>().ok().and_then(|space| spaces.iter().position(|found| *found == Some(space))) {
                Some(depth) => (named(context, &format!("@@#^{depth}#{name}")), held),
                None => (key, held),
            }
        })
        .collect()
}

/// The inverse: a stream's portable private key as THIS program's, for an
/// instance of the class whose prototype this is. A v1 stream spelled a
/// private field `#name`, with no class to tell two apart; it is read as the
/// instance's own class's, which is what v1 meant when it wrote one.
pub(super) fn local(context: &mut Context, spaces: &[Option<u32>], keys: &mut [Key], legacy: bool) {
    for key in keys.iter_mut() {
        let Key::Name(named_key) = *key else {
            continue;
        };
        let Some(text) = context.interner.text(named_key).and_then(|text| text.to_rust()) else {
            continue;
        };
        let (depth, name) = match (text.strip_prefix("@@#^"), legacy) {
            (Some(rest), _) => match rest.split_once('#') {
                Some((depth, name)) => (depth.parse::<usize>().ok(), name.to_owned()),
                None => continue,
            },
            (None, true) if text.starts_with('#') => (Some(0), text[1..].to_owned()),
            _ => continue,
        };
        if let Some(Some(space)) = depth.and_then(|depth| spaces.get(depth)) {
            *key = named(context, &format!("@@#{space}#{name}"));
        }
    }
}

/// A private key's two parts in memory — its class number and its name — or
/// `None` for a key that is not one.
pub(super) fn private_parts(context: &Context, key: Key) -> Option<(String, String)> {
    let Key::Name(named_key) = key else {
        return None;
    };
    let text = context.interner.text(named_key)?.to_rust()?;
    let rest = text.strip_prefix("@@#")?;
    let (space, name) = rest.split_once('#')?;
    space.chars().all(|c| c.is_ascii_digit()).then(|| (space.to_owned(), name.to_owned()))
}

fn named(context: &mut Context, text: &str) -> Key {
    Key::Name(context.interner.intern_str(text, &mut context.keys))
}

/// The private-name number of the class whose prototype this is, if it
/// declares any — the one an `upgrade` sees its fields under as `"#name"`.
pub(super) fn own_space(context: &mut Context, prototype: u64) -> Option<u32> {
    let prototype = Value(prototype).as_slot()?;
    spaces(context, prototype).first().copied().flatten()
}

/// The key of the symbol `rts:serde` exports as `version`.
///
/// A symbol's property key IS its key text interned, so the key is reachable
/// without the symbol value — which is what lets the writer read a class's
/// version in the same borrow it reads the class's name.
pub(super) const VERSION: &str = "@@serde.version";

/// The key of the symbol `rts:serde` exports as `upgrade`.
pub(super) const UPGRADE: &str = "@@serde.upgrade";

/// The schema version a class declares as `static [version] = n`, or `0` for
/// one that declares none — a non-negative whole number, anything else being
/// `0` as well rather than a number the stream would carry and nothing could
/// compare.
pub(super) fn version_of(context: &mut Context, cell: u32) -> u64 {
    let key = context.well_known(VERSION);
    super::super::objects::own_property(context, cell, key)
        .and_then(|found| found.numeric())
        .filter(|number| number.fract() == 0.0 && *number >= 0.0 && *number <= 9_007_199_254_740_991.0)
        .map_or(0, |number| number as u64)
}

/// The callable a stream's name means in this program.
///
/// The qualified name first. When nothing matches it — a different program,
/// a file that moved — the plain name, but ONLY when exactly one declaration
/// in this program answers to it: two candidates is a `TypeError` naming both,
/// never a choice made silently, which is what v1's flat namespace did (the
/// last registration won). `module: None` is a v1 stream, which never had a
/// qualified name to try.
pub(super) fn resolve(context: &mut Context, module: Option<&Str>, name: &Str) -> Result<u64, String> {
    let spelled = name.to_rust_lossy();
    let candidates = candidates(context, name);
    let mut named: Vec<(String, u64)> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let Some(cell) = Value(candidate).as_slot() else {
            continue;
        };
        let Some(declared) = declared_as(context, cell) else {
            continue;
        };
        if module.is_some_and(|module| module.same_units(&declared.module)) {
            return Ok(candidate);
        }
        named.push((declared.module.to_rust_lossy(), candidate));
    }
    match named.as_slice() {
        [] => Err(format!("pickle: '{spelled}' is not declared in this program")),
        [(_, only)] => Ok(*only),
        many => {
            let listed: Vec<String> = many
                .iter()
                .map(|(module, _)| format!("'{spelled}' in {}", shown(module)))
                .collect();
            let why = match module {
                Some(_) => "the stream's module matches none of",
                None => "a v1 stream names no module, and it could mean any of",
            };
            Err(format!(
                "pickle: '{spelled}' is ambiguous here — {why} the {} declarations of that \
                 name ({}), and picking one would be a guess",
                many.len(),
                listed.join(", ")
            ))
        }
    }
}

/// How a module key reads in a message: the entry file has none.
fn shown(module: &str) -> String {
    match module.is_empty() {
        true => "the entry module".to_owned(),
        false => format!("module '{module}'"),
    }
}

/// Every declaration of a plain name, newest last.
fn candidates(context: &mut Context, name: &Str) -> Vec<u64> {
    let Some(registry) = registry(context, false) else {
        return Vec::new();
    };
    let key = Key::Name(context.interner.intern(name, &mut context.keys));
    super::super::objects::own_property(context, registry, key)
        .and_then(|list| list.as_slot())
        .and_then(|list| context.elements_at(list).cloned())
        .unwrap_or_default()
}

/// The registry object, made on first use when `make` asks for it.
fn registry(context: &mut Context, make: bool) -> Option<u32> {
    let holder = super::super::global::holder(context)?;
    let key = context.well_known(REGISTRY);
    if let Some(found) = super::super::objects::own_property(context, holder, key).and_then(|found| found.as_slot()) {
        return Some(found);
    }
    if !make {
        return None;
    }
    let made = super::super::native::plain(context)?;
    super::super::objects::put(context, holder, key, Value::from_slot(made).bits());
    super::super::native::hidden(context, holder, key);
    Some(made)
}
