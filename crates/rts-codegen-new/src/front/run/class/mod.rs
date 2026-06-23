//! User CLASSES — the compile-time descriptor + collection (P4.9 single classes,
//! P5.1 inheritance / accessors / statics).
//!
//! A class instance in this engine IS an object in the P3.6 representation: an
//! `Entry::Vec` whose slot 0 is a GLOBAL shape-id (interned from the class's
//! ordered FLATTENED field list — parent fields first, then own fields) and whose
//! slots `1 + slot_index` hold the field values. Methods are ordinary functions
//! whose FIRST parameter is the receiver `this`. `new C(args)` runs the
//! constructor (also a `this`-first function that allocates the instance,
//! zero-inits its fields to `undefined`, runs the user constructor body — which
//! for a subclass begins with `super(args)` calling the parent constructor — and
//! returns the instance). Static method dispatch only: `instance.method(args)` is
//! lowered to a direct call when the receiver's class is statically known; a
//! receiver of unknown class BAILS.
//!
//! ## Inheritance model (P5.1)
//! - **Field flattening.** `class B extends A` has the instance shape = A's
//!   flattened fields THEN B's own fields (parent-first). Slot indices are
//!   assigned parent-first. Instances stay FLAT — no runtime prototype walk for
//!   fields.
//! - **super(args).** A subclass constructor calls `super(args)` which lowers to
//!   a direct call of the parent's synthesized constructor with `this` = the
//!   instance being built. A subclass that omits a constructor gets a synthesized
//!   one that forwards `super(...sameArgs)`.
//! - **Method resolution.** `instance.method()` resolves on the most-derived
//!   class that declares it, walking the chain at COMPILE TIME (the receiver's
//!   static class is known). `super.method(args)` calls the parent's method
//!   explicitly with `this` = the instance.
//! - **Accessors.** `get x()` / `set x()` are synthesized like methods; a READ of
//!   an accessor property calls the getter, a WRITE calls the setter. A class has
//!   either a field `x` or an accessor `x`, never both (the ambiguous case bails).
//! - **Statics.** `static m()` is a synthesized fn with NO `this`; `C.m(args)`
//!   calls it directly. `static f = init` is read via a synthesized zero-arg
//!   getter (`C.f` returns the initializer); a static-field WRITE bails.
//!
//! This module owns [`ClassDesc`] / [`ClassTable`] / [`collect_classes`]. The
//! synthesized functions are appended to the program's `funcs` list (so they get
//! signatures, thunks, and definitions through the EXISTING machinery). Anything
//! still out of subset (abstract, private `#fields`, computed/Symbol names, an
//! `extends` of an unknown parent, mixing field+accessor) makes the whole class
//! [`Unsupported`] — the program never runs with a partially-modeled class.

use std::collections::HashMap;

use rts_ast::ast::{ClassDecl, ClassMember, MethodRole};

use rts_hir::ir::{HirExpr, HirExprKind, HirFunc, HirParam, HirType};

use crate::front::error::{FrontResult, Unsupported};

mod dispatch;
mod inherit;
mod litshape;
mod synth;
mod vdispatch;
mod walk;

pub(crate) use litshape::{LitMethod, build_literal_class, ident_param_name};
pub(crate) use walk::{body_uses_raw_this, rewrite_this_block};

/// The implicit receiver parameter name bound to a0 in every constructor/method.
pub(crate) const THIS: &str = "this";

/// One getter/setter pair on a class (either side may be absent).
#[derive(Clone, Default)]
pub(crate) struct Accessor {
    /// The synthesized zero-arg getter fn (`__rtsn_get_C_x`), if a `get x()` exists.
    pub getter: Option<String>,
    /// The synthesized one-arg setter fn (`__rtsn_set_C_x`), if a `set x()` exists.
    pub setter: Option<String>,
}

