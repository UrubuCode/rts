//! Flatten a loaded [`ModuleGraph`] into ONE `rts_ast::Program` + an
//! import-binding map.
//!
//! DFS post-order (dependencies before consumers) concatenates every user
//! module's top-level items EXCEPT `Item::Import`/`Item::ExportNamespace`
//! (imports are erased; their effect is captured in the binding map). While
//! concatenating we enforce the honesty floor:
//!
//! - a top-level NAME defined/exported by two different user modules that would
//!   collide in the flat program → [`ModuleError::NameCollision`] (NO last-wins);
//! - importing a name a user module does NOT export → [`ModuleError::MissingExport`].
//!
//! The binding map maps each importing module's LOCAL name to a [`Binding`]:
//! `Builtin { ns, member }` for `rts:<ns>`/`node:<ns>`/`rts`, or `Local { name }`
//! for a name another resolved user module exports.

use std::collections::HashMap;

use rts_ast::ast::{Item, Program};

use super::error::{ModuleError, ModuleResult};
use super::graph::{post_order, ModuleGraph};
use super::resolve::Target;

/// What an imported local name binds to in the flattened program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// A builtin member: `import { print } from "rts:io"` →
    /// `Builtin { ns: "io", member: "print" }`. For the bare `rts` module the
    /// `ns` is empty and `member` is the imported name. M1b resolves the real
    /// `__RTS_FN_*` symbol via the Registry; M1a only records the intent.
    Builtin { ns: String, member: String },
    /// A name exported by another resolved USER module, visible in the flat
    /// program under `name` (its top-level declaration name).
    Local { name: String },
}

/// Flatten `graph` into a single program + binding map.
pub fn flatten(graph: &ModuleGraph) -> ModuleResult<(Program, HashMap<String, Binding>)> {
    let order = post_order(graph);

    let mut program = Program::default();
    // Tracks which user module first defined each top-level name, for collision
    // diagnostics.
    let mut defined: HashMap<String, std::path::PathBuf> = HashMap::new();
    let mut bindings: HashMap<String, Binding> = HashMap::new();

    for key in &order {
        let Some(node) = graph.modules.get(key) else {
            continue;
        };

        // 1. Concatenate this module's declarations (skip imports), checking for
        //    top-level name collisions across modules.
        for item in &node.program.items {
            match item {
                Item::Import(_) | Item::ExportNamespace(_) => continue,
                _ => {}
            }
            if let Some(name) = top_level_name(item) {
                if let Some(prev) = defined.get(&name) {
                    if prev != key {
                        return Err(ModuleError::NameCollision { name });
                    }
                }
                defined.insert(name, key.clone());
            }
            program.items.push(item.clone());
        }

        // 2. Build the binding map for THIS module's imports.
        for edge in &node.imports {
            match &edge.target {
                Target::Builtin { specifier } => {
                    // `rts:test` is NOT a runtime namespace — it is the high-level
                    // test FRAMEWORK, embedded as the `TEST_BUNDLE_TS` prelude. Its
                    // `describe`/`test`/`expect` are AMBIENT prelude functions (merged
                    // into the flat program). Bind each imported name to the prelude
                    // declaration of the same name (`Binding::Local`): a plain import
                    // is a no-op rename and the bare call resolves to the prelude fn;
                    // an `as`-alias renames the reference. NOT a `Builtin` binding (a
                    // namespace-member call to a nonexistent `test` namespace).
                    if specifier == "rts:test" {
                        for (orig, local) in &edge.names {
                            bindings.insert(local.clone(), Binding::Local { name: orig.clone() });
                        }
                        continue;
                    }
                    let ns = builtin_ns(specifier);
                    // BARE `"rts"` (`ns == ""`): each imported name is a NAMESPACE
                    // OBJECT (`import { io, gc } from "rts"` → `io`/`gc`), so bind it
                    // as the namespace itself (member empty) — a later `gc.member(..)`
                    // resolves through `namespace_member`. A `rts:<ns>` import instead
                    // names a MEMBER of that one namespace.
                    let bare = ns.is_empty();
                    for (orig, local) in &edge.names {
                        let binding = if bare {
                            Binding::Builtin {
                                ns: orig.clone(),
                                member: String::new(),
                            }
                        } else {
                            Binding::Builtin {
                                ns: ns.clone(),
                                member: orig.clone(),
                            }
                        };
                        bindings.insert(local.clone(), binding);
                    }
                    if let Some(default_local) = &edge.default_name {
                        // Default import of a builtin namespace: bind to the
                        // namespace itself (empty member).
                        bindings.insert(
                            default_local.clone(),
                            Binding::Builtin {
                                ns: ns.clone(),
                                member: String::new(),
                            },
                        );
                    }
                }
                Target::File(dep) => {
                    let dep_node = graph.modules.get(dep).ok_or_else(|| {
                        ModuleError::Resolve(format!("dangling edge to {}", dep.display()))
                    })?;
                    for (orig, local) in &edge.names {
                        if !dep_node.exports.contains(orig) {
                            return Err(ModuleError::MissingExport {
                                name: orig.clone(),
                                from: edge.specifier.clone(),
                            });
                        }
                        bindings.insert(local.clone(), Binding::Local { name: orig.clone() });
                    }
                    if edge.default_name.is_some() {
                        return Err(ModuleError::Unsupported(format!(
                            "default import from user module '{}'",
                            edge.specifier
                        )));
                    }
                }
                Target::Unsupported { specifier } => {
                    return Err(ModuleError::Unsupported(format!(
                        "import from bare specifier '{specifier}'"
                    )));
                }
            }
        }
    }

    Ok((program, bindings))
}

/// The namespace key for a builtin specifier: `rts:io` → `io`, `node:fs` → `fs`,
/// bare `rts` → `""` (the member alone identifies it).
fn builtin_ns(specifier: &str) -> String {
    if let Some(ns) = specifier.strip_prefix("rts:") {
        ns.to_string()
    } else if let Some(ns) = specifier.strip_prefix("node:") {
        ns.to_string()
    } else {
        // bare "rts"
        String::new()
    }
}

/// The top-level declaration name of an item, if it introduces one (function /
/// class / interface). `Statement`/`Import`/`ExportNamespace` introduce none.
fn top_level_name(item: &Item) -> Option<String> {
    match item {
        Item::Function(f) => Some(f.name.clone()),
        Item::Class(c) => Some(c.name.clone()),
        Item::Interface(i) => Some(i.name.clone()),
        _ => None,
    }
}
