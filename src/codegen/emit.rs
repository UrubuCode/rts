//! Cranelift-backed object emitter.
//!
//! Compiles a full [`Program`] — user functions, control flow, variables,
//! arithmetic, and namespace calls — into a native `.o` file with a `main`
//! entry point.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::codegen::lower::compile_program;
use crate::codegen::object::ObjectArtifact;
use crate::parser::ast::Program;

/// Compiles a program into a native object file.
pub fn compile_program_to_object(
    program: &mut Program,
    output_path: &Path,
) -> Result<(ObjectArtifact, Vec<String>)> {
    let mut module = build_module()?;
    let mut extern_cache = HashMap::new();
    let mut data_counter: u32 = 0;

    let warnings = compile_program(program, &mut module, &mut extern_cache, &mut data_counter)?;

    let used_namespaces = collect_used_namespaces(&extern_cache);

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|err| anyhow!("cranelift object emission failed: {err}"))?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(output_path, &bytes)
        .with_context(|| format!("failed to write object to {}", output_path.display()))?;

    let emitted_calls = extern_cache
        .keys()
        .filter(|s| s.starts_with("__RTS_FN_NS_") || s.starts_with("__RTS_CONST_NS_"))
        .count();

    Ok((
        ObjectArtifact {
            path: output_path.to_path_buf(),
            bytes_written: bytes.len(),
            emitted_calls,
            used_namespaces,
        },
        warnings,
    ))
}

fn collect_used_namespaces(
    extern_cache: &HashMap<&'static str, cranelift_module::FuncId>,
) -> HashSet<String> {
    extern_cache
        .keys()
        .filter_map(|sym| namespace_from_symbol(sym))
        .collect()
}

fn namespace_from_symbol(symbol: &str) -> Option<String> {
    let rest = symbol
        .strip_prefix("__RTS_FN_NS_")
        .or_else(|| symbol.strip_prefix("__RTS_CONST_NS_"))?;
    let ns_upper = rest.split('_').next()?;
    if ns_upper.is_empty() {
        return None;
    }
    Some(ns_upper.to_ascii_lowercase())
}

fn build_module() -> Result<ObjectModule> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    // Speed: inlining and LICM help hot loops like bench/monte_carlo_pi;
    // impact on non-loop code is negligible.
    flag_builder
        .set("opt_level", "speed")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    // Egraph e alias analysis sao default-on em Cranelift recente mas
    // a explicitacao garante CSE/constant fold/LICM agressivo.
    let _ = flag_builder.set("use_egraphs", "true");
    let _ = flag_builder.set("enable_alias_analysis", "true");
    let _ = flag_builder.set("enable_jump_tables", "true");
    // Tail calls (#93) require frame pointers on x86-64 in Cranelift 0.131.
    flag_builder
        .set("preserve_frame_pointers", "true")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    let flags = settings::Flags::new(flag_builder);

    let isa_builder =
        cranelift_native::builder().map_err(|e| anyhow!("failed to detect native target: {e}"))?;
    let isa = isa_builder
        .finish(flags)
        .map_err(|e| anyhow!("failed to finalise ISA: {e}"))?;

    let builder = ObjectBuilder::new(
        isa,
        b"rts_entry".to_vec(),
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| anyhow!("failed to build object module: {e}"))?;

    Ok(ObjectModule::new(builder))
}
