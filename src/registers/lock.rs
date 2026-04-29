use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const LOCK_VERSION: u32 = 1;
pub const LOCK_FILE: &str = "rts.lock";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LockFile {
    #[serde(rename = "lockfileVersion")]
    pub version: u32,
    pub packages: BTreeMap<String, LockEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LockEntry {
    pub provider: String,
    pub name: String,
    pub version: String,
    pub resolved: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
}

impl LockFile {
    pub fn new() -> Self {
        Self {
            version: LOCK_VERSION,
            packages: BTreeMap::new(),
        }
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(LOCK_FILE);
        if !path.exists() {
            return Ok(Self::new());
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join(LOCK_FILE);
        let content = serde_json::to_string_pretty(self).context("serialize lock")?;
        std::fs::write(&path, content)
            .with_context(|| format!("write {}", path.display()))
    }

    pub fn insert_npm(
        &mut self,
        name: &str,
        version: &str,
        resolved: &str,
        integrity: Option<&str>,
        deps: BTreeMap<String, String>,
    ) {
        let key = format!("{name}@{version}");
        self.packages.insert(
            key,
            LockEntry {
                provider: "npm".to_string(),
                name: name.to_string(),
                version: version.to_string(),
                resolved: resolved.to_string(),
                integrity: integrity.map(|s| s.to_string()),
                dependencies: deps,
            },
        );
    }

    pub fn get_by_name(&self, name: &str) -> Option<&LockEntry> {
        self.packages.values().find(|e| e.name == name)
    }
}
