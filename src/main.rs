fn main() {
    // Retém o objeto do `rts-napi` no link do bin: as fns `napi_*` são símbolos
    // crus chamados só por `dlsym` de um `.node`, nunca pelo código Rust do bin;
    // sem esta referência o LTO descarta o crate e o `/EXPORT` falha com
    // LNK2001. Ver docs/specs/napi-implementation.md (Etapa 1).
    std::hint::black_box(rts_napi::force_link());

    rts::crash::install();
    rts_codegen::register_runtime_artifacts(rts::rt_artifacts);

    std::process::exit(match rts::cli::dispatch(std::env::args()) {
        Ok(()) => 0,
        Err(e) => {
            let use_color = rts::diagnostics::reporter::stderr_supports_color();
            let engine = rts::diagnostics::reporter::global_engine();
            if engine.has_errors() {
                eprint!("{}", engine.render_all(use_color));
            } else {
                eprint!(
                    "{}",
                    rts::diagnostics::reporter::format_anyhow_error(&e, use_color)
                );
            }
            1
        }
    });
}
