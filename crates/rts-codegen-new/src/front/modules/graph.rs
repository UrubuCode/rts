//! The module graph: BFS-load every reachable user module from an entry file,
//! dedup by canonical path, and detect import cycles (3-color DFS).
//!
//! Structure mirrors the old engine's `module/mod.rs` (`ModuleGraph::load` +
//! `detect_cycle`), but is a FRESH implementation: it depends on no old-engine
//! crate, parses through `rts_parser`, and reads each module's EXPORT set from
//! the parser's new `exported: bool` flag (not a line-scan).
//!
//! Builtins (`rts`/`rts:<ns>`/`node:<ns>`) are LEAF edges: recorded on the
//! importing module but never loaded from disk and never part of a cycle.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use rts_ast::ast::{Item, Program};

use super::error::{ModuleError, ModuleResult};
use super::resolve::{self, Target};

/// One import edge of a module, after specifier resolution.
#[derive(Debug, Clone)]
pub struct ImportEdge {
    /// The original specifier string (`./lib`, `rts:io`, …).
    pub specifier: String,
    /// The resolved target (file key / builtin / unsupported).
    pub target: Target,
    /// The named bindings this import introduces (`{ orig as local }`).
    pub names: Vec<(String, String)>,
    /// The default-import local name, if any (`import io from "..."`).
    pub default_name: Option<String>,
    /// True for a RE-EXPORT edge (`export * from` — empty `names` — or
    /// `export { x as y } from`): the target's exports join THIS module's
    /// export map (the post-load star-chain fixpoint).
    pub reexport: bool,
}

/// A single loaded user module.
#[derive(Debug, Clone)]
pub struct ModuleNode {
    /// Canonical absolute path (the dedup key).
    pub key: PathBuf,
    /// The parsed program (full item list, imports included).
    pub program: Program,
    /// Resolved import edges, in source order.
    pub imports: Vec<ImportEdge>,
    /// Export name → the FLAT top-level name it resolves to. For an own
    /// declaration the two coincide; a re-export alias (`export { add as plus }
    /// from`) maps `plus → add`; a NAMED default export maps `default → name`.
    pub exports: BTreeMap<String, String>,
    /// `export * as ns from "./m"` aggregates: `ns` → the resolved TARGET key.
    /// The flatten materializes the member map from the target's (fixpointed)
    /// exports and binds the importing side as a namespace-alias rewrite.
    pub ns_reexports: Vec<(String, PathBuf)>,
}

/// The loaded graph: the entry key + every reachable user module, keyed by
/// canonical path. Builtins are NOT nodes (they are leaf edges on importers).
#[derive(Debug)]
pub struct ModuleGraph {
    pub entry_key: PathBuf,
    pub modules: HashMap<PathBuf, ModuleNode>,
}

impl ModuleGraph {
    /// BFS-load the whole graph reachable from `entry` (already a file path).
    pub fn load(entry: &Path) -> ModuleResult<Self> {
        let entry_key = resolve::resolve_entry(entry)?;
        let mut modules: HashMap<PathBuf, ModuleNode> = HashMap::new();
        let mut pending: VecDeque<PathBuf> = VecDeque::new();
        pending.push_back(entry_key.clone());

        while let Some(key) = pending.pop_front() {
            if modules.contains_key(&key) {
                continue;
            }
            let node = load_one(&key)?;
            for edge in &node.imports {
                if let Target::File(dep) = &edge.target {
                    if !modules.contains_key(dep) {
                        pending.push_back(dep.clone());
                    }
                }
            }
            for (_, dep) in &node.ns_reexports {
                if !modules.contains_key(dep) {
                    pending.push_back(dep.clone());
                }
            }
            modules.insert(key, node);
        }

        propagate_reexports(&mut modules);

        let graph = Self { entry_key, modules };
        if let Some(cycle) = graph.detect_cycle() {
            return Err(ModuleError::Cycle(format_cycle(&cycle)));
        }
        Ok(graph)
    }

