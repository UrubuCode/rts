fn main() {
    rts::crash::install();

    // Hand the CLI the bin-owned runtime-archive resolver so `rts compile` (AOT)
    // can locate the embedded `<host>.a` to link against (the archive + its
    // on-demand materialization live in this bin crate, which the CLI can't reach).
    rts::cli::set_runtime_archive_resolver(rts::rt_artifacts);


    let status = rts::cli::dispatch(std::env::args());

    // Release the GPU device HERE, not from its thread-local destructor.
    //
    // `std::process::exit` below skips thread-local destructors on the normal
    // path, but Windows still runs them from the TLS callback inside
    // `LdrShutdownProcess` — i.e. while the driver's DLLs are already being
    // unloaded. Dropping a live `wgpu::Device` at that point made the AMD D3D12
    // UMD raise `__fastfail` (`0xC0000409`, `STATUS_STACK_BUFFER_OVERRUN`): the
    // program printed everything correctly and then died non-zero with no
    // message, which the test harness rightly counted as a failed file. It
    // reproduced for any run that had synchronized the device even once.
    //
    // No-op when no GPU was ever created, so every other command pays nothing.
    rts_runtime::namespaces::gpu::shutdown();

    std::process::exit(match status {
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
