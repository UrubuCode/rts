//! Error type for the module subsystem.
//!
//! Internally the resolver/graph/flatten steps produce a structured
//! [`ModuleError`] (so tests can assert on the *kind* of failure: a cycle vs a
//! missing export vs a name collision). At the public boundary
//! ([`super::load_program`]) every `ModuleError` is funneled into the crate's
//! one bail type [`crate::front::error::Unsupported`] via [`From`], so the
//! module system speaks the same `FrontResult` language as the rest of the
//! front-end. Honesty floor: every failure is EXPLICIT — the resolver never
//! silently drops an import or last-wins a collision.

use crate::front::error::Unsupported;

/// A structured module-resolution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    /// Filesystem read / canonicalize failure.
    Io(String),
    /// A specifier could not be resolved to a file or builtin.
    Resolve(String),
    /// A parse error in one of the modules (carries the parser message).
    Parse(String),
    /// A circular import was detected; the message lists the cycle path.
    Cycle(String),
    /// An imported name is not exported by the resolved (user) module.
    MissingExport {
        /// The name the consumer tried to import.
        name: String,
        /// The specifier it tried to import from.
        from: String,
    },
    /// Two user modules contribute the same top-level name to the flat program.
    NameCollision {
        /// The colliding top-level name.
        name: String,
    },
    /// A construct outside M1 scope (e.g. a bare npm import reached as a real
    /// dependency edge, `import * as ns`).
    Unsupported(String),
}

/// Internal result alias for the module subsystem.
pub type ModuleResult<T> = Result<T, ModuleError>;

impl std::fmt::Display for ModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleError::Io(m) => write!(f, "module io error: {m}"),
            ModuleError::Resolve(m) => write!(f, "module resolution error: {m}"),
            ModuleError::Parse(m) => write!(f, "module parse error: {m}"),
            ModuleError::Cycle(m) => write!(f, "circular import detected: {m}"),
            ModuleError::MissingExport { name, from } => {
                write!(f, "'{name}' is not exported by '{from}'")
            }
            ModuleError::NameCollision { name } => {
                write!(f, "top-level name collision across modules: '{name}'")
            }
            ModuleError::Unsupported(m) => write!(f, "module feature unsupported in M1: {m}"),
        }
    }
}

impl From<ModuleError> for Unsupported {
    fn from(e: ModuleError) -> Self {
        Unsupported::new(e.to_string())
    }
}