    /// 3-color DFS cycle detection over FILE edges only (builtins are leaves).
    /// Returns the cycle path (canonical keys) if one exists.
    fn detect_cycle(&self) -> Option<Vec<PathBuf>> {
        const WHITE: u8 = 0;
        const GRAY: u8 = 1;
        const BLACK: u8 = 2;

        let mut color: HashMap<&Path, u8> =
            self.modules.keys().map(|k| (k.as_path(), WHITE)).collect();
        let mut stack: Vec<&Path> = Vec::new();

        fn visit<'a>(
            graph: &'a ModuleGraph,
            key: &'a Path,
            color: &mut HashMap<&'a Path, u8>,
            stack: &mut Vec<&'a Path>,
        ) -> Option<Vec<PathBuf>> {
            color.insert(key, GRAY);
            stack.push(key);

            if let Some(node) = graph.modules.get(key) {
                for edge in &node.imports {
                    let Target::File(dep) = &edge.target else {
                        continue;
                    };
                    // Borrow the key actually stored in the map (stable lifetime).
                    let Some((dep_key, _)) = graph.modules.get_key_value(dep) else {
                        continue;
                    };
                    let dep_ref = dep_key.as_path();
                    match color.get(dep_ref).copied().unwrap_or(WHITE) {
                        WHITE => {
                            if let Some(c) = visit(graph, dep_ref, color, stack) {
                                return Some(c);
                            }
                        }
                        GRAY => {
                            let start =
                                stack.iter().position(|k| *k == dep_ref).unwrap_or(0);
                            let mut cycle: Vec<PathBuf> =
                                stack[start..].iter().map(|p| p.to_path_buf()).collect();
                            cycle.push(dep_ref.to_path_buf());
                            return Some(cycle);
                        }
                        _ => {}
                    }
                }
            }

            stack.pop();
            color.insert(key, BLACK);
            None
        }

        // Start at the entry, then sweep any unvisited (defensive — the graph is
        // entry-rooted so all nodes are reachable, but a disconnected node would
        // still get checked).
        let entry = self.modules.get_key_value(&self.entry_key)?.0.as_path();
        if let Some(c) = visit(self, entry, &mut color, &mut stack) {
            return Some(c);
        }
        let keys: Vec<&Path> = self.modules.keys().map(|k| k.as_path()).collect();
        for k in keys {
            if color.get(k).copied().unwrap_or(WHITE) == WHITE {
                if let Some(c) = visit(self, k, &mut color, &mut stack) {
                    return Some(c);
                }
            }
        }
        None
    }
}

/// Read + parse one module file, resolve its imports, and collect its exports.
fn load_one(key: &Path) -> ModuleResult<ModuleNode> {
    let source = std::fs::read_to_string(key)
        .map_err(|e| ModuleError::Io(format!("failed to read {}: {e}", key.display())))?;
    let program = rts_parser::parse_source(&source)
        .map_err(|e| ModuleError::Parse(format!("{}: {e}", key.display())))?;

    let mut imports = Vec::new();
    let mut ns_reexports = Vec::new();
    for item in &program.items {
        match item {
            Item::Import(decl) => {
                let target = resolve::resolve(&decl.from, key)?;
                let names = decl
                    .names
                    .iter()
                    .map(|n| (n.orig.clone(), n.local.clone()))
                    .collect();
                imports.push(ImportEdge {
                    specifier: decl.from.clone(),
                    target,
                    names,
                    default_name: decl.default_name.clone(),
                    reexport: decl.reexport,
                });
            }
            // `export * as ns from "./mod"` — a NAMESPACE AGGREGATE: record the
            // resolved target; the flatten materializes the member map from the
            // target's (fixpointed) exports and rewrites `ns.<m>` accesses.
            Item::ExportNamespace(d) => {
                let target = resolve::resolve(&d.from, key)?;
                let Target::File(dep) = target else {
                    return Err(ModuleError::Unsupported(format!(
                        "`export * as {} from \"{}\"` over a non-file module",
                        d.local, d.from
                    )));
                };
                ns_reexports.push((d.local.clone(), dep));
            }
            _ => {}
        }
    }

    let exports = collect_exports(&program);

    Ok(ModuleNode {
        key: key.to_path_buf(),
        program,
        imports,
        ns_reexports,
        exports,
    })
}

