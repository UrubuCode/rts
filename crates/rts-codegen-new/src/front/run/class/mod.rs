//! User CLASSES without inheritance — the compile-time descriptor + collection
//! (P4.9).
//!
//! A class instance in this engine IS an object in the P3.6 representation: an
//! `Entry::Vec` whose slot 0 is a GLOBAL shape-id (interned from the class's
//! ordered FIELD names) and whose slots `1 + slot_index` hold the field values.
//! Methods are ordinary functions whose FIRST parameter is the receiver `this`.
//! `new C(args)` runs the constructor (also a `this`-first function that
//! allocates the instance, zero-inits its fields to `undefined`, runs the user
//! constructor body, and returns the instance). Static method dispatch only:
//! `instance.method(args)` is lowered to a direct call when the receiver's class
//! is statically known (a `new C()` result, a `: C`-annotated local, or `this`
//! inside a method); a receiver of unknown class BAILS.
//!
//! This module owns:
//! - [`ClassDesc`] — the compile-time descriptor (field order, global shape-id,
//!   method name set, constructor presence + arity);
//! - [`ClassTable`] — name → [`ClassDesc`], built from the AST `ClassDecl`s;
//! - [`collect_classes`] — lower each supported `ClassDecl` into its descriptor
//!   PLUS the synthesized top-level `HirFunc`s (constructor + methods, each with
//!   `this` as the implicit first param). Anything out of the no-inheritance
//!   subset (extends/super/static/getter/setter/private/computed) makes the whole
//!   class [`Unsupported`] — the program never runs with a partially-modeled class.
//!
//! The synthesized functions are appended to the program's `funcs` list, so they
//! get signatures, thunks, and definitions through the EXISTING function
//! machinery (no parallel codegen). The naming convention is private to the
//! engine and cannot collide with a user identifier (the `__rtsn_` prefix).

use std::collections::HashMap;

use rts_ast::ast::{ClassDecl, ClassMember, MethodRole};
use rts_hir::ir::{HirExpr, HirExprKind, HirFunc, HirParam, HirType};

use crate::front::error::{FrontResult, Unsupported};

mod dispatch;
mod synth;

/// The implicit receiver parameter name bound to a0 in every constructor/method.
pub(crate) const THIS: &str = "this";

/// The compile-time descriptor of one user class (no inheritance).
#[derive(Clone)]
pub(crate) struct ClassDesc {
    /// The class name (`C` in `class C { … }`).
    pub name: String,
    /// The ordered instance FIELD names (declared properties ∪ first-assigned
    /// `this.x` in the constructor, in first-seen order). Slot `i` of the
    /// instance Vec (after the slot-0 header) holds `fields[i]`.
    pub fields: Vec<String>,
    /// The GLOBAL shape-id interned from `fields` (slot 0 of every instance; the
    /// inspect trampoline reads it to recover the keys).
    pub global_shape: u32,
    /// The synthesized constructor function name (`__rtsn_ctor_C`). Always present
    /// (a default no-op constructor is synthesized when the class declares none).
    pub ctor: String,
    /// Constructor user-parameter count (excluding the implicit `this`).
    pub ctor_arity: usize,
    /// method name → synthesized function name (`__rtsn_method_C_m`). The function
    /// takes `this` as its first param followed by the method's own params.
    pub methods: HashMap<String, String>,
}

impl ClassDesc {
    /// The synthesized function name for `method` on this class, if it exists.
    pub fn method_fn(&self, method: &str) -> Option<&str> {
        self.methods.get(method).map(String::as_str)
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
}

/// Collect every `class` declaration in `classes` into a [`ClassTable`] plus the
/// synthesized constructor/method `HirFunc`s (to append to the program's `funcs`).
///
/// Each class either fully models or makes the whole program `Unsupported` — a
/// class outside the no-inheritance subset is refused up front (so no `new C(..)`
/// / method call on it ever silently mis-runs). The bail cases (extends, super,
/// static members, getters/setters, private `#fields`, computed names) are
/// detected here, BEFORE any lowering.
pub(crate) fn collect_classes(classes: &[&ClassDecl]) -> FrontResult<(ClassTable, Vec<HirFunc>)> {
    let mut table = ClassTable::default();
    let mut funcs: Vec<HirFunc> = Vec::new();

    for decl in classes {
        check_supported(decl)?;
        let (desc, fns) = synth::build_class(decl)?;
        table.by_name.insert(desc.name.clone(), desc);
        funcs.extend(fns);
    }
    Ok((table, funcs))
}

/// Refuse a class that uses any feature outside the single-class, no-inheritance
/// subset. Returns `Ok(())` for a supported shape, else an explicit `Unsupported`.
fn check_supported(decl: &ClassDecl) -> FrontResult<()> {
    if decl.super_class.is_some() {
        return Err(Unsupported::new(format!(
            "class `{}` uses `extends` (inheritance is a later increment)",
            decl.name
        )));
    }
    if decl.is_abstract {
        return Err(Unsupported::new(format!("abstract class `{}`", decl.name)));
    }
    if !decl.static_init_body.is_empty() || !decl.static_init_blocks.is_empty() {
        return Err(Unsupported::new(format!(
            "class `{}` has a `static {{}}` init block",
            decl.name
        )));
    }
    for m in &decl.members {
        match m {
            ClassMember::Constructor(_) => {}
            ClassMember::Method(md) => {
                if md.modifiers.is_static {
                    return Err(Unsupported::new(format!(
                        "static method `{}.{}`",
                        decl.name, md.name
                    )));
                }
                if md.modifiers.is_abstract {
                    return Err(Unsupported::new(format!(
                        "abstract method `{}.{}`",
                        decl.name, md.name
                    )));
                }
                if !matches!(md.role, MethodRole::Method) {
                    return Err(Unsupported::new(format!(
                        "getter/setter `{}.{}`",
                        decl.name, md.name
                    )));
                }
                if md.name.starts_with('#') {
                    return Err(Unsupported::new(format!(
                        "private method `{}.{}`",
                        decl.name, md.name
                    )));
                }
            }
            ClassMember::Property(pd) => {
                if pd.modifiers.is_static {
                    return Err(Unsupported::new(format!(
                        "static field `{}.{}`",
                        decl.name, pd.name
                    )));
                }
                if pd.name.starts_with('#') {
                    return Err(Unsupported::new(format!(
                        "private field `{}.{}`",
                        decl.name, pd.name
                    )));
                }
            }
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
    }
}

/// Whether a lowered HIR expr is a `this`-reference: the swc `ThisExpr` lowers to
/// a `Raw("This(...)")` node (rts-hir has no dedicated `this` arm). We rewrite
/// those to `Ident("this")` after lowering (see [`synth`]).
pub(crate) fn is_raw_this(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Raw(s) if s.starts_with("This("))
}
