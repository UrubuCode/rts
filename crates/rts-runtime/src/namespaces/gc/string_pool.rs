//! String-producing ABI for the GC namespace.

use super::handles::{Entry, alloc_entry, free_handle, with_entry, with_two_entries};

/// Reads a string handle into an owned Rust `String`.
pub fn read_string_handle(handle: u64) -> Option<String> {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    })
}

/// Allocates a new string by copying `len` bytes from `ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64 {
    if ptr.is_null() || len < 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    alloc_entry(Entry::String(slice.to_vec()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(bytes)) => bytes.len() as i64,
        _ => -1,
    })
}

/// Returns a pointer to the string's buffer. Valid until the handle is freed.
///
/// # Safety
/// Caller must not read past `LEN` bytes or access after `free`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_PTR(handle: u64) -> *const u8 {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(bytes)) => bytes.as_ptr(),
        _ => std::ptr::null(),
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FREE(handle: u64) -> i64 {
    if free_handle(handle) { 1 } else { 0 }
}

/// Generic length dispatcher — backs `.size`/`.length` in codegen.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_HANDLE_LEN(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(b)) => b.len() as i64,
        Some(Entry::Map(m)) => m.len() as i64,
        Some(Entry::Vec(v)) => v.len() as i64,
        Some(Entry::Buffer(b)) => b.len() as i64,
        Some(Entry::Env(s)) => s.len() as i64,
        _ => -1,
    })
}

/// (#208 / Array.isArray) Returns 1 if handle aponta para um Vec, 0 caso contrário.
/// Backing pra Array.isArray(x) no codegen.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_VEC(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(_)) => 1,
        _ => 0,
    })
}

/// instanceof Date — handle aponta para Entry::DateMs.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_DATE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::DateMs(_)) => 1,
        _ => 0,
    })
}

/// instanceof RegExp — handle aponta para Entry::Regex.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_REGEX(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(_)) => 1,
        _ => 0,
    })
}

/// instanceof Map/Set/WeakMap/WeakSet/Object — handle aponta para Entry::Map
/// ou WeakMap/WeakSet (todos sao map-like). Object aceita qualquer Map.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_MAP_LIKE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(_)) | Some(Entry::WeakMap(_)) => 1,
        _ => 0,
    })
}

/// instanceof Promise — handle aponta para Entry::PromiseAsync.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_IS_PROMISE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(_)) => 1,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_I64(value: i64) -> u64 {
    alloc_entry(Entry::String(value.to_string().into_bytes()))
}

/// (#proto-method) Coerce inteligente para template literal:
/// se \`value\` (i64) eh handle valido para Entry::String, retorna o
/// proprio handle. Senao, formata como integer (\`STRING_FROM_I64\`).
/// Usado pra \`${expr}\` quando \`expr\` veio de var_member_call cujo
/// retorno tem tipo dinamico.
///
/// Rendering por tipo de Entry:
/// - String: passthrough do handle
/// - Vec<i64>: JS-style \`1,2,3\` (sem espacos, separador virgula —
///   semantica de \`Array.prototype.toString\`)
/// - Map: \`[object Object]\` (semantica JS para objects sem
///   toString custom)
/// - Demais (Buffer, sockets, etc.): formato \`[object <Kind>]\`
/// - Handle invalido / nao-handle: trata o valor como i64 raw
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TPL_COERCE_AUTO(value: i64) -> u64 {
    let h = value as u64;
    let snap = snapshot_entry(h);
    match snap {
        EntrySnap::Str(_) => h, // passthrough do handle original
        EntrySnap::None => {
            // Nao eh handle valido — interpreta como i64 raw (legacy).
            alloc_entry(Entry::String(value.to_string().into_bytes()))
        }
        other => {
            let bytes = snapshot_to_bytes(&other);
            alloc_entry(Entry::String(bytes))
        }
    }
}