/// The set of top-level names a module exports, read from the parser's
/// `exported: bool` flag on functions/classes.
fn collect_exports(program: &Program) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for item in &program.items {
        match item {
            Item::Function(f) if f.exported => {
                map.insert(f.name.clone(), f.name.clone());
            }
            Item::Class(c) if c.exported => {
                map.insert(c.name.clone(), c.name.clone());
            }
            _ => {}
        }
    }
    // `export const`/`export let` names — a `Statement` carries no exported
    // flag, so the parser collected them straight off the swc module.
    for name in &program.exported_names {
        map.insert(name.clone(), name.clone());
    }
    // A NAMED `export default function NAME(){}` — exposed under "default".
    if let Some(name) = &program.default_export {
        map.insert("default".to_string(), name.clone());
    }
    map
}

/// The star-chain FIXPOINT: a `reexport` edge joins the target's export map
/// into the re-exporter's — an EMPTY-names edge (`export * from`) copies the
/// whole map EXCEPT `"default"` (per spec, `export *` never re-exports the
/// default); a NAMED edge (`export { x as y } from`) maps `y → target[x]`
/// (already resolved to the FLAT name, so chains stay transitive). Iterated to
/// a fixed point so chains of star re-exports converge (module counts are
/// small; each pass only ever ADDS entries, so it terminates).
fn propagate_reexports(modules: &mut HashMap<PathBuf, ModuleNode>) {
    loop {
        let mut updates: Vec<(PathBuf, String, String)> = Vec::new();
        for node in modules.values() {
            for edge in node.imports.iter().filter(|e| e.reexport) {
                let Target::File(dep) = &edge.target else {
                    continue;
                };
                let Some(target) = modules.get(dep) else {
                    continue;
                };
                if edge.names.is_empty() {
                    for (k, v) in &target.exports {
                        if k != "default" && node.exports.get(k) != Some(v) {
                            updates.push((node.key.clone(), k.clone(), v.clone()));
                        }
                    }
                } else {
                    for (orig, local) in &edge.names {
                        if let Some(v) = target.exports.get(orig) {
                            if node.exports.get(local) != Some(v) {
                                updates.push((node.key.clone(), local.clone(), v.clone()));
                            }
                        }
                    }
                }
            }
        }
        if updates.is_empty() {
            break;
        }
        for (key, k, v) in updates {
            if let Some(node) = modules.get_mut(&key) {
                node.exports.insert(k, v);
            }
        }
    }
}

/// Render a cycle path (`a.ts -> b.ts -> a.ts`) for the error message.
fn format_cycle(cycle: &[PathBuf]) -> String {
    let names: Vec<String> = cycle
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| p.display().to_string())
        })
        .collect();
    names.join(" -> ")
}

/// DFS post-order (dependencies before consumers) over FILE edges, returning the
/// canonical keys in flatten order. Used by `flatten.rs`.
pub fn post_order(graph: &ModuleGraph) -> Vec<PathBuf> {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut order: Vec<PathBuf> = Vec::new();
    dfs_post(graph, &graph.entry_key, &mut visited, &mut order);
    order
}

fn dfs_post(
    graph: &ModuleGraph,
    key: &Path,
    visited: &mut HashSet<PathBuf>,
    order: &mut Vec<PathBuf>,
) {
    if !visited.insert(key.to_path_buf()) {
        return;
    }
    if let Some(node) = graph.modules.get(key) {
        for edge in &node.imports {
            if let Target::File(dep) = &edge.target {
                dfs_post(graph, dep, visited, order);
            }
        }
    }
    order.push(key.to_path_buf());
}
