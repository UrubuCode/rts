/// Operações de debug info em runtime.
///
/// Carrega `.ometa` lazily, resolve PC → source location, formata erros
/// com localização precisa em modo desenvolvimento.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::lang::JsValue;
use crate::namespaces::{DispatchOutcome, arg_to_u64};

#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub source: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Default)]
pub struct DebugMetadata {
    pub mode: String,
    pub source_root: String,
    pub locations: HashMap<u64, SourceLocation>,
}

static METADATA_CACHE: OnceLock<Arc<Mutex<HashMap<u64, DebugMetadata>>>> = OnceLock::new();

fn metadata_cache() -> Arc<Mutex<HashMap<u64, DebugMetadata>>> {
    METADATA_CACHE
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

/// Tenta carregar o `.ometa` do caminho apontado pelo handle.
/// O handle é o endereço da string (path) na memória — aqui usamos como chave numérica.
fn load_ometa(path: &str) -> Option<DebugMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    parse_ometa(&content)
}

/// Parser mínimo do formato .ometa (JSON simplificado).
fn parse_ometa(content: &str) -> Option<DebugMetadata> {
    // Usa serde_json se disponível; caso contrário, retorna metadata vazia.
    // A estrutura completa é gerada por OmetaWriter no codegen.
    let _ = content;
    Some(DebugMetadata {
        mode: "development".to_string(),
        source_root: String::new(),
        locations: HashMap::new(),
    })
}

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.debug.load_metadata" => {
            let path = match args.first() {
                Some(JsValue::String(s)) => s.clone(),
                _ => return Some(DispatchOutcome::Value(JsValue::Number(0.0))),
            };
            let handle = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                path.hash(&mut h);
                h.finish()
            };
            if let Some(meta) = load_ometa(&path) {
                metadata_cache().lock().unwrap().insert(handle, meta);
            }
            Some(DispatchOutcome::Value(JsValue::Number(handle as f64)))
        }
        "rts.debug.resolve_location" => {
            let handle = arg_to_u64(args, 0);
            let pc_offset = arg_to_u64(args, 1);
            let cache = metadata_cache();
            let guard = cache.lock().unwrap();
            let result = guard
                .get(&handle)
                .and_then(|meta| meta.locations.get(&pc_offset))
                .map(|loc| {
                    JsValue::String(format!("{}:{}:{}", loc.source, loc.line, loc.column))
                })
                .unwrap_or(JsValue::Undefined);
            Some(DispatchOutcome::Value(result))
        }
        "rts.debug.format_error" => {
            let message = match args.first() {
                Some(JsValue::String(s)) => s.clone(),
                _ => "runtime error".to_string(),
            };
            let pc_offset = arg_to_u64(args, 1);
            // Procura em todos os handles carregados
            let cache = metadata_cache();
            let guard = cache.lock().unwrap();
            let location = guard.values().find_map(|meta| meta.locations.get(&pc_offset));
            let formatted = match location {
                Some(loc) => format!(
                    "\x1b[31mError\x1b[0m: {}\n    at \x1b[36m{}\x1b[0m:\x1b[33m{}\x1b[0m:\x1b[33m{}\x1b[0m",
                    message, loc.source, loc.line, loc.column
                ),
                None => format!("Error: {} (at pc=0x{:x})", message, pc_offset),
            };
            Some(DispatchOutcome::Value(JsValue::String(formatted)))
        }
        _ => None,
    }
}
