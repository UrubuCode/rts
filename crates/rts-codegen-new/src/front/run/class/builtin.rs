//! `class X extends <BuiltinError>` — a synthesized VIRTUAL parent so a user
//! class can extend a built-in error (Error / TypeError / RangeError / …) (P5.3).
//!
//! The PRIMORDIAL-vs-Registry doctrine lets the engine NAME the Error family
//! (they are primordials). A subclass instance in this engine is a normal object
//! Vec (slot-0 shape-id + field slots) — NOT the runtime's `Entry::ErrorObj` — so
//! the cleanest sound model is to give the builtin parent a SYNTHESIZED
//! `ClassDesc` with the error fields (`message`, `name`, `stack`) and a
//! constructor that does exactly what `new Error(msg)` does to a fresh instance:
//!
//! ```text
//! __rtsn_ctor_Error(this, message) {
//!     this.message = message;   // the constructor argument
//!     this.name    = "Error";   // the builtin's default .name
//!     this.stack   = "";        // a basic .stack (header only)
//! }
//! __rtsn_method_Error_toString(this) {
//!     return this.name + ": " + this.message;   // JS Error.prototype.toString
//! }
//! ```
//!
//! With that virtual parent in the [`super::ClassTable`], the EXISTING inheritance
//! machinery handles everything: `super(msg)` lowers to a call of the parent ctor
//! (field flattening puts `message`/`name`/`stack` first), the user class's own
//! `this.name = "MyErr"` reassigns the inherited slot, `.message`/`.name` are
//! ordinary field reads, and `e.toString()` resolves the inherited method. No
//! runtime `ErrorObj` is involved for the subclass case — it stays fully in the
//! engine's object model.
//!
//! Only the Error FAMILY is synthesized here; extending any other builtin
//! (`Array`, `Map`, …) is refused at [`super::collect_classes`] (those need real
//! exotic-object behavior the flat-Vec model cannot fake soundly).

use rts_hir::ir::{HirExpr, HirExprKind, HirFunc, HirLit, HirParam, HirStmt, HirType};

use super::synth::{ctor_name, method_name};
use super::walk::this_field_assign;
use super::{ClassDesc, this_param};
use std::collections::HashMap;

/// The built-in error class names the engine may name + synthesize a virtual
/// parent for (the primordial Error family).
pub(super) const BUILTIN_ERRORS: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "URIError",
    "EvalError",
    "AggregateError",
];

/// Whether `name` is a built-in error the engine can synthesize a virtual parent
/// for (so `class X extends <name>` is supported).
pub(super) fn is_builtin_error(name: &str) -> bool {
    BUILTIN_ERRORS.contains(&name)
}

/// Synthesize the virtual parent [`ClassDesc`] for builtin error `name` plus its
/// constructor + `toString` `HirFunc`s. The instance fields are
/// `["message", "name", "stack"]`; the ctor takes one `message` argument.
///
/// Every error SUBTYPE (`TypeError`/`RangeError`/…) has `parent = Some("Error")`
/// so the JS truth `x instanceof Error` holds for any error subclass (the engine
/// walks the class chain to `Error`). `Error` itself is the root (`parent: None`).
/// The caller ensures the `Error` base is also synthesized when any subtype is.
pub(super) fn synth_builtin_error(name: &str) -> (ClassDesc, Vec<HirFunc>) {
    let fields = vec![
        "message".to_string(),
        "name".to_string(),
        "stack".to_string(),
    ];
    let global_shape = crate::shape::intern_global_shape(&fields);

    let ctor = ctor_name(name);
    let to_string_fn = method_name(name, "toString");

    let mut funcs = Vec::with_capacity(2);
    funcs.push(synth_ctor(name, &ctor));
    funcs.push(synth_to_string(&to_string_fn));

    let mut methods: HashMap<String, String> = HashMap::new();
    methods.insert("toString".to_string(), to_string_fn);

    let parent = if name == "Error" {
        None
    } else {
        Some("Error".to_string())
    };
    // The virtual Error parent's fields (message/name/stack) are all strings.
    let field_strings: std::collections::HashSet<String> = fields.iter().cloned().collect();
    let desc = ClassDesc {
        name: name.to_string(),
        parent,
        fields,
        global_shape,
        ctor,
        ctor_arity: 1,
        methods,
        accessors: HashMap::new(),
        statics: HashMap::new(),
        static_fields: HashMap::new(),
        // The virtual Error parent has no array-typed fields.
        field_arrays: std::collections::HashSet::new(),
        field_strings,
    };
    (desc, funcs)
}

/// `__rtsn_ctor_<Name>(this, message) { this.message = message; this.name =
/// "<Name>"; this.stack = ""; }`.
fn synth_ctor(name: &str, ctor: &str) -> HirFunc {
    let params = vec![
        this_param(),
        HirParam {
            name: "message".to_string(),
            ty: HirType::Unknown,
            variadic: false,
            has_default: false,
            optional: false,
            default_expr: None,
        },
    ];
    let body = vec![
        this_field_assign("message", ident("message")),
        this_field_assign("name", str_lit(name)),
        this_field_assign("stack", str_lit("")),
    ];
    HirFunc {
        name: ctor.to_string(),
        params,
        ret: HirType::Void,
        body,
        is_async: false,
        is_arrow: false,
    }
}

/// `__rtsn_method_<Name>_toString(this) { return this.name + ": " + this.message; }`
/// — JS `Error.prototype.toString` (`"<name>: <message>"`).
fn synth_to_string(fn_name: &str) -> HirFunc {
    let name = this_member("name");
    let colon = str_lit(": ");
    let message = this_member("message");
    // (this.name + ": ") + this.message
    let left = bin_add(name, colon);
    let full = bin_add(left, message);
    HirFunc {
        name: fn_name.to_string(),
        params: vec![this_param()],
        ret: HirType::Unknown,
        body: vec![HirStmt::Return(Some(full))],
        is_async: false,
        is_arrow: false,
    }
}

// ---- tiny HIR builders ----

fn ident(name: &str) -> HirExpr {
    HirExpr::new(HirExprKind::Ident(name.to_string()), HirType::Unknown)
}

fn str_lit(s: &str) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Str(s.to_string())), HirType::Str)
}

fn this_member(field: &str) -> HirExpr {
    HirExpr::new(
        HirExprKind::Member {
            object: Box::new(ident(super::THIS)),
            prop: field.to_string(),
        },
        HirType::Unknown,
    )
}

fn bin_add(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
    HirExpr::new(
        HirExprKind::Bin {
            op: rts_hir::ir::HirBinOp::Add,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        HirType::Unknown,
    )
}
