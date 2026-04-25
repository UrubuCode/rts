use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cache::ObjCache;
use crate::codegen::ObjectArtifact;
use crate::compile_options::CompileOptions;
use crate::linker::{self, LinkedBinary};
use crate::parser;

#[derive(Debug, Clone)]
pub struct CompileOutcome {
    pub input: PathBuf,
    pub object: ObjectArtifact,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LinkOutcome {
    pub compile: CompileOutcome,
    pub binary: LinkedBinary,
    pub runtime_objects: Vec<PathBuf>,
    pub from_cache: bool,
}

/// Parses `input` and emits an object file next to it.
pub fn compile_file(
    input: &Path,
    output_object: &Path,
    options: CompileOptions,
) -> Result<CompileOutcome> {
    let source = std::fs::read_to_string(input)
        .with_context(|| format!("failed to read {}", input.display()))?;
    compile_source(&source, input, output_object, options)
}

/// Parses an in-memory source and emits an object.
pub fn compile_source(
    source: &str,
    input: &Path,
    output_object: &Path,
    options: CompileOptions,
) -> Result<CompileOutcome> {
    let mut program = parser::parse_source_with_mode(source, options.frontend_mode)
        .with_context(|| format!("failed to parse {}", input.display()))?;

    let (object, warnings) = crate::codegen::compile_program_to_object(&mut program, output_object)?;

    Ok(CompileOutcome {
        input: input.to_path_buf(),
        object,
        warnings,
    })
}

/// Full compile + link: produces an executable at `output_binary`.
///
/// User object and namespace objects are cached under
/// `node_modules/.rts/` relative to the nearest `package.json`.
pub fn build_executable(
    input: &Path,
    output_binary: &Path,
    options: CompileOptions,
) -> Result<LinkOutcome> {
    let cache = ObjCache::for_input(input);

    let (obj_path, compile_outcome, from_cache) =
        match cache.lookup(input).context("cache lookup failed")? {
            Some(hit) => {
                let fake_artifact = ObjectArtifact {
                    path: hit.obj_path.clone(),
                    bytes_written: std::fs::metadata(&hit.obj_path)
                        .map(|m| m.len() as usize)
                        .unwrap_or(0),
                    emitted_calls: 0,
                    used_namespaces: hit.used_namespaces,
                };
                let outcome = CompileOutcome {
                    input: input.to_path_buf(),
                    object: fake_artifact,
                    warnings: vec![],
                };
                (hit.obj_path, outcome, true)
            }
            None => {
                let tmp_obj = std::env::temp_dir()
                    .join(format!("rts_compile_{}.o", std::process::id()));
                let compile = compile_file(input, &tmp_obj, options)?;
                let used_ns = compile.object.used_namespaces.clone();
                let cached_path = cache
                    .store(input, &tmp_obj, &used_ns)
                    .context("failed to store compiled object in cache")?;
                let _ = std::fs::remove_file(&tmp_obj);
                (cached_path, compile, false)
            }
        };

    let runtime_archive =
        crate::runtime_objects::extract_runtime_archive(&cache.runtime_dir())
            .context("failed to extract runtime archive")?;

    let inputs = vec![obj_path, runtime_archive.clone()];

    let binary = linker::link_objects_to_binary(&inputs, output_binary)
        .context("linker failed")?;

    Ok(LinkOutcome {
        compile: compile_outcome,
        binary,
        runtime_objects: vec![runtime_archive],
        from_cache,
    })
}

/// Parses `input` and runs it directly in memory via Cranelift JIT.
///
/// Skips the object-file + system-linker cycle entirely: the program is
/// compiled into executable memory by `JITModule`, the `__RTS_MAIN`
/// symbol is resolved to a raw function pointer, and the binary is
/// invoked with an `extern "C"` transmute. Returns the exit code the
/// program produced.
///
/// This is the hot path for `rts run` — no disk I/O after parse, no
/// linker spawn. AOT (`rts compile`) keeps going through
/// [`build_executable`].
pub fn run_jit(input: &Path, options: CompileOptions) -> Result<(i32, Vec<String>)> {
    let source = std::fs::read_to_string(input)
        .with_context(|| format!("failed to read {}", input.display()))?;
    let mut program = parser::parse_source_with_mode(&source, options.frontend_mode)
        .with_context(|| format!("failed to parse {}", input.display()))?;

    let (module, warnings) = crate::codegen::compile_program_to_jit(&mut program)
        .context("JIT compile failed")?;

    // Resolve `__RTS_MAIN`. The codegen pipeline always emits it with
    // Linkage::Local + platform default call conv (`int __RTS_MAIN(void)`).
    use cranelift_module::Module;
    let name = "__RTS_MAIN";
    let main_id = match module.get_name(name) {
        Some(cranelift_module::FuncOrDataId::Func(id)) => id,
        _ => anyhow::bail!("JIT: `{name}` not found in module"),
    };
    let main_ptr = module.get_finalized_function(main_id);
    // SAFETY: codegen guarantees __RTS_MAIN matches this signature.
    let main_fn: extern "C" fn() -> i32 = unsafe { std::mem::transmute(main_ptr) };
    let exit_code = main_fn();

    // JITModule owns the executable pages — keep it alive until the
    // call returns. Leaking is fine for one-shot `rts run`: the process
    // exits right after this function.
    std::mem::forget(module);

    Ok((exit_code, warnings))
}

/// Returns the set of namespaces inferred from a pre-compiled object's
/// extern symbols without re-running codegen.
pub fn namespaces_from_symbols(symbols: &HashSet<String>) -> HashSet<String> {
    symbols
        .iter()
        .filter_map(|s| {
            let rest = s
                .strip_prefix("__RTS_FN_NS_")
                .or_else(|| s.strip_prefix("__RTS_CONST_NS_"))?;
            let ns = rest.split('_').next()?;
            if ns.is_empty() { None } else { Some(ns.to_ascii_lowercase()) }
        })
        .collect()
}
