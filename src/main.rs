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
    //
    // Called by `rts-host` at the end of every run now (`rts_ui::
    // shutdown`), which is where the device's lifetime actually ends. This
    // reached the OLD runtime's copy of the same thing and was deleted with it —
    // a second shutdown from the bin would either be a no-op or a race to drop
    // the same device.

    std::process::exit(match status {
        Ok(()) => 0,
        Err(e) => {
            // One branch now. The other read a process-global diagnostic engine
            // that nothing had emitted into since the old engine's parser was
            // deleted, so `has_errors()` was a constant `false` — a dead branch
            // whose presence said errors could arrive two ways.
            let use_color = rts::errors::stderr_supports_color();
            eprint!("{}", rts::errors::format_anyhow_error(&e, use_color));
            1
        }
    });
}
