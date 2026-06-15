//! `front/modules` — the new engine's PURE module-resolution subsystem (M1a).
//!
//! Turns a multi-file TS program on disk into ONE flattened `rts_ast::Program`
//! plus an import-binding map, with NO lowering/JIT (that is M1b). The pipeline:
//!
//! ```text
//! entry &Path
//!   --resolve--> canonical entry key
//!   --graph::load--> BFS over relative imports, dedup by canonical path,
//!                    3-color cycle detection (builtins are leaf edges)
//!   --flatten--> DFS post-order concat of every module's top-level items
//!                (imports erased) + the LOCAL-name -> Binding map
//!   = ResolvedProgram { program, bindings }
//! ```
//!
//! This is a FRESH implementation (not an extraction of `rts-codegen-old`): it
//! depends on no old-engine crate, reads the runtime only through the
//! `rts-runtime` facade (and in M1a not even that — builtin member resolution is
//! deferred to M1b's Registry dispatch), and reads each module's export set from
//! the parser's `exported: bool` flag. See `docs/specs/rts-codegen-new-design.md`
//! §15.
//!
//! Honesty floor: EVERY failure (cycle, missing export, name collision, bare
//! npm import, namespace re-export) is an explicit error — the resolver never
//! silently drops an import or last-wins a collision.

pub mod error;
pub mod flatten;
pub mod graph;
pub mod resolve;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;

pub use flatten::Binding;

use crate::front::error::FrontResult;

use graph::ModuleGraph;

/// The output of [`load_program`]: one flattened AST program (M1b lowers this)
/// plus the import-binding map (M1b consults it to resolve a local identifier to
/// either a builtin member or another module's top-level declaration).
#[derive(Debug)]
pub struct ResolvedProgram {
    /// All resolved user modules concatenated in dependency order, with every
    /// `Item::Import`/`Item::ExportNamespace` erased.
    pub program: rts_ast::ast::Program,
    /// Maps an imported LOCAL name (as seen by the importing module) to its
    /// [`Binding`] — a builtin member or another user module's export.
    pub bindings: HashMap<String, Binding>,
}

/// Resolve, load, and flatten the whole program reachable from `entry`.
///
/// Returns a [`ResolvedProgram`] on success. Any failure (entry not found, a
/// missing dependency, a parse error, an import cycle, a name a module does not
/// export, a top-level name collision, an out-of-scope construct) is funneled
/// into the crate's [`crate::front::error::Unsupported`] bail type so the module
/// system speaks the same `FrontResult` language as the rest of the front-end.
pub fn load_program(entry: &Path) -> FrontResult<ResolvedProgram> {
    let graph = ModuleGraph::load(entry)?;
    let (program, bindings) = flatten::flatten(&graph)?;
    Ok(ResolvedProgram { program, bindings })
}