/// The compile-time descriptor of one user class.
#[derive(Clone)]
pub(crate) struct ClassDesc {
    /// The class name (`C` in `class C { … }`).
    pub name: String,
    /// The direct superclass name (`A` in `class B extends A`), if any. Resolved
    /// to a known class in the same program at collection time, else the class
    /// bails.
    pub parent: Option<String>,
    /// The FLATTENED ordered instance FIELD names: the parent's flattened fields
    /// first, then this class's own declared/ctor-assigned fields, in first-seen
    /// order. Slot `i` of the instance Vec (after the slot-0 header) holds
    /// `fields[i]`.
    pub fields: Vec<String>,
    /// The GLOBAL shape-id interned from `fields` (slot 0 of every instance; the
    /// inspect trampoline reads it to recover the keys).
    pub global_shape: u32,
    /// The synthesized constructor function name (`__rtsn_ctor_C`). Always present.
    pub ctor: String,
    /// Constructor user-parameter count (excluding the implicit `this`).
    pub ctor_arity: usize,
    /// FLATTENED method name → synthesized function name (own ∪ inherited; own
    /// shadows the parent). The function takes `this` first then its own params.
    pub methods: HashMap<String, String>,
    /// FLATTENED accessor name → getter/setter pair (own ∪ inherited).
    pub accessors: HashMap<String, Accessor>,
    /// Static method name → synthesized fn name (`__rtsn_static_C_m`; no `this`).
    pub statics: HashMap<String, String>,
    /// Static field name → synthesized zero-arg getter fn (`__rtsn_sfield_C_f`).
    pub static_fields: HashMap<String, String>,
    /// FLATTENED names of fields PROVEN to hold an Array (declaration initializer
    /// or a constructor assignment is an array literal `[...]`). Absent ⇒
    /// scalar/opaque. Drives chained access on `this.<field>` (`this.field[i]`,
    /// `this.field.length`, `this.field.push(x)`).
    pub field_arrays: std::collections::HashSet<String>,
    /// FLATTENED names of fields PROVEN to hold a native STRING — the field's
    /// declared type annotation is `string` (the canonical `#value: string`).
    /// Absent ⇒ scalar/opaque. Drives native string ops on `this.<field>`
    /// (`this.value.length`, `this.value[i]`, `this.value.charCodeAt(i)`) — a
    /// field of this kind reads as a `JsKind::Str` value so it reaches the same
    /// native string member/index/method paths a string literal or param does.
    pub field_strings: std::collections::HashSet<String>,
}

impl ClassDesc {
    /// The synthesized function name for `method` on this class (or an inherited
    /// one), if it exists in the flattened method map.
    pub fn method_fn(&self, method: &str) -> Option<&str> {
        self.methods.get(method).map(String::as_str)
    }

    /// The accessor pair for property `name`, if this class (or an ancestor)
    /// declares a getter/setter for it.
    pub fn accessor(&self, name: &str) -> Option<&Accessor> {
        self.accessors.get(name)
    }

    /// Whether `field` is statically proven to hold an Array.
    pub fn field_is_array(&self, field: &str) -> bool {
        self.field_arrays.contains(field)
    }

    /// Whether `field` is statically proven to hold a native `string`.
    pub fn field_is_string(&self, field: &str) -> bool {
        self.field_strings.contains(field)
    }
}

/// name → descriptor for every supported class in the program.
#[derive(Default, Clone)]
pub(crate) struct ClassTable {
    by_name: HashMap<String, ClassDesc>,
}

impl ClassTable {
    /// The descriptor for class `name`, if collected.
    pub fn get(&self, name: &str) -> Option<&ClassDesc> {
        self.by_name.get(name)
    }

