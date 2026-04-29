mod import_resolver;
pub mod manifest;
mod module_cache;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::compile_options::CompileOptions;
use crate::parser;
use crate::parser::ast::{Item, Program};

use import_resolver::{resolve_entry_path, resolve_import_target};
use manifest::{ManifestCache, find_owner_manifest};
use module_cache::ModuleCache;

#[derive(Debug, Clone)]
pub struct ResolvedImport {
    pub specifier: String,
    pub resolved_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    Entry,
    Source,
    WorkspacePackage,
    CachedDependency,
    Builtin,
}

impl ModuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Source => "source",
            Self::WorkspacePackage => "workspace-package",
            Self::CachedDependency => "cached-dependency",
            Self::Builtin => "builtin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceModule {
    pub key: String,
    pub path: PathBuf,
    pub source: String,
    pub program: Program,
    pub imports: Vec<ResolvedImport>,
    pub exports: BTreeSet<String>,
    pub kind: ModuleKind,
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
            kind: ModuleKind::Builtin,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleGraph {
    entry_key: String,
    modules: BTreeMap<String, SourceModule>,
}

impl ModuleGraph {
    pub fn load(entry_input: &Path, options: CompileOptions) -> Result<Self> {
        let entry_path = resolve_entry_path(entry_input)?;
        let entry_key = canonical_module_key(&entry_path)?;
        let workspace_root = discover_workspace_root(&entry_path)?;
        let module_cache = ModuleCache::discover()?;
        let mut manifest_cache = ManifestCache::default();

        let mut modules = BTreeMap::new();
        let mut pending = VecDeque::new();
        pending.push_back(PendingModule {
            path: entry_path.clone(),
            kind: ModuleKind::Entry,
            trace_route: vec![entry_path.display().to_string()],
        });

        while let Some(current) = pending.pop_front() {
            let module_key = canonical_module_key(&current.path)?;
            if modules.contains_key(&module_key) {
                continue;
            }

            let source = std::fs::read_to_string(&current.path).with_context(|| {
                attach_trace(
                    format!("failed to read module {}", current.path.display()),
                    &current.trace_route,
                    options,
                )
            })?;

            let file_id =
                crate::diagnostics::source_store::register(current.path.clone(), source.clone());
            let program = parser::parse_source_with_file(&source, options.frontend_mode, file_id)
                .with_context(|| {
                attach_trace(
                    format!("failed to parse module {}", current.path.display()),
                    &current.trace_route,
                    options,
                )
            })?;

            let owner_manifest = find_owner_manifest(
                &current.path,
                &workspace_root,
                &mut manifest_cache,
                options,
                &current.trace_route,
            )?;

            let mut imports = Vec::new();
            for (specifier, import_span) in collect_imports(&program) {
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

                let resolved = resolve_import_target(
                    &current.path,
                    &specifier,
                    import_span,
                    owner_manifest.as_ref(),
                    &workspace_root,
                    &module_cache,
                    &mut manifest_cache,
                    options,
                    &current.trace_route,
                )?;

                let resolved_key = canonical_module_key(&resolved.path)?;

                let mut trace_route = current.trace_route.clone();
                trace_route.push(format!("{} -> {}", specifier, resolved.path.display()));

                pending.push_back(PendingModule {
                    path: resolved.path,
                    kind: resolved.kind,
                    trace_route,
                });

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
                    path: current.path,
                    source,
                    program,
                    imports,
                    exports,
                    kind: current.kind,
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

    /// Retorna as chaves canonicas de todos os modulos que `module` importa
    /// direta ou transitivamente, em ordem determinstica (lexicografica).
    /// Nao inclui o proprio modulo.
    ///
    /// Usado para calcular `deps_hash` — SHA256 concatenado do conteudo
    /// de todos os transitivos, garantindo que alterar `c.ts` invalide
    /// o cache de `a.ts` quando `a -> b -> c`.
    pub fn transitive_deps(&self, module_key: &str) -> Vec<String> {
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> = Vec::new();

        if let Some(module) = self.modules.get(module_key) {
            for import in &module.imports {
                stack.push(import.resolved_key.clone());
            }
        }

        while let Some(key) = stack.pop() {
            if !visited.insert(key.clone()) {
                continue;
            }
            if let Some(dep) = self.modules.get(&key) {
                for import in &dep.imports {
                    if !visited.contains(&import.resolved_key) {
                        stack.push(import.resolved_key.clone());
                    }
                }
            }
        }

        visited.into_iter().collect()
    }

    /// Calcula SHA256 combinando o hash do source de cada dependencia
    /// transitiva (ordem lexicografica das chaves). Usado no cache para
    /// invalidar `.o` quando qualquer modulo importado mudou.
    ///
    /// Modulos builtin sao pulados: sua versao e controlada por `rts_version`
    /// ja presente no `ObjectCacheMeta`.
    pub fn transitive_deps_hash(&self, module_key: &str) -> String {
        use sha2::{Digest, Sha256};

        let deps = self.transitive_deps(module_key);
        let mut hasher = Sha256::new();
        for dep_key in &deps {
            if let Some(dep) = self.modules.get(dep_key) {
                if matches!(dep.kind, ModuleKind::Builtin) {
                    continue;
                }
                hasher.update(dep_key.as_bytes());
                hasher.update(b":");
                hasher.update(dep.source.as_bytes());
                hasher.update(b"\n");
            }
        }
        format!("{:x}", hasher.finalize())
    }

    /// Flattens the entire module graph into a single `Program` for JIT execution.
    ///
    /// Traverses modules in DFS post-order (dependencies before their consumers),
    /// strips all `Item::Import` declarations, and concatenates every item. The
    /// result can be compiled directly by `compile_program_to_jit` without any
    /// module-resolution infrastructure — builtin namespace calls already resolve
    /// via the ABI symbol table registered in the JIT builder.
    pub fn flatten_for_jit(&self) -> Program {
        let mut visited = std::collections::HashSet::new();
        let mut order: Vec<String> = Vec::new();
        self.dfs_post(&self.entry_key, &mut visited, &mut order);

        let mut merged = Program::default();
        for key in &order {
            let Some(module) = self.modules.get(key) else { continue };
            if module.kind == ModuleKind::Builtin {
                continue;
            }
            for item in &module.program.items {
                if matches!(item, Item::Import(_)) {
                    continue;
                }
                merged.items.push(item.clone());
            }
        }
        merged
    }

    fn dfs_post(
        &self,
        key: &str,
        visited: &mut std::collections::HashSet<String>,
        order: &mut Vec<String>,
    ) {
        if !visited.insert(key.to_string()) {
            return;
        }
        if let Some(module) = self.modules.get(key) {
            for import in &module.imports {
                self.dfs_post(&import.resolved_key, visited, order);
            }
        }
        order.push(key.to_string());
    }

    /// Retorna todos os paths (absolutos) dos modulos do grafo que sao
    /// arquivos em disco — util para registrar watchers e para `rts clean`.
    pub fn disk_paths(&self) -> Vec<PathBuf> {
        self.modules
            .values()
            .filter(|m| !matches!(m.kind, ModuleKind::Builtin))
            .map(|m| m.path.clone())
            .collect()
    }
}

#[derive(Debug, Clone)]
struct PendingModule {
    path: PathBuf,
    kind: ModuleKind,
    trace_route: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ImportTarget {
    pub(crate) path: PathBuf,
    pub(crate) kind: ModuleKind,
}

fn collect_imports(program: &Program) -> Vec<(String, crate::parser::span::Span)> {
    program
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Import(import_decl) = item {
                Some((import_decl.from.clone(), import_decl.span))
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

fn discover_workspace_root(entry_path: &Path) -> Result<PathBuf> {
    for ancestor in entry_path.ancestors() {
        if ancestor.join("packages").exists() {
            return ancestor.canonicalize().with_context(|| {
                format!(
                    "failed to canonicalize workspace root {}",
                    ancestor.display()
                )
            });
        }
    }

    std::env::current_dir().context("failed to resolve current directory for workspace root")
}

pub(crate) fn attach_trace(
    prefix: String,
    trace_route: &[String],
    options: CompileOptions,
) -> String {
    if !options.include_trace_data() || trace_route.is_empty() {
        return prefix;
    }

    let mut out = prefix;
    out.push_str("\nImport trace route:");
    for segment in trace_route {
        out.push_str("\n  - ");
        out.push_str(segment);
    }
    out
}

fn canonical_module_key(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize module {}", path.display()))?;

    Ok(canonical.to_string_lossy().to_string())
}
