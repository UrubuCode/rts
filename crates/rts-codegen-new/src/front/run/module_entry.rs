//! `front/run/module_entry` — run a REAL multi-file TS program from disk (M1b).
//!
//! Where [`super::run_source`] runs ONE source string (eval / `-e`, no disk
//! imports), [`run_path`] takes an ENTRY `.ts` file, resolves its whole relative-
//! import graph through the pure M1a resolver
//! ([`crate::front::modules::load_program`]), and runs the flattened program end
//! to end:
//!
//! ```text
//! entry &Path
//!   --modules::load_program--> ResolvedProgram { program, bindings }
//!   --apply_bindings--> flattened Program with imported LOCAL names remapped
//!                       to the exported declaration name (alias rewrite)
//!   --build_from_program--> LoweredProgram (USER side; NOT prelude)
//!   --merge_programs(includes_prelude, user)--> one module
//!   --compile_program--> JIT --> run --> stdout
//! ```
//!
//! ## Binding application
//!
//! - **`Binding::Local { name }`** (a user-module export): the importing module
//!   sees the export under its LOCAL name (`import { add as plus }` → `plus`),
//!   but the flattened program carries the declaration under its REAL name
//!   (`add`). [`apply_bindings`] renames every `plus` IDENTIFIER reference to
//!   `add` via a focused swc `VisitMut` over each statement / function body /
//!   class initializer — so the reference resolves to the flattened declaration.
//!   A plain `import { add }` (local == orig) needs no rename (the name already
//!   matches the declaration); the rewrite is a no-op for it.
//!
//! - **`Binding::Builtin { ns, member }`** (`import … from "rts:<ns>"`): the
//!   binding is recorded in the [`super::LoweredProgram::builtins`] map (local name
//!   → `(ns, member)`). A CALL to that name lowers to the real `__RTS_FN_NS_*`
//!   symbol resolved through the Registry ([`super::registry::namespace_member`])
//!   and marshaled via the generic class-method path (`recv = None`). The `rts:<ns>`
//!   form is fully wired (e.g. `io`, `math`); bare `"rts"` (a namespace OBJECT
//!   import) and any unknown member bail honestly at the call site.

use std::collections::HashMap;
use std::path::Path;

use swc_ecma_visit::VisitMutWith;

use rts_ast::ast::{Item, Statement};

use crate::front::error::FrontResult;
use crate::front::modules::{self, Binding};

use super::{build_from_program, merge_programs, module_jit, registry, render_source_core};

/// Resolve, lower, JIT, and RUN the whole multi-file program reachable from
/// `entry`. `console.log` output goes to the process's real stdout (same as
/// [`super::run_source`]). Relative imports resolve from the entry file's
/// directory.
///
/// Errors: a resolver failure (missing file, cycle, missing export, name
/// collision), a parse error, any construct outside the implemented subset, or a
/// builtin import that does not resolve to a real namespace function (bare `"rts"`
/// namespace object, unknown member, builtin used as a value) — all explicit
/// `Unsupported`.
pub fn run_path(entry: &Path) -> FrontResult<()> {
    let prog = build_path(entry)?;
    let program = module_jit::compile_program(&prog)?;
    program.run_main();
    // An uncaught top-level `throw` records the thrown value in the codegen
    // pending-error slot and unwinds via a sentinel `return` out of `main`, but
    // it does NOT crash the process. Surface it host-side: stringify the thrown
    // value, print `Uncaught <value>` to stderr, and fail (non-zero exit),
    // matching JS/Node. No-op when the slot is clear.
    if crate::value::errslot::__rtsadp_err_pending() != 0 {
        let word = crate::value::errslot::__rtsadp_err_take();
        let s_word = crate::value::genops::__rtsadp_to_string(word);
        let text = crate::value::abi_adapter::resolve_poly(crate::value::PolyValue::from_raw(s_word));
        eprintln!("Uncaught {text}");
        return Err(crate::front::error::Unsupported::new(format!(
            "uncaught exception: {text}"
        )));
    }
    Ok(())
}

/// `rts ir` — build + JIT-compile the whole program reachable from `entry` with
/// the per-function Cranelift IR dump enabled (each successfully-lowered function
/// prints to stderr), WITHOUT running `__rtsn_main`. Errors are the same set as
/// [`run_path`] (resolver/parse/unsupported); on a bail the functions already
/// lowered before the failing one have still been printed — useful to see how far
/// the lowering got.
pub fn dump_ir_path(entry: &Path) -> FrontResult<()> {
    let prog = build_path(entry)?;
    module_jit::set_dump_ir(true);
    let res = module_jit::compile_program(&prog);
    module_jit::set_dump_ir(false);
    res.map(|_| ())
}