/// Helper: nome textual do kind de Entry (para fallback `[object Kind]`).
fn entry_kind_name(e: &Entry) -> &'static str {
    match e {
        Entry::String(_) => "String",
        Entry::Buffer(_) => "Buffer",
        Entry::Vec(_) => "Array",
        Entry::Map(_) => "Object",
        Entry::Json(_) => "Json",
        Entry::BigFixed(_) => "BigFixed",
        Entry::ProcessChild(_) => "ProcessChild",
        Entry::TcpListener(_) => "TcpListener",
        Entry::TcpStream(_) => "TcpStream",
        Entry::UdpSocket(_) => "UdpSocket",
        Entry::Function { .. } => "Function",
        _ => "Object",
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_F64(value: f64) -> u64 {
    let s = format_js_number(value);
    alloc_entry(Entry::String(s.into_bytes()))
}

/// `typeof <handle-valued expression>` — retorna handle de string com
/// o tipo JS apropriado para o conteudo do handle. Cobre os casos que
/// o codegen nao consegue resolver estaticamente (var Symbol, member
/// access que pode ser symbol, etc.).
///
/// Mapeamento:
/// - Entry::Symbol -> "symbol"
/// - Entry::Function -> "function"
/// - Entry::String -> "string"
/// - Entry::Vec / Entry::Map / outros -> "object"
/// - Handle invalido -> "string" (preserva semantica historica)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TYPEOF_HANDLE(handle: u64) -> u64 {
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Symbol { .. }) => "symbol",
        Some(Entry::Function(_)) => "function",
        Some(Entry::String(_)) => "string",
        Some(Entry::Vec(_)) | Some(Entry::Map(_)) | Some(Entry::Buffer(_))
        | Some(Entry::Json(_)) | Some(Entry::DateMs(_))
        | Some(Entry::PromiseAsync(_)) | Some(Entry::Promise(_)) => "object",
        Some(_) => "object",
        None => "string",
    });
    alloc_entry(Entry::String(kind.as_bytes().to_vec()))
}

/// Formata f64 conforme JS spec ToString:
/// - NaN -> "NaN"
/// - +-Infinity -> "Infinity" / "-Infinity"
/// - integer dentro de [-1e21, 1e21] -> sem ponto decimal
/// - magnitude muito alta ou baixa (< 1e-6, >= 1e21) -> notation exponencial
/// - resto -> minimal decimal Rust
pub fn format_js_number(value: f64) -> String {
    if value.is_nan() { return "NaN".to_string(); }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() };
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let abs = value.abs();
    // JS usa exponential quando abs < 1e-6 ou >= 1e21
    if abs >= 1e21 || abs < 1e-6 {
        // Rust default f64 Display: "1e308" — JS usa "1e+308". Ajustar sinal.
        let s = format!("{value:e}");
        // Garante sinal explicito no expoente: "1e308" -> "1e+308"
        if let Some(epos) = s.find('e') {
            let (mantissa, exp) = s.split_at(epos);
            let exp_body = &exp[1..]; // skip 'e'
            if !exp_body.starts_with('-') && !exp_body.starts_with('+') {
                return format!("{}e+{}", mantissa, exp_body);
            }
        }
        return s;
    }
    if value.fract() == 0.0 && abs < 1e16 {
        return format!("{}", value as i64);
    }
    format!("{value}")
}

/// Concatenates two string handles and returns a new handle.
///
/// Para handles que NAO sao String (Vec, Map, etc.), aplica coercao
/// JS-style identica a `TPL_COERCE_AUTO`:
/// - Vec<i64> -> "1,2,3"
/// - Map -> "[object Object]"
/// - Outros -> "[object <Kind>]"
///
/// Handles invalidos viram string vazia (semantica JS template literal).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64 {
    // Coleta dados dos dois handles ANTES de qualquer processamento
    // recursivo (que precisa re-acessar o HandleTable).
    let snap_a = snapshot_entry(a);
    let snap_b = snapshot_entry(b);
    let mut out = snapshot_to_bytes(&snap_a);
    out.extend_from_slice(&snapshot_to_bytes(&snap_b));
    alloc_entry(Entry::String(out))
}

