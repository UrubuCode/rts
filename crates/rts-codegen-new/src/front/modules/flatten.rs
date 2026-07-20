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
use super::graph::{ModuleGraph, post_order};
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

        // 1b. Synthesize each `export * as ns from "./mod"` aggregate as a
        //     namespace OBJECT literal `const ns = { a: a, b: b };` referencing
        //     the target's exports (post-order guarantees the target's items
        //     already precede this point). Parsed through `rts_parser` so the
        //     statement carries a real swc node (the object-literal lowering
        //     materializes fn-valued props; `ns.f()` invokes the fn value).
        for (local, target) in &node.ns_reexports {
            let dep_node = graph.modules.get(target).ok_or_else(|| {
                ModuleError::Resolve(format!("dangling ns re-export to {}", target.display()))
            })?;
            let fields: Vec<String> = dep_node
                .exports
                .iter()
                .map(|n| format!("{n}: {n}"))
                .collect();
            let synth = format!("const {local} = {{ {} }};", fields.join(", "));
            let parsed = rts_parser::parse_source(&synth)
                .map_err(|e| ModuleError::Parse(format!("ns re-export `{local}` synth: {e}")))?;
            if let Some(prev) = defined.get(local) {
                if prev != key {
                    return Err(ModuleError::NameCollision {
                        name: local.clone(),
                    });
                }
            }
            defined.insert(local.clone(), key.clone());
            program.items.extend(parsed.items);
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
                    // Some `node:` modules RE-EXPORT an engine GLOBAL class/value of
                    // the same name (`import { URL } from "node:url"` → the ambient
                    // `URL` class) — REUSE, never a duplicated impl. Such a name binds
                    // like the bare global (`Binding::Local`, a no-op rename that
                    // resolves to the ambient class). Any OTHER imported name from the
                    // same module is a native namespace member (`url.fileURLToPath`),
                    // resolved through the Registry below.
                    if let Some(reexports) = node_reexported_globals(specifier) {
                        let ns = builtin_ns(specifier);
                        let subns = node_subnamespace_reexports(specifier);
                        for (orig, local) in &edge.names {
                            // Priority: a SUB-NAMESPACE object (`fs.promises`) →
                            // bind to that whole namespace; else a re-exported name
                            // → its AMBIENT prelude declaration (possibly renamed);
                            // else a native namespace member via the Registry.
                            let binding = if let Some((_, sub)) =
                                subns.and_then(|t| t.iter().find(|(o, _)| *o == orig.as_str()))
                            {
                                Binding::Builtin {
                                    ns: (*sub).to_string(),
                                    member: String::new(),
                                }
                            } else if let Some((_, decl)) =
                                reexports.iter().find(|(o, _)| *o == orig.as_str())
                            {
                                Binding::Local {
                                    name: decl.to_string(),
                                }
                            } else {
                                Binding::Builtin {
                                    ns: ns.clone(),
                                    member: orig.clone(),
                                }
                            };
                            bindings.insert(local.clone(), binding);
                        }
                        // `import stream from "node:stream"` — the default export
                        // is the module namespace object; synthesize it as an
                        // object of the ambient re-exported decls so `stream.X`
                        // resolves. ONLY for modules whose WHOLE surface is
                        // re-exported (a partial one like node:fs must instead bind
                        // its default to the native namespace, below, so
                        // `fs.readFileSync` still resolves).
                        if let (Some(default_local), true) =
                            (&edge.default_name, node_reexport_whole_surface(specifier))
                        {
                            let fields: Vec<String> = reexports
                                .iter()
                                .map(|(o, decl)| format!("{o}: {decl}"))
                                .collect();
                            let synth =
                                format!("const {default_local} = {{ {} }};", fields.join(", "));
                            let parsed = rts_parser::parse_source(&synth).map_err(|e| {
                                ModuleError::Parse(format!(
                                    "node default re-export `{default_local}` synth: {e}"
                                ))
                            })?;
                            program.items.extend(parsed.items);
                        } else if let Some(default_local) = &edge.default_name {
                            // Partial re-export (node:fs): the default export is the
                            // native namespace object — `fs.readFileSync` resolves
                            // through the Registry (the ambient stream classes are
                            // reached via the NAMED import, not `fs.createReadStream`).
                            bindings.insert(
                                default_local.clone(),
                                Binding::Builtin {
                                    ns: ns.clone(),
                                    member: String::new(),
                                },
                            );
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
                    let subns = node_subnamespace_reexports(specifier);
                    for (orig, local) in &edge.names {
                        let binding = if let Some((_, sub)) =
                            subns.and_then(|t| t.iter().find(|(o, _)| *o == orig.as_str()))
                        {
                            // A sub-namespace object (`fs.promises`) — bind to the
                            // whole namespace, member empty, like a bare-`rts`
                            // namespace import; `promises.readFile()` then resolves
                            // through the Registry on that sub-namespace.
                            Binding::Builtin {
                                ns: (*sub).to_string(),
                                member: String::new(),
                            }
                        } else if bare {
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
                        // TRANSITIVE re-export: `orig` may be an alias the dep
                        // itself re-exported (`export { add as plus }` — no flat
                        // declaration named `plus`). Post-order processed the dep
                        // first, so its binding for `orig` already points at the
                        // REAL flat declaration — follow it.
                        let binding = match bindings.get(orig) {
                            Some(b) if !defined.contains_key(orig) => b.clone(),
                            _ => Binding::Local { name: orig.clone() },
                        };
                        bindings.insert(local.clone(), binding);
                    }
                    if let Some(default_local) = &edge.default_name {
                        // `import answer from "./mod"` — bind the local to the
                        // dep's `export default` flat declaration name.
                        let Some(def) = &dep_node.default_export else {
                            return Err(ModuleError::Unsupported(format!(
                                "default import from user module '{}' (module has no `export default` declaration)",
                                edge.specifier
                            )));
                        };
                        bindings
                            .insert(default_local.clone(), Binding::Local { name: def.clone() });
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
/// The engine GLOBAL classes/values a `node:` module RE-EXPORTS (reused, never
/// re-implemented): `import { URL } from "node:url"` binds to the ambient `URL`
/// class. Data-driven; only names that are real registered engine globals belong
/// here. A name NOT listed is treated as a native namespace member.
/// Node modules that expose a SUB-NAMESPACE OBJECT under a member name — e.g.
/// `fs.promises` is the entire `node:fs/promises` API, reachable both as
/// `import { promises } from "node:fs"` and `fsDefault.promises`. Data-driven
/// (import-name → sub-namespace specifier): the name binds to that namespace as
/// an OBJECT (`Binding::Builtin` with an empty member), so `promises.readFile(p)`
/// resolves through the Registry exactly like a bare-`rts` namespace import. This
/// generalizes to any module with a promise/sub-API object (`dns.promises`,
/// `timers.promises`) by adding a row — no per-name control-flow special-case.
fn node_subnamespace_reexports(specifier: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match specifier {
        "node:fs" => Some(&[("promises", "node:fs/promises")]),
        _ => None,
    }
}

/// Whether a `node:` module's `node_reexported_globals` covers its WHOLE surface
/// (every export is an ambient prelude decl). True → a default import synthesizes
/// a namespace object of those decls; false (a PARTIAL re-export like `node:fs`,
/// which mixes native members with a few ambient stream classes) → the default
/// import binds the native namespace instead.
fn node_reexport_whole_surface(specifier: &str) -> bool {
    !matches!(specifier, "node:fs" | "node:fs/promises")
}

fn node_reexported_globals(specifier: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match specifier {
        // node:fs — MOSTLY native members (readFileSync/…), but the stream classes
        // + their factories are ambient `.ts` prelude decls (fs/stream.ts). Only
        // these four re-export; every other name stays a native Registry member.
        // NOT a whole-surface re-export (see `node_reexport_whole_surface`), so a
        // default `import fs from "node:fs"` still binds the native namespace.
        "node:fs" => Some(&[
            ("createReadStream", "createReadStream"),
            ("createWriteStream", "createWriteStream"),
            ("ReadStream", "ReadStream"),
            ("WriteStream", "WriteStream"),
            ("Utf8Stream", "Utf8Stream"),
        ]),
        // node:fs/promises — mostly native, but `open` is a `.ts` wrapper that
        // augments the native FileHandle with its stream methods (fhstream.ts).
        "node:fs/promises" => Some(&[("open", "__fsPromisesOpen")]),
        "node:url" => Some(&[("URL", "URL"), ("URLSearchParams", "URLSearchParams")]),
        // `StringDecoder` is a registered global (Registry) class — bind the
        // import to the ambient class, like `URL` (reuse, never re-implement).
        "node:string_decoder" => Some(&[("StringDecoder", "StringDecoder")]),
        // node:diagnostics_channel — ambient `.ts` prelude. Classes keep their
        // names; the module fns are renamed `__dc*` (generic names like `channel`/
        // `subscribe` would otherwise be ambient globals colliding with user code).
        "node:diagnostics_channel" => Some(&[
            ("channel", "__dcChannel"),
            ("hasSubscribers", "__dcHasSubscribers"),
            ("subscribe", "__dcSubscribe"),
            ("unsubscribe", "__dcUnsubscribe"),
            ("tracingChannel", "__dcTracingChannel"),
            ("Channel", "Channel"),
            ("TracingChannel", "TracingChannel"),
        ]),
        // `node:stream` — every class/fn is an ambient `.ts` prelude decl of the
        // SAME name (see rts-node `stream.ts`/…). Data-driven: (import, decl).
        "node:stream" => Some(&[
            ("Stream", "Stream"),
            ("Readable", "Readable"),
            ("Writable", "Writable"),
            ("Duplex", "Duplex"),
            ("Transform", "Transform"),
            ("PassThrough", "PassThrough"),
            ("pipeline", "pipeline"),
            ("finished", "finished"),
            ("compose", "compose"),
            ("duplexPair", "duplexPair"),
            ("isErrored", "isErrored"),
            ("isReadable", "isReadable"),
            ("isWritable", "isWritable"),
            ("addAbortSignal", "addAbortSignal"),
            ("getDefaultHighWaterMark", "getDefaultHighWaterMark"),
            ("setDefaultHighWaterMark", "setDefaultHighWaterMark"),
        ]),
        // Submodule decls are prefixed to avoid clashing with the base module's
        // `pipeline`/`finished` (callback vs promise form).
        "node:stream/promises" => Some(&[
            ("pipeline", "__streamPromisesPipeline"),
            ("finished", "__streamPromisesFinished"),
        ]),
        "node:stream/consumers" => Some(&[
            ("text", "__streamConsumersText"),
            ("json", "__streamConsumersJson"),
            ("buffer", "__streamConsumersBuffer"),
            ("arrayBuffer", "__streamConsumersArrayBuffer"),
            ("bytes", "__streamConsumersBytes"),
            ("blob", "__streamConsumersBlob"),
        ]),
        // WHATWG streams: re-export the ambient web-stream globals (rts-shared
        // `streams.ts`). Only the classes that ambient prelude actually provides.
        "node:stream/web" => Some(&[
            ("ReadableStream", "ReadableStream"),
            ("WritableStream", "WritableStream"),
            ("TransformStream", "TransformStream"),
            ("TextEncoderStream", "TextEncoderStream"),
            ("TextDecoderStream", "TextDecoderStream"),
        ]),
        _ => None,
    }
}

fn builtin_ns(specifier: &str) -> String {
    if let Some(ns) = specifier.strip_prefix("rts:") {
        ns.to_string()
    } else if specifier.starts_with("node:") {
        // KEEP the scheme for `node:` — the module resolves by its DECLARED key
        // (`e.module("node","X")` → registry key `node:X`), so rts-node can OWN a
        // module distinct from the shared `rts:X`. `resolve.rs`/`registry.rs`
        // apply a transparent `node:X` → `rts:X` fallback for modules rts-node has
        // not natively provided yet (data-driven; no per-module special-case).
        specifier.to_string()
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