/// AOT: build the SAME (prelude + user) program from `entry` and emit a native
/// object file — the relocatable `.o`/`.obj` the linker turns into a standalone
/// binary. Shares the whole front-end + lowering with [`run_path`]; only the
/// backend (`ObjectModule`) and the synthesized `main` entry shim differ. Returns
/// the object bytes; the caller writes them and drives the linker.
pub fn compile_path_to_object(entry: &Path) -> FrontResult<Vec<u8>> {
    let prog = build_path(entry)?;
    super::module_aot::compile_program_aot(&prog)
}

/// Like [`run_path`] but CAPTURES `console.log` output into a `String` (used by
/// the in-process e2e tests). Mirrors [`super::render_source`] for the disk path.
pub fn render_path(entry: &Path) -> FrontResult<String> {
    let prog = build_path(entry)?;
    render_source_core(&prog)
}

/// Shared core: resolve `entry`, apply the import bindings, and build the merged
/// (prelude + user) [`super::LoweredProgram`]. The user side is NOT prelude
/// (`is_prelude=false`), so the privacy gate keeps denying `engine.*` to it.
fn build_path(entry: &Path) -> FrontResult<super::LoweredProgram> {
    let resolved = modules::load_program(entry)?;
    let modules::ResolvedProgram {
        mut program,
        bindings,
    } = resolved;

    let builtins = apply_bindings(&mut program, &bindings)?;

    // Build the engine prelude FIRST (if any) so its classes are AMBIENT for the
    // user program's `extends` resolution (`class X extends Error`/`Map`).
    let inc = registry::includes_prelude();
    let prelude = if inc.is_empty() {
        None
    } else {
        Some(super::build_program_for_prelude(&inc)?)
    };
    let ambient = prelude
        .as_ref()
        .map(|p| &p.classes)
        .unwrap_or(&EMPTY_AMBIENT);
    // AMBIENT prelude names (describe/test/expect + primordial `.ts` helpers AND
    // top-level singletons like `console`) — seeded into the user build's arrow
    // extraction so a user callback arrow that references one (`x => console.log(x)`)
    // is treated as a free top-level reference, not an unsound capture (see
    // `build_from_program` → `extract_arrows`). Uses the SAME `prelude_fn_names` the
    // single-string path does — it adds the `const x = new C()` singletons the bare
    // `p.funcs` scan missed (so `console` in a callback no longer bails).
    let ambient_fns: std::collections::HashSet<String> = prelude
        .as_ref()
        .map(super::prelude_fn_names)
        .unwrap_or_default();

    // The flattened multi-file USER program has no single source string, so the
    // PARAMETER-destructuring recovery (which re-parses a source) gets `""` — a
    // destructured param stays `"_"` and bails at lowering (sound). Every other
    // destructuring site recovers from the swc `Stmt` nodes in `program`.
    let mut user = build_from_program(program, "", ambient, &ambient_fns)?;
    // Carry the BUILTIN-IMPORT bindings into the lowering so a call to an imported
    // `rts:<ns>` member resolves to its real `__RTS_FN_NS_*` symbol (M1b).
    user.builtins = builtins;

    match prelude {
        None => Ok(user),
        Some(prelude) => merge_programs(prelude, user),
    }
}

/// An empty ambient class table for the no-prelude path (a shared default so
/// [`build_path`] borrows a stable reference).
static EMPTY_AMBIENT: std::sync::LazyLock<super::class::ClassTable> =
    std::sync::LazyLock::new(super::class::ClassTable::default);

