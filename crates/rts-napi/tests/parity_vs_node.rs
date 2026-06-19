//! Teste de paridade diferencial: o MESMO addon N-API `.node` produz a mesma
//! saída no Node e no RTS. Valida que a implementação N-API do RTS é
//! comportamentalmente compatível com o Node real (não só com a spec).
//!
//! Requer: `rustc`, `node`, e o toolchain MSVC (`lib.exe`/`dumpbin.exe`) para
//! gerar as import libs do host. Em qualquer ausência, o teste é PULADO (não
//! falha) — ambientes de CI sem MSVC/Node não quebram. No Windows do dev (com
//! tudo presente) ele roda de verdade.
//!
//! Ver docs/specs/napi-implementation.md (Etapa 11, paridade).

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

const ADDON_SRC: &str = r#"
use std::ffi::{c_char, c_void};
use std::ptr;
#[repr(C)] #[derive(Clone, Copy)] pub struct E(*mut c_void);
#[repr(C)] #[derive(Clone, Copy)] pub struct V(*mut c_void);
#[repr(C)] #[derive(Clone, Copy)] pub struct I(*mut c_void);
type Cb = unsafe extern "C" fn(E, I) -> V;
extern "C" {
    fn napi_create_double(e: E, v: f64, r: *mut V) -> i32;
    fn napi_get_value_double(e: E, v: V, r: *mut f64) -> i32;
    fn napi_create_function(e: E, n: *const c_char, l: usize, cb: Option<Cb>, d: *mut c_void, r: *mut V) -> i32;
    fn napi_get_cb_info(e: E, i: I, argc: *mut usize, argv: *mut V, this: *mut V, d: *mut *mut c_void) -> i32;
    fn napi_set_named_property(e: E, o: V, n: *const c_char, v: V) -> i32;
}
unsafe extern "C" fn add(e: E, info: I) -> V {
    let mut argc = 2usize;
    let mut argv = [V(ptr::null_mut()); 2];
    napi_get_cb_info(e, info, &mut argc, argv.as_mut_ptr(), ptr::null_mut(), ptr::null_mut());
    let mut a = 0.0; let mut b = 0.0;
    napi_get_value_double(e, argv[0], &mut a);
    napi_get_value_double(e, argv[1], &mut b);
    let mut r = V(ptr::null_mut());
    napi_create_double(e, a + b, &mut r);
    r
}
#[no_mangle]
pub extern "C" fn napi_register_module_v1(e: E, exports: V) -> V {
    unsafe {
        let mut f = V(ptr::null_mut());
        napi_create_function(e, ptr::null(), 0, Some(add), ptr::null_mut(), &mut f);
        napi_set_named_property(e, exports, b"add\0".as_ptr() as *const c_char, f);
    }
    exports
}
"#;

