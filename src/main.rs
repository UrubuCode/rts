fn main() {
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