/// Snapshot minimo de uma Entry — copia o que basta para formatar
/// fora do lock, evitando deadlock recursivo.
enum EntrySnap {
    Str(Vec<u8>),
    Vec(Vec<i64>),
    Map,
    Buffer,
    Json(String),
    Other(&'static str),
    None,
}

fn snapshot_entry(h: u64) -> EntrySnap {
    with_entry(h, |e| match e {
        Some(Entry::String(s)) => EntrySnap::Str(s.clone()),
        Some(Entry::Vec(v)) => EntrySnap::Vec((**v).clone()),
        Some(Entry::Map(_)) => EntrySnap::Map,
        Some(Entry::Buffer(_)) => EntrySnap::Buffer,
        Some(Entry::Json(j)) => EntrySnap::Json(j.to_string()),
        Some(other) => EntrySnap::Other(entry_kind_name(other)),
        None => EntrySnap::None,
    })
}

fn snapshot_to_bytes(s: &EntrySnap) -> Vec<u8> {
    match s {
        EntrySnap::Str(b) => b.clone(),
        EntrySnap::Vec(slots) => {
            let parts: Vec<String> = slots.iter().map(|x| element_to_string(*x)).collect();
            parts.join(",").into_bytes()
        }
        EntrySnap::Map => b"[object Object]".to_vec(),
        EntrySnap::Buffer => b"[object Buffer]".to_vec(),
        EntrySnap::Json(j) => j.clone().into_bytes(),
        EntrySnap::Other(name) => format!("[object {}]", name).into_bytes(),
        EntrySnap::None => Vec::new(),
    }
}

/// Converte um i64 raw (slot de Vec) numa string. Tenta lookup no
/// HandleTable: se for handle valido de String/Vec/Map/etc., renderiza
/// a Entry recursivamente. Caso contrario, formata como integer
/// (semantica padrao para Vec<i64> em RTS — slots sao i64 raw, nao
/// bits f64).
///
/// **NAO chamar dentro de um with_entry/with_two_entries lock** — esta
/// fn re-acessa o HandleTable e causaria deadlock no mesmo shard.
fn element_to_string(raw: i64) -> String {
    let h = raw as u64;
    // Heuristica: handles RTS comecam com gen >= 1, dando valores >= 2^48.
    // Valores menores sao quase certamente integers literais (TS [1,2,3]).
    // Esta optimization evita lookup desnecessario no HandleTable para
    // arrays numericos pequenos (caso comum).
    if h < (1u64 << 48) {
        return format_js_number(raw as f64);
    }
    enum Resolved {
        StringBytes(Vec<u8>),
        VecSlots(Vec<i64>),
        Object,
        Json(String),
        Other(&'static str),
        NotHandle,
    }
    let resolved = with_entry(h, |e| match e {
        Some(Entry::String(s)) => Resolved::StringBytes(s.clone()),
        Some(Entry::Vec(v)) => Resolved::VecSlots((**v).clone()),
        Some(Entry::Map(_)) => Resolved::Object,
        Some(Entry::Json(j)) => Resolved::Json(j.to_string()),
        Some(other) => Resolved::Other(entry_kind_name(other)),
        None => Resolved::NotHandle,
    });
    match resolved {
        Resolved::StringBytes(b) => String::from_utf8_lossy(&b).into_owned(),
        Resolved::VecSlots(slots) => {
            let inner: Vec<String> =
                slots.iter().map(|x| element_to_string(*x)).collect();
            inner.join(",")
        }
        Resolved::Object => "[object Object]".to_string(),
        Resolved::Json(s) => s,
        Resolved::Other(name) => format!("[object {}]", name),
        // Handle nao reconhecido — formata como integer.
        Resolved::NotHandle => format_js_number(raw as f64),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_STATIC(ptr: *const u8, len: i64) -> u64 {
    __RTS_FN_NS_GC_STRING_NEW(ptr, len)
}

/// Compares two string handles by content. Returns 1 if equal, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_EQ(a: u64, b: u64) -> i64 {
    if a == b {
        return 1;
    }
    with_two_entries(a, b, |ea, eb| match (ea, eb) {
        (Some(Entry::String(sa)), Some(Entry::String(sb))) => {
            if sa == sb { 1 } else { 0 }
        }
        _ => 0,
    })
}
