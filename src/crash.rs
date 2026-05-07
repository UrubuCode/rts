/// Crash handler: panic hook + Windows USEF para segfaults.
///
/// Segfaults no RTS vêm de: handle inválido em member_get/set, transmute
/// de fn ptr com calling convention errada, THIS_SLOT vazio, etc. Sem
/// handler o processo morre silenciosamente (exit -1073741819).
///
/// Uso: chamar `crash::install()` no início de `main()`.
use std::sync::atomic::{AtomicBool, Ordering};

static INSTALLED: AtomicBool = AtomicBool::new(false);

pub fn install() {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    install_panic_hook();
    #[cfg(windows)]
    install_windows_handler();
}

// ── Panic hook ────────────────────────────────────────────────────────────────

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| String::from("<unknown>"));

        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            String::from("<non-string payload>")
        };

        eprintln!("\n\x1b[1;31m[RTS PANIC]\x1b[0m {}", msg);
        eprintln!("  at \x1b[33m{}\x1b[0m", location);

        print_backtrace("Panic backtrace");
    }));
}

// ── Windows unhandled exception filter ───────────────────────────────────────

#[cfg(windows)]
fn install_windows_handler() {
    unsafe {
        SetUnhandledExceptionFilter(Some(unhandled_exception_filter));
    }
}

#[cfg(windows)]
unsafe extern "system" fn unhandled_exception_filter(
    info: *mut ExceptionPointers,
) -> i32 {
    if info.is_null() {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let record = unsafe { &*(*info).exception_record };
    let code = record.exception_code;

    let name = match code {
        0xC000_0005 => "ACCESS_VIOLATION",
        0xC000_00FD => "STACK_OVERFLOW",
        0xC000_001D => "ILLEGAL_INSTRUCTION",
        0xC000_0006 => "IN_PAGE_ERROR",
        0x8000_0003 => "BREAKPOINT",
        _ => return EXCEPTION_CONTINUE_SEARCH,
    };

    let addr_info = if code == 0xC000_0005u32 && record.number_parameters >= 2 {
        let op = match record.exception_information[0] {
            0 => "leitura de",
            1 => "escrita em",
            8 => "execução de",
            _ => "acesso a",
        };
        format!(" ({} endereço 0x{:016X})", op, record.exception_information[1])
    } else {
        String::new()
    };

    let ip = record.exception_address as u64;

    eprintln!(
        "\n\x1b[1;31m[RTS CRASH]\x1b[0m {} (código 0x{:08X}){}\n  \
         Instrução: 0x{:016X}",
        name, code, addr_info, ip
    );

    // Tenta imprimir o nome do símbolo Rust para o IP do crash.
    print_symbol_at(ip);
    print_backtrace("Stack trace");

    eprintln!(
        "\n\x1b[2mDicas de diagnóstico:\x1b[0m\
         \n  • Compile com \x1b[33mcargo build\x1b[0m (sem --release) para símbolos completos\
         \n  • Defina \x1b[33mRUST_BACKTRACE=full\x1b[0m para rastrear mais frames\
         \n  • Use \x1b[33mrts ir arquivo.ts\x1b[0m para inspecionar o IR Cranelift gerado\
         \n  • Segfaults comuns: handle GC inválido, fn ptr com callconv errada\n"
    );

    std::process::exit(1);
}

// ── Backtrace ─────────────────────────────────────────────────────────────────

fn print_backtrace(label: &str) {
    let bt = backtrace::Backtrace::new();
    eprintln!("\n\x1b[2m{}:\x1b[0m", label);

    let mut shown = 0usize;
    for (idx, frame) in bt.frames().iter().enumerate() {
        for sym in frame.symbols() {
            let name = sym
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|| String::from("<unknown>"));

            if should_skip(&name) {
                continue;
            }

            let loc = match (sym.filename(), sym.lineno()) {
                (Some(f), Some(l)) => {
                    // Mostra apenas o path relativo ao workspace para legibilidade.
                    let path = f.to_string_lossy();
                    let short = shorten_path(&path);
                    format!("\x1b[2m  @ {}:{}\x1b[0m", short, l)
                }
                (Some(f), None) => format!("\x1b[2m  @ {}\x1b[0m", f.display()),
                _ => String::new(),
            };

            eprintln!("  \x1b[2m#{:<3}\x1b[0m \x1b[36m{}\x1b[0m{}", idx, name, if loc.is_empty() { String::new() } else { format!("\n{}", loc) });
            shown += 1;

            if shown >= 50 {
                eprintln!("  ... (truncado em 50 frames)");
                return;
            }
        }
    }

    if shown == 0 {
        eprintln!("  (sem símbolos — use build debug ou RUST_BACKTRACE=full)");
    }
}

fn print_symbol_at(addr: u64) {
    backtrace::resolve(addr as *mut std::ffi::c_void, |sym| {
        if let Some(name) = sym.name() {
            eprintln!("  Símbolo: \x1b[33m{}\x1b[0m", name);
        }
    });
}

fn should_skip(name: &str) -> bool {
    const SKIP: &[&str] = &[
        "backtrace::",
        "std::panicking",
        "std::panic",
        "core::panicking",
        "rust_begin_unwind",
        "__rust_",
        "___rust_",
        "_start",
        "BaseThreadInitThunk",
        "RtlUserThreadStart",
        "rts::crash::",
        "std::rt::",
        "core::ops::function",
    ];
    SKIP.iter().any(|p| name.starts_with(p))
}

fn shorten_path(path: &str) -> &str {
    // Mostra só a partir de `src/` ou `crates/` para reduzir ruído.
    for anchor in &["\\src\\", "/src/", "\\crates\\", "/crates/"] {
        if let Some(pos) = path.find(anchor) {
            return &path[pos + 1..];
        }
    }
    path
}

// ── FFI declarations para Windows (sem dep em windows-sys) ────────────────────

#[cfg(windows)]
unsafe extern "system" {
    fn SetUnhandledExceptionFilter(
        handler: Option<unsafe extern "system" fn(*mut ExceptionPointers) -> i32>,
    ) -> Option<unsafe extern "system" fn(*mut ExceptionPointers) -> i32>;
}

#[cfg(windows)]
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

#[cfg(windows)]
#[repr(C)]
struct ExceptionRecord {
    exception_code: u32,
    exception_flags: u32,
    exception_record: *mut ExceptionRecord,
    exception_address: *mut std::ffi::c_void,
    number_parameters: u32,
    _pad: u32,
    exception_information: [usize; 15],
}

#[cfg(windows)]
#[repr(C)]
struct ExceptionPointers {
    exception_record: *mut ExceptionRecord,
    context_record: *mut std::ffi::c_void,
}
