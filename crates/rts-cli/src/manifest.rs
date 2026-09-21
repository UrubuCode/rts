//! Minimal `package.json` parsing for `rts install`. The full module-graph
//! manifest resolver lived in the old engine (deleted at the P5 cutover); only the
//! raw deserialization + JSON-comment stripping the installer needs is kept here.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The subset of `package.json` the installer reads.
#[derive(Debug, Deserialize)]
pub struct RawPackageManifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub main: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// Strip JSONC down to JSON.
///
/// Moved to `rts-host` so the module loader can call it for `tsconfig.json`:
/// `rts-cli` depends on `rts-host` and not the other way round, so the shared
/// answer has to live in the lower crate. Kept as a name here because callers
/// in this crate spell it this way.
pub use rts_host::jsonc::strip as strip_json_comments;
