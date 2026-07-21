fn main() {
    rts::crash::install();

    // Hand the CLI the bin-owned runtime-archive resolver so `rts compile` (AOT)
    // can locate the embedded `<host>.a` to link against (the archive + its
    // on-demand materialization live in this bin crate, which the CLI can't reach).
    rts::cli::set_runtime_archive_resolver(rts::rt_artifacts);

    // Step 10, slice 2: install the baked resident-prelude manifest (if this binary
    // was built with one). Empty → install nothing → the run path uses the fallback.
    if !rts::prelude_baked::MANIFEST.is_empty() {
        rts::cli::install_resident_prelude(rts::prelude_baked::MANIFEST.to_vec());
    }

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
