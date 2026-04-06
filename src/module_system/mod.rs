use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use crate::parser;
use crate::parser::ast::{Item, Program};

#[derive(Debug, Clone)]
pub struct ResolvedImport {
    pub specifier: String,
    pub resolved_key: String,
}

#[derive(Debug, Clone)]
pub struct SourceModule {
    pub key: String,
    pub path: PathBuf,
    pub source: String,
    pub program: Program,
    pub imports: Vec<ResolvedImport>,
    pub exports: BTreeSet<String>,
}

impl SourceModule {
    fn from_builtin(module: crate::runtime::BuiltinModule) -> Self {
        Self {
            key: module.key.clone(),
            path: PathBuf::from(module.key.clone()),
            source: String::new(),
            program: Program::default(),
            imports: Vec::new(),
            exports: module.exports,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleGraph {
    entry_key: String,
    modules: BTreeMap<String, SourceModule>,
}

impl ModuleGraph {
    pub fn load(entry_input: &Path) -> Result<Self> {
        let entry_path = resolve_entry_path(entry_input)?;
        let entry_key = canonical_module_key(&entry_path)?;

        let mut modules = BTreeMap::new();
        let mut pending = VecDeque::new();
        pending.push_back(entry_path);

        while let Some(module_path) = pending.pop_front() {
            let module_key = canonical_module_key(&module_path)?;
            if modules.contains_key(&module_key) {
                continue;
            }

            let source = std::fs::read_to_string(&module_path)
                .with_context(|| format!("failed to read module {}", module_path.display()))?;

            let program = parser::parse_source(&source)
                .with_context(|| format!("failed to parse module {}", module_path.display()))?;

            let mut imports = Vec::new();
            for specifier in collect_imports(&program) {
                if let Some(builtin) = crate::runtime::builtin_module(&specifier) {
                    let resolved_key = builtin.key.clone();
                    modules
                        .entry(resolved_key.clone())
                        .or_insert_with(|| SourceModule::from_builtin(builtin));

                    imports.push(ResolvedImport {
                        specifier,
                        resolved_key,
                    });

                    continue;
                }

                let resolved_path = resolve_import_path(&module_path, &specifier)?;
                let resolved_key = canonical_module_key(&resolved_path)?;
                pending.push_back(resolved_path);

                imports.push(ResolvedImport {
                    specifier,
                    resolved_key,
                });
            }

            let exports = collect_exports(&source);

            modules.insert(
                module_key.clone(),
                SourceModule {
                    key: module_key,
                    path: module_path,
                    source,
                    program,
                    imports,
                    exports,
                },
            );
        }

        Ok(Self { entry_key, modules })
    }

    pub fn entry(&self) -> Option<&SourceModule> {
        self.modules.get(&self.entry_key)
    }

    pub fn modules(&self) -> impl Iterator<Item = &SourceModule> {
        self.modules.values()
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    pub fn import_exports_for(&self, module: &SourceModule) -> BTreeMap<String, BTreeSet<String>> {
        let mut map = BTreeMap::new();

        for import in &module.imports {
            if let Some(target) = self.modules.get(&import.resolved_key) {
                map.insert(import.specifier.clone(), target.exports.clone());
            }
        }

        map
    }
}

fn collect_imports(program: &Program) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Import(import_decl) = item {
                Some(import_decl.from.clone())
            } else {
                None
            }
        })
        .collect()
}

fn collect_exports(source: &str) -> BTreeSet<String> {
    let mut exports = BTreeSet::new();

    for raw_line in source.lines() {
        let line = strip_inline_comment(trim_bom(raw_line).trim());
        if let Some(rest) = line.strip_prefix("export ") {
            parse_export_line(rest.trim(), &mut exports);
        }
    }

    exports
}

fn parse_export_line(rest: &str, exports: &mut BTreeSet<String>) {
    for keyword in ["class", "interface", "function", "const", "let", "var"] {
        let prefix = format!("{keyword} ");
        if let Some(decl) = rest.strip_prefix(&prefix) {
            let name = parse_decl_name(decl);
            if !name.is_empty() {
                exports.insert(name);
            }
            return;
        }
    }

    if let Some(named) = rest.strip_prefix('{') {
        let Some(close) = named.find('}') else {
            return;
        };

        let names_raw = &named[..close];
        for piece in names_raw.split(',') {
            let symbol = piece.trim();
            if symbol.is_empty() {
                continue;
            }

            if let Some((_, alias)) = symbol.split_once(" as ") {
                let alias = alias.trim();
                if !alias.is_empty() {
                    exports.insert(alias.to_string());
                }
            } else {
                exports.insert(symbol.to_string());
            }
        }
    }
}

fn parse_decl_name(text: &str) -> String {
    text.split(|c: char| c == '{' || c == '(' || c == ';' || c == ':' || c.is_whitespace())
        .find(|segment| !segment.is_empty())
        .unwrap_or("")
        .to_string()
}

fn trim_bom(line: &str) -> &str {
    line.trim_start_matches('\u{feff}')
}

fn strip_inline_comment(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else {
        line
    }
    .trim()
}

fn resolve_entry_path(input: &Path) -> Result<PathBuf> {
    if input.exists() {
        validate_source_extension(input)?;
        return input
            .canonicalize()
            .with_context(|| format!("failed to canonicalize entry path {}", input.display()));
    }

    if input.extension().is_some() {
        bail!("entry module not found: {}", input.display());
    }

    for candidate in [input.with_extension("ts"), input.with_extension("rts")] {
        if candidate.exists() {
            return candidate.canonicalize().with_context(|| {
                format!("failed to canonicalize entry path {}", candidate.display())
            });
        }
    }

    bail!(
        "entry module not found. tried: {}, {} and {}",
        input.display(),
        input.with_extension("ts").display(),
        input.with_extension("rts").display()
    )
}

fn validate_source_extension(path: &Path) -> Result<()> {
    let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
        bail!("source file must have .rts or .ts extension: {}", path.display());
    };

    if ext != "rts" && ext != "ts" {
        bail!(
            "unsupported source extension '.{}' in {} (expected .rts or .ts)",
            ext,
            path.display()
        );
    }

    Ok(())
}

fn resolve_import_path(current_module: &Path, specifier: &str) -> Result<PathBuf> {
    if specifier.starts_with('.') {
        let base_dir = current_module
            .parent()
            .ok_or_else(|| anyhow!("module has no parent directory: {}", current_module.display()))?;
        return resolve_user_module(base_dir, specifier);
    }

    bail!(
        "unsupported import specifier '{}' in {}. only relative imports and builtin modules are available",
        specifier,
        current_module.display()
    )
}

fn resolve_user_module(base_dir: &Path, specifier: &str) -> Result<PathBuf> {
    let candidate = base_dir.join(specifier);

    let mut attempts = Vec::new();

    if candidate.extension().is_some() {
        attempts.push(candidate.clone());
    } else {
        attempts.push(candidate.with_extension("ts"));
        attempts.push(candidate.with_extension("rts"));
        attempts.push(candidate.join("index.ts"));
        attempts.push(candidate.join("index.rts"));
    }

    for path in attempts {
        if path.exists() {
            validate_source_extension(&path)?;
            return path.canonicalize().with_context(|| {
                format!("failed to canonicalize import module {}", path.display())
            });
        }
    }

    bail!(
        "unable to resolve import '{}' from {}",
        specifier,
        base_dir.display()
    )
}

fn canonical_module_key(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize module {}", path.display()))?;

    Ok(canonical.to_string_lossy().to_string())
}
