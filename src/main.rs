fn main() {
    rts::crash::install();

    // Hand the CLI the bin-owned runtime-archive resolver so `rts compile` (AOT)
    // can locate the embedded `<host>.a` to link against (the archive + its
    // on-demand materialization live in this bin crate, which the CLI can't reach).
    rts::cli::set_runtime_archive_resolver(rts::rt_artifacts);

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