fn tool(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Localiza `lib.exe`/`dumpbin.exe` no PATH (devem estar disponíveis num
/// developer prompt). Retorna `None` se ausente.
fn msvc_tool(name: &str) -> Option<String> {
    // No developer prompt, estão no PATH.
    if Command::new(name).arg("/?").output().map(|o| o.status.success() || o.status.code() == Some(0)).unwrap_or(false)
        || Command::new(name).output().is_ok()
    {
        return Some(name.to_string());
    }
    None
}

fn rts_exe() -> Option<PathBuf> {
    // O bin release do workspace.
    let candidates = [
        PathBuf::from("target/release/rts.exe"),
        PathBuf::from("../../target/release/rts.exe"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Gera uma import lib `<out>` a partir dos símbolos `napi_*` exportados por
/// `host_exe`, apontando para `host_name`.
fn make_import_lib(dumpbin: &str, lib: &str, host_exe: &Path, host_name: &str, dir: &Path, out: &str) -> Option<PathBuf> {
    let exports = Command::new(dumpbin)
        .arg("/EXPORTS")
        .arg(host_exe)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&exports.stdout);
    let mut syms: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_whitespace().find(|w| w.starts_with("napi_")))
        .collect();
    syms.sort_unstable();
    syms.dedup();
    if syms.is_empty() {
        return None;
    }
    let def_path = dir.join(format!("{out}.def"));
    let mut def = String::from("EXPORTS\n");
    for s in &syms {
        def.push_str(s);
        def.push('\n');
    }
    std::fs::write(&def_path, def).ok()?;
    let lib_path = dir.join(format!("{out}.lib"));
    let ok = Command::new(lib)
        .arg(format!("/DEF:{}", def_path.display()))
        .arg(format!("/OUT:{}", lib_path.display()))
        .arg("/MACHINE:X64")
        .arg(format!("/NAME:{host_name}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok && lib_path.exists() { Some(lib_path) } else { None }
}

fn build_addon(dir: &Path, lib_dir: &Path, lib_name: &str, out_node: &str) -> Option<PathBuf> {
    let src = dir.join("addon.rs");
    std::fs::write(&src, ADDON_SRC).ok()?;
    let out_dir = dir.to_path_buf();
    let lib_dir_win = lib_dir.to_string_lossy().replace('/', "\\");
    let status = Command::new("rustc")
        .args(["--crate-type", "cdylib", "--crate-name", "addon"])
        .arg(&src)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("-C")
        .arg(format!("link-arg=/LIBPATH:{lib_dir_win}"))
        .arg("-C")
        .arg(format!("link-arg={lib_name}.lib"))
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let dll = out_dir.join("addon.dll");
    if !dll.exists() {
        return None;
    }
    let node = out_dir.join(out_node);
    std::fs::copy(&dll, &node).ok()?;
    Some(node)
}

#[test]
fn addon_add_matches_node() {
    // Pré-requisitos: rustc, node, dumpbin, lib, e o rts.exe release.
    if !tool("rustc") || !tool("node") {
        eprintln!("rustc/node ausente — pulando paridade");
        return;
    }
    let (Some(dumpbin), Some(lib)) = (msvc_tool("dumpbin.exe"), msvc_tool("lib.exe")) else {
        eprintln!("toolchain MSVC (dumpbin/lib) ausente — pulando paridade");
        return;
    };
    let Some(rts) = rts_exe() else {
        eprintln!("target/release/rts.exe ausente (rode `cargo build --release` antes) — pulando");
        return;
    };
    let node_exe = match Command::new("where").arg("node.exe").output() {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            PathBuf::from(s.lines().next().unwrap_or("").trim())
        }
        _ => {
            eprintln!("node.exe não localizado — pulando");
            return;
        }
    };

    let tmp = std::env::temp_dir().join(format!("rts_napi_parity_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);

    // Import libs dos dois hosts.
    let Some(_rts_lib) = make_import_lib(&dumpbin, &lib, &rts, "rts.exe", &tmp, "napi_rts") else {
        eprintln!("falha ao gerar import lib do rts.exe — pulando");
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    };
    let Some(_node_lib) = make_import_lib(&dumpbin, &lib, &node_exe, "node.exe", &tmp, "napi_node") else {
        eprintln!("falha ao gerar import lib do node.exe — pulando");
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    };

    // Addons linkados contra cada host.
    let Some(rts_node) = build_addon(&tmp, &tmp, "napi_rts", "add_rts.node") else {
        eprintln!("falha ao compilar addon p/ RTS — pulando");
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    };
    let Some(node_node) = build_addon(&tmp, &tmp, "napi_node", "add_node.node") else {
        eprintln!("falha ao compilar addon p/ Node — pulando");
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    };

    // Programas equivalentes.
    let ts = tmp.join("t.ts");
    std::fs::write(
        &ts,
        r#"import a from "./add_rts.node";
console.log(a.add(2,3));
console.log(a.add(10,7));
console.log(a.add(-1,1));
"#,
    )
    .unwrap();
    let js = tmp.join("t.js");
    std::fs::write(
        &js,
        r#"const a = require('./add_node.node');
console.log(a.add(2,3));
console.log(a.add(10,7));
console.log(a.add(-1,1));
"#,
    )
    .unwrap();

    let rts_out = Command::new(&rts)
        .arg("run")
        .arg("--allow-native-addons")
        .arg(&ts)
        .output()
        .expect("rodar rts");
    let node_out = Command::new(&node_exe).arg(&js).output().expect("rodar node");

    let rts_stdout = String::from_utf8_lossy(&rts_out.stdout);
    let node_stdout = String::from_utf8_lossy(&node_out.stdout);

    let norm = |s: &str| s.lines().map(|l| l.trim()).collect::<Vec<_>>().join("\n");
    let _ = (&rts_node, &node_node);

    assert_eq!(
        norm(&rts_stdout),
        norm(&node_stdout),
        "saída do RTS deve bater com a do Node.\nRTS:\n{rts_stdout}\nNODE:\n{node_stdout}"
    );
    // Sanidade: deve conter os valores esperados.
    assert!(norm(&node_stdout).contains("5"), "node deveria imprimir 5");

    let _ = std::fs::remove_dir_all(&tmp);
}