    /// Whether class `name` is already collected (used by the literal-class
    /// recovery pass to share one descriptor across identical literals).
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Register a descriptor (the literal-class recovery pass adds synthesized
    /// object-literal classes here after `collect_classes`).
    pub fn insert(&mut self, desc: ClassDesc) {
        self.by_name.insert(desc.name.clone(), desc);
    }

    /// Every collected class descriptor (user-declared AND synthesized virtual
    /// builtin parents). Used to bind `this` for every synthesized ctor/method.
    pub fn iter(&self) -> impl Iterator<Item = &ClassDesc> {
        self.by_name.values()
    }
}

/// Collect every `class` declaration into a [`ClassTable`] plus the synthesized
/// constructor/method/accessor/static `HirFunc`s (to append to the program's
/// `funcs`).
///
/// Classes are processed PARENT-FIRST (so a subclass flattens its parent's fields
/// and methods). Each class either fully models or makes the whole program
/// `Unsupported`. The out-of-subset bails (abstract, private, computed names,
/// unknown parent, field/accessor clash) are detected here, before any lowering.
///
/// `ambient` carries the AMBIENT (engine-prelude) classes — `Error` (+ its
/// subclasses) and `Map`/`Set` — already built from the embedded `.ts` includes.
/// A user `class X extends Error` resolves its parent against `ambient` (the
/// `Error` descriptor is the real prelude class, not a synthesized stub). This is
/// how the PRIMORDIAL `Error` family (now a `.ts` class) participates in user
/// inheritance: the engine no longer synthesizes a virtual Error parent in codegen.
pub(crate) fn collect_classes(
    classes: &[&ClassDecl],
    ambient: &ClassTable,
) -> FrontResult<(ClassTable, Vec<HirFunc>)> {
    let mut table = ClassTable::default();
    let mut funcs: Vec<HirFunc> = Vec::new();

    // Resolve a parent-first processing order (so a child sees its parent's
    // already-built descriptor). An `extends` of a class not in this program (and
    // not an ambient prelude class), or a cycle, bails.
    let order = inherit::topo_order(classes)?;
    for decl in order {
        check_supported(decl)?;
        let parent_desc = match &decl.super_class {
            // Resolve the parent: a class earlier in THIS program first, else an
            // AMBIENT prelude class (`Error`/`Map`/`Set`). An `extends` of anything
            // else (an unmodeled builtin like `Array`) bails with a precise message.
            Some(p) => Some(
                table
                    .by_name
                    .get(p)
                    .or_else(|| ambient.get(p))
                    .cloned()
                    .ok_or_else(|| {
                        Unsupported::new(format!(
                            "class `{}` extends unknown class `{p}` (not a user class \
                             in this program nor an engine-prelude class)",
                            decl.name
                        ))
                    })?,
            ),
            None => None,
        };
        let (desc, fns) = synth::build_class(decl, parent_desc.as_ref())?;
        table.by_name.insert(desc.name.clone(), desc);
        funcs.extend(fns);
    }
    Ok((table, funcs))
}

/// Refuse a class that uses a feature outside the implemented subset. Returns
/// `Ok(())` for a supported shape, else an explicit `Unsupported`. (Inheritance,
/// accessors, and statics are now SUPPORTED; only the genuinely-unmodeled
/// features remain refused here.)
fn check_supported(decl: &ClassDecl) -> FrontResult<()> {
    if decl.is_abstract {
        return Err(Unsupported::new(format!("abstract class `{}`", decl.name)));
    }
    if !decl.static_init_blocks.is_empty() {
        return Err(Unsupported::new(format!(
            "class `{}` has a `static {{}}` init block",
            decl.name
        )));
    }
    for m in &decl.members {
        match m {
            ClassMember::Constructor(_) => {}
            ClassMember::Method(md) => {
                if md.modifiers.is_abstract {
                    return Err(Unsupported::new(format!(
                        "abstract method `{}.{}`",
                        decl.name, md.name
                    )));
                }
                if md.name.starts_with('#') {
                    return Err(Unsupported::new(format!(
                        "private method `{}.{}`",
                        decl.name, md.name
                    )));
                }
                if !matches!(
                    md.role,
                    MethodRole::Method | MethodRole::Getter | MethodRole::Setter
                ) {
                    return Err(Unsupported::new(format!(
                        "unsupported method role `{}.{}`",
                        decl.name, md.name
                    )));
                }
            }
            // (F2) Private INSTANCE fields (`#name`) are ordinary field slots.
            // The decl name (`"#name"`) and a `this.#name` access prop (also
            // `"#name"`, per rts-hir `lower.rs` MemberProp::PrivateName) are the
            // SAME interned string, so the shape slot lookup matches with no
            // normalization. Reads/writes/chained access go through the same
            // slot machinery as public fields. Private METHODS stay unsupported
            // (rejected in the `ClassMember::Method` arm above).
            ClassMember::Property(_) => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared HIR helpers used by the synthesizer (kept here so both `mod` and
// `synth` stay under the 500-line rule).
// ---------------------------------------------------------------------------

/// Build a `this`-typed implicit first parameter (Tagged at the ABI: an opaque
/// instance word). The HIR type is `Unknown` so the sig lattice picks `Tagged`.
pub(crate) fn this_param() -> HirParam {
    HirParam {
        name: THIS.to_string(),
        ty: HirType::Unknown,
        variadic: false,
        has_default: false,
        optional: false,
        default_expr: None,
        class_hint: None,
    }
}

/// Whether a lowered HIR expr is a `this`-reference: the swc `ThisExpr` lowers to
/// a `Raw("This(...)")` node (rts-hir has no dedicated `this` arm). We rewrite
/// those to `Ident("this")` after lowering (see [`synth`]).
pub(crate) fn is_raw_this(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Raw(s) if s.starts_with("This("))
}