/// Apply the import binding map to the flattened `program` IN PLACE, returning the
/// BUILTIN-IMPORT map (local name → `(ns, member)`) the lowering consults.
///
/// - For each `Binding::Local { name }` whose LOCAL key differs from the exported
///   `name` (an `as`-alias), rename every identifier reference of the local key to
///   `name` across the whole program so it resolves to the flattened declaration.
/// - Each `Binding::Builtin { ns, member }` is recorded in the returned map under
///   its LOCAL name. A CALL to that local resolves the real `__RTS_FN_NS_*` symbol
///   via [`super::registry::namespace_member`] (M1b). The `rts:<ns>` form is fully
///   wired; a bare-`"rts"` (`ns == ""`) entry — which imports a namespace OBJECT,
///   not a member — is still recorded, but the call lowering bails on it honestly
///   (no `namespace_member` match), as does any unknown member. A builtin name used
///   as a VALUE (not called) is unmodeled and bails at lowering, not here.
fn apply_bindings(
    program: &mut rts_ast::ast::Program,
    bindings: &HashMap<String, Binding>,
) -> FrontResult<HashMap<String, (String, String)>> {
    // 1. Split the bindings into alias renames (local → orig) and builtins.
    let mut renames: HashMap<String, String> = HashMap::new();
    let mut builtins: HashMap<String, (String, String)> = HashMap::new();
    for (local, binding) in bindings {
        match binding {
            Binding::Local { name } => {
                if local != name {
                    renames.insert(local.clone(), name.clone());
                }
            }
            Binding::Builtin { ns, member } => {
                builtins.insert(local.clone(), (ns.clone(), member.clone()));
            }
        }
    }

    // 2. Rewrite every identifier reference across the flattened program (only if
    //    there is an alias to rewrite).
    if !renames.is_empty() {
        let mut renamer = Renamer { renames: &renames };
        for item in &mut program.items {
            rename_item(item, &mut renamer);
        }
    }
    Ok(builtins)
}

/// Apply the [`Renamer`] to one flattened top-level item (statement body, function
/// body, or class property initializer — every swc subtree the lowering re-reads).
fn rename_item(item: &mut Item, renamer: &mut Renamer<'_>) {
    match item {
        Item::Statement(Statement::Raw(raw)) => {
            if let Some(stmt) = raw.stmt.as_mut() {
                stmt.visit_mut_with(renamer);
            }
        }
        Item::Function(f) => {
            for s in &mut f.body {
                let Statement::Raw(raw) = s;
                if let Some(stmt) = raw.stmt.as_mut() {
                    stmt.visit_mut_with(renamer);
                }
            }
        }
        Item::Class(c) => {
            for m in &mut c.members {
                rename_class_member(m, renamer);
            }
            for s in &mut c.static_init_body {
                let Statement::Raw(raw) = s;
                if let Some(stmt) = raw.stmt.as_mut() {
                    stmt.visit_mut_with(renamer);
                }
            }
        }
        // Imports/ExportNamespace are erased before flatten reaches here; an
        // Interface introduces no runtime references.
        Item::Import(_) | Item::ExportNamespace(_) | Item::Interface(_) => {}
    }
}

/// Rename references inside a class member's swc subtrees (method bodies + a
/// constructor body + a property initializer).
fn rename_class_member(m: &mut rts_ast::ast::ClassMember, renamer: &mut Renamer<'_>) {
    use rts_ast::ast::ClassMember;
    match m {
        ClassMember::Constructor(ctor) => {
            for s in &mut ctor.body {
                let Statement::Raw(raw) = s;
                if let Some(stmt) = raw.stmt.as_mut() {
                    stmt.visit_mut_with(renamer);
                }
            }
        }
        ClassMember::Method(md) => {
            for s in &mut md.body {
                let Statement::Raw(raw) = s;
                if let Some(stmt) = raw.stmt.as_mut() {
                    stmt.visit_mut_with(renamer);
                }
            }
        }
        ClassMember::Property(p) => {
            if let Some(init) = p.initializer.as_mut() {
                init.visit_mut_with(renamer);
            }
        }
    }
}

/// A focused swc `VisitMut` that rewrites an IDENTIFIER reference's symbol per the
/// `renames` map. Only `Ident` nodes are visited — swc models member-access
/// properties (`obj.add`) as `IdentName`/`MemberProp` and object-literal keys as
/// `PropName`, neither of which is an `Ident`, so a property/key named like an
/// imported alias is NOT touched. Binding occurrences (a same-named local) ARE
/// renamed too (a known limitation of the flat global binding map — fine for the
/// distinct-name common case).
struct Renamer<'a> {
    renames: &'a HashMap<String, String>,
}

impl swc_ecma_visit::VisitMut for Renamer<'_> {
    fn visit_mut_ident(&mut self, ident: &mut swc_ecma_ast::Ident) {
        if let Some(target) = self.renames.get(ident.sym.as_ref()) {
            ident.sym = target.clone().into();
        }
    }
}
