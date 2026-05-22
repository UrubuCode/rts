//! String-producing ABI for the GC namespace.

use super::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut, with_two_entries};

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
    // (#1023) StringBox unwrap antes do dispatch — recurse uma vez.
    let unwrap: Option<u64> = with_entry(handle, |entry| match entry {
        Some(Entry::StringBox(h)) => Some(*h),
        _ => None,
    });
    if let Some(inner) = unwrap {
        return __RTS_FN_NS_GC_HANDLE_LEN(inner);
    }
    with_entry(handle, |entry| match entry {
        // JS spec: String.length = number of UTF-16 code units, not bytes.
        Some(Entry::String(b)) => {
            match std::str::from_utf8(b) {
                Ok(s) => s.encode_utf16().count() as i64,
                Err(_) => b.len() as i64,
            }
        }
        Some(Entry::Map(m)) => {
            // Array-like maps (e.g. regex match results) store "length" key.
            if let Some(&v) = m.get("length") {
                v
            } else {
                m.len() as i64
            }
        }
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
    // Raw i64 → decimal string. Esta fn eh primitiva exposta via
    // `gc.string_from_i64` (ex: num.checked_* retornando i64::MIN
    // de overflow precisa virar "-9223372036854775808"). Sentinelas
    // JS (MIN..MIN+4) sao tratadas em coerce_to_handle/template via
    // TPL_COERCE_AUTO (e via STRING_FROM_I64_TPL abaixo).
    alloc_entry(Entry::String(value.to_string().into_bytes()))
}

/// (cross-runtime) Variante de STRING_FROM_I64 usada em template
/// literal/coerce_to_handle: filtra sentinelas JS antes de formatar
/// para que `${undefined}` -> "undefined" em vez de raw i64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_FROM_I64_TPL(value: i64) -> u64 {
    if value == i64::MIN     { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 { return alloc_entry(Entry::String(b"null".to_vec())); }
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
    // (cross-runtime sentinelas) Array literal sentinelas viram strings:
    //   MIN     = false, MIN+1 = true, MIN+2 = undefined, MIN+3 = null,
    //   MIN+4   = sparse hole (renderiza vazio em join, mas como
    //   "undefined" quando coergido isoladamente, ex: forOf var).
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    // (#573) value=0 com tipo Handle representa `null` em RTS (e
    // tambem o sentinel de handle invalido). Em JS, console.log(null)
    // -> "null", consistente com Array.prototype.join e String(null).
    if value == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    let h = value as u64;
    let snap = snapshot_entry(h);
    match snap {
        EntrySnap::Str(_) => h, // passthrough do handle original
        EntrySnap::None => {
            // Nao eh handle valido — interpreta como i64 raw OU bits f64.
            // (cross-runtime #49) `add.bind(null, 5)` chamado retorna i64
            // que sao bits de f64 (INVOKE_AUTO faz f64_to_i64). Se o valor
            // esta no range tipico de bits f64 (mas seria abaixo de MIN_SAFE_INTEGER
            // ou acima de MAX_SAFE_INTEGER como inteiro), interpreta como f64 bits.
            // Range: bits de f64 com magnitude razoavel (1e-300 a 1e300) tem
            // expoente IEEE-754 entre cerca de 0x000 e 0x7FE; portanto bit 63
            // (sign) + bits 62..52 (exponent) tem valores variados.
            // Heuristica conservadora: se |value| > 2^53 (MAX_SAFE_INTEGER) e
            // f64::from_bits(u64) produz numero finito nao-NaN razoavel, usa f64.
            const MAX_SAFE: i64 = (1i64 << 53) - 1; // 2^53 - 1
            if value > MAX_SAFE || value < -MAX_SAFE {
                let f = f64::from_bits(value as u64);
                if f.is_finite() && !f.is_nan() && f.abs() < 1e16 && f != 0.0 {
                    // Provavelmente bits f64 de result de bind/invoke.
                    return alloc_entry(Entry::String(format_js_number(f).into_bytes()));
                }
            }
            alloc_entry(Entry::String(value.to_string().into_bytes()))
        }
        other => {
            let bytes = snapshot_to_bytes(&other);
            alloc_entry(Entry::String(bytes))
        }
    }
}

/// Variante de `__RTS_FN_RT_TPL_COERCE_AUTO` para slots de Vec onde
/// `0` eh um numero literal (nao handle inválido / null). Identico a
/// AUTO exceto que value=0 retorna "0" (nao "null").
///
/// Usado pelo codegen no fallback de `arr[i]` quando obj_handle e' Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TPL_COERCE_VEC_SLOT(value: i64) -> u64 {
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    // Slot Vec: value=0 eh literal `0`, nao null.
    let h = value as u64;
    let snap = snapshot_entry(h);
    match snap {
        EntrySnap::Str(_) => h,
        EntrySnap::None => {
            const MAX_SAFE: i64 = (1i64 << 53) - 1;
            if value > MAX_SAFE || value < -MAX_SAFE {
                let f = f64::from_bits(value as u64);
                if f.is_finite() && !f.is_nan() && f.abs() < 1e16 && f != 0.0 {
                    return alloc_entry(Entry::String(format_js_number(f).into_bytes()));
                }
            }
            alloc_entry(Entry::String(value.to_string().into_bytes()))
        }
        other => {
            let bytes = snapshot_to_bytes(&other);
            alloc_entry(Entry::String(bytes))
        }
    }
}

/// JS-spec truthiness para Handle ambiguo:
/// - 0/null/undefined -> falsy (0)
/// - Entry::String com bytes vazios -> falsy (0)
/// - Entry::Vec/Map vazios -> truthy (objetos sao sempre truthy em JS)
/// - Outros handles validos -> truthy (1)
/// - Handle invalido (nao existe na tabela) e value != 0 -> truthy
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TRUTHY(value: i64) -> i64 {
    if value == 0 {
        return 0;
    }
    // Sentinela bool em slot de Vec/Map (codegen empacota false como
    // i64::MIN, true como i64::MIN+1). Trata como JS bool.
    if value == i64::MIN {
        return 0;
    }
    if value == i64::MIN + 1 {
        return 1;
    }
    // (cross-runtime #223) Sentinelas undefined/null/hole sao falsy
    // em JS, equivalentes ao 0 do RTS para fins de truthy check.
    if value == i64::MIN + 2 || value == i64::MIN + 3 || value == i64::MIN + 4 {
        return 0;
    }
    let h = value as u64;
    let snap = with_entry(h, |entry| match entry {
        Some(Entry::String(b)) => {
            // `undefined` (handle de string literal "undefined") eh falsy.
            // String vazia tambem.
            if b.is_empty() || b.as_slice() == b"undefined" {
                Some(false)
            } else {
                Some(true)
            }
        }
        Some(_) => Some(true),
        None => None,
    });
    match snap {
        Some(true) => 1,
        Some(false) => 0,
        None => 1,
    }
}

/// Spread universal: copia elementos de `src` para o Vec `dst`.
/// Detecta tipo de Entry e itera apropriadamente:
/// - Entry::Vec -> push de cada slot
/// - Entry::String -> push de cada char (handle de string char)
/// - Entry::Map -> push de cada value (ordem do IndexMap)
/// - outros -> no-op
///
/// Usado por `[...x]` no codegen quando `x` pode ser string/array.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_SPREAD_INTO_VEC(dst: u64, src: u64) {
    if dst == 0 || src == 0 {
        return;
    }
    enum Snap {
        Vec(Vec<i64>),
        Str(Vec<u8>),
        Map(Vec<i64>),
        Empty,
    }
    let snap = with_entry(src, |entry| match entry {
        Some(Entry::Vec(slots)) => Snap::Vec(slots.as_ref().clone()),
        Some(Entry::String(b)) => Snap::Str(b.clone()),
        Some(Entry::Map(m)) => Snap::Map(m.values().copied().collect()),
        _ => Snap::Empty,
    });
    match snap {
        Snap::Vec(items) => {
            for v in items {
                push_vec_slot(dst, v);
            }
        }
        Snap::Str(bytes) => {
            // Iter por chars Unicode (string spread JS itera codepoints).
            let s = String::from_utf8_lossy(&bytes);
            for ch in s.chars() {
                let mut buf = [0u8; 4];
                let ch_bytes = ch.encode_utf8(&mut buf).as_bytes().to_vec();
                let h = alloc_entry(Entry::String(ch_bytes));
                push_vec_slot(dst, h as i64);
            }
        }
        Snap::Map(vals) => {
            for v in vals {
                push_vec_slot(dst, v);
            }
        }
        Snap::Empty => {}
    }
}

fn push_vec_slot(dst: u64, value: i64) {
    with_entry_mut(dst, |entry| {
        if let Some(Entry::Vec(slots)) = entry {
            slots.push(value);
        }
    });
}

/// `.length` universal para handles ambiguous (any/var_member_call):
/// detecta Entry::Vec/Map/String e retorna size; retorna -1 quando
/// handle invalido ou tipo sem length. Usado pelo codegen quando
/// recv de `.length` tem tipo dinamico (e.g. `JSON.parse(s).length`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_UNIVERSAL_LENGTH(handle: u64) -> i64 {
    if handle == 0 {
        return -1;
    }
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(slots)) => slots.len() as i64,
        Some(Entry::Map(map)) => map.len() as i64,
        Some(Entry::String(b)) => b.len() as i64,
        Some(Entry::Json(v)) => match v.as_ref() {
            serde_json::Value::Array(a) => a.len() as i64,
            serde_json::Value::Object(o) => o.len() as i64,
            serde_json::Value::String(s) => s.len() as i64,
            _ => -1,
        },
        _ => -1,
    })
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
    // (cross-runtime #110) Sentinelas JS:
    //   MIN     = false  -> "boolean"
    //   MIN+1   = true   -> "boolean"
    //   MIN+2   = undef  -> "undefined"
    //   MIN+3   = null   -> "object"
    //   MIN+4   = hole   -> "undefined"
    let val_i64 = handle as i64;
    if val_i64 == i64::MIN || val_i64 == i64::MIN + 1 {
        return alloc_entry(Entry::String(b"boolean".to_vec()));
    }
    if val_i64 == i64::MIN + 2 || val_i64 == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if val_i64 == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"object".to_vec()));
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Symbol { .. }) => "symbol",
        Some(Entry::Function(_)) => "function",
        Some(Entry::String(_)) => "string",
        Some(Entry::BooleanBox(_)) => "object",
        Some(Entry::StringBox(_)) => "object",
        Some(Entry::Vec(_)) | Some(Entry::Map(_)) | Some(Entry::Buffer(_))
        | Some(Entry::Json(_)) | Some(Entry::DateMs(_))
        | Some(Entry::PromiseAsync(_)) | Some(Entry::Promise(_)) => "object",
        Some(_) => "object",
        None => {
            // (cross-runtime #110) Handle invalido <2^48 eh numero raw
            // (slot de arr/map int storage); retorna "number". Acima de
            // 2^48 eh provavelmente handle stale — historic "string" mantido.
            if handle < (1u64 << 48) {
                "number"
            } else {
                "string"
            }
        }
    });
    alloc_entry(Entry::String(kind.as_bytes().to_vec()))
}

/// `<handle>.toString()` runtime dispatch baseado no tipo da Entry.
/// Cobre casos que o codegen nao despacha estaticamente:
/// - Entry::Symbol -> "Symbol(desc)" / "Symbol()"
/// - Entry::Function -> "function name() { [native code] }"
/// - Entry::String -> handle propria (passthrough)
/// - Entry::Vec -> "1,2,3" (Array.prototype.toString)
/// - Entry::Map -> "[object Object]"
/// - Outros -> "[object Kind]"
///
/// Handle invalido retorna "" (nao crash).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TO_STRING_HANDLE(handle: u64) -> u64 {
    let snap = snapshot_entry(handle);
    match snap {
        EntrySnap::Str(_) => handle, // passthrough
        _ => {
            // Snapshot nao cobre Symbol/Function explicitamente — vamos
            // detectar via segundo lookup direto.
            let special = with_entry(handle, |e| match e {
                Some(Entry::Symbol { description }) => match description {
                    Some(d) => Some(format!("Symbol({})", d)),
                    None => Some("Symbol()".to_string()),
                },
                Some(Entry::Function(d)) => {
                    let name = if d.name.is_empty() { "anonymous" } else { &*d.name };
                    Some(format!("function {}() {{ [native code] }}", name))
                }
                _ => None,
            });
            match special {
                Some(s) => alloc_entry(Entry::String(s.into_bytes())),
                None => {
                    let bytes = snapshot_to_bytes(&snap);
                    alloc_entry(Entry::String(bytes))
                }
            }
        }
    }
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
    // Handle 0 = JS null — displayed as "null" in string context (JS spec: null + "" = "null").
    let snap_a = if a == 0 { EntrySnap::Str(b"null".to_vec()) } else { snapshot_entry(a) };
    let snap_b = if b == 0 { EntrySnap::Str(b"null".to_vec()) } else { snapshot_entry(b) };
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
    // Sentinela bool (mesma logica de inspect_slot).
    if raw == i64::MIN {
        return "false".to_string();
    }
    if raw == i64::MIN + 1 {
        return "true".to_string();
    }
    // (cross-runtime #51) undefined/null/sparse hole em
    // Array.prototype.toString viram "" (JS spec). Sem isso, o raw
    // i64::MIN+2/+3/+4 caia em format_js_number e gerava lixo
    // tipo "-9223372036854776000".
    if raw == i64::MIN + 2 || raw == i64::MIN + 3 || raw == i64::MIN + 4 {
        return String::new();
    }
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

/// Lexicographic comparison of two string handles by content (memcmp + length).
/// Returns -1 if a < b, 0 if equal, 1 if a > b. Matches JS string ordering.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_STRING_CMP(a: u64, b: u64) -> i64 {
    if a == b {
        return 0;
    }
    with_two_entries(a, b, |ea, eb| match (ea, eb) {
        (Some(Entry::String(sa)), Some(Entry::String(sb))) => {
            match sa.as_slice().cmp(sb.as_slice()) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            }
        }
        _ => 0,
    })
}

/// Strict equality (===) entre dois i64 quando pelo menos um eh
/// ambiguo (resultado de vec_get/map_get sem tipo declarado). Cada
/// lado eh interpretado dinamicamente:
/// - handle valido (Entry::String/Vec/Map/...) -> objeto JS
/// - i64 fora do range de handle -> numero JS
///
/// Regras JS strict equality:
/// - Mesmo tipo + mesmo valor -> 1
/// - Tipos diferentes -> 0
/// - Dois handles String iguais em conteudo -> 1
/// - Dois handles do mesmo objeto (Vec/Map) -> 1 (identidade)
/// - Numero vs handle -> 0
///
/// Usado quando codegen detecta uma das pontas em `var_member_call_values`
/// e a comparacao seria short-circuit por strict-kind.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_STRICT_EQ_AMBIG(a: i64, b: i64) -> i64 {
    // Mesmo bit pattern -> sempre igual (cobre handles identicos e
    // numeros iguais).
    if a == b {
        return 1;
    }
    let snap_a = if a == 0 { EntrySnap::None } else { snapshot_entry(a as u64) };
    let snap_b = if b == 0 { EntrySnap::None } else { snapshot_entry(b as u64) };
    match (&snap_a, &snap_b) {
        (EntrySnap::Str(sa), EntrySnap::Str(sb)) => {
            if sa == sb { 1 } else { 0 }
        }
        // Tipos compostos diferentes ou um lado handle e o outro numero: 0.
        _ => 0,
    }
}

/// `Object.prototype.toString.call(x)` — retorna "[object Type]".
///
/// Tag eh fornecida pelo codegen baseado no tipo estatico:
///   0 = ambiguo (inspect runtime entry); decide via Entry.
///   1 = Number, 2 = String, 3 = Boolean, 4 = Null, 5 = Undefined,
///   6 = Function
///
/// Para tag=0, inspeciona Entry: Vec->Array, Map->Object, Date->Date,
/// Regex->RegExp, Function->Function, etc.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OBJECT_TO_STRING(value: i64, tag: i64) -> u64 {
    let kind: &str = match tag {
        1 => "Number",
        2 => "String",
        3 => "Boolean",
        4 => "Null",
        5 => "Undefined",
        6 => "Function",
        _ => {
            // tag=0: detecta via Entry
            if value == 0 {
                "Null"
            } else {
                let h = value as u64;
                with_entry(h, |e| match e {
                    Some(Entry::Vec(_)) => "Array",
                    Some(Entry::Map(_)) => {
                        if crate::namespaces::collections::map::handle_is_set_kind(h) {
                            "Set"
                        } else if crate::namespaces::collections::map::handle_is_map_kind(h) {
                            "Map"
                        } else {
                            "Object"
                        }
                    }
                    Some(Entry::DateMs(_)) => "Date",
                    Some(Entry::Regex(_)) => "RegExp",
                    Some(Entry::Function(_)) => "Function",
                    Some(Entry::WeakMap(_)) => "WeakMap",
                    Some(Entry::String(b)) => {
                        if b.as_slice() == b"undefined" {
                            "Undefined"
                        } else {
                            "String"
                        }
                    }
                    Some(_) => "Object",
                    None => "Null",
                })
            }
        }
    };
    let s = format!("[object {}]", kind);
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Inspect/pretty-print no estilo Node/Bun para `console.log` — arrays
/// viram `[ 1, 2, 'a' ]`, objetos `{ k: v }`, strings TOP-LEVEL sem
/// aspas (strings DENTRO de array/object recebem aspas simples).
///
/// String top-level retorna o handle original (passthrough), igual ao
/// TPL_COERCE_AUTO, preservando `console.log("oi")` -> `oi`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_INSPECT(value: i64) -> u64 {
    // Sentinelas: false/true/undefined/null/sparse-hole (consistente
    // com TPL_COERCE_AUTO). Sem isso console.log(opt_chain_undef) -> "null".
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    if value == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    let h = value as u64;
    let snap = snapshot_entry(h);
    match snap {
        EntrySnap::Str(_) => h,
        EntrySnap::None => {
            alloc_entry(Entry::String(value.to_string().into_bytes()))
        }
        _ => {
            let s = inspect_handle(h, 0);
            alloc_entry(Entry::String(s.into_bytes()))
        }
    }
}

const INSPECT_MAX_DEPTH: usize = 6;

fn inspect_handle(h: u64, depth: usize) -> String {
    if depth >= INSPECT_MAX_DEPTH {
        return "[Object]".to_string();
    }
    enum R {
        Str(Vec<u8>),
        Vec(Vec<i64>),
        Map(Vec<(Vec<u8>, i64)>),
        Json(String),
        Other(&'static str),
        None,
    }
    let r = with_entry(h, |e| match e {
        Some(Entry::String(s)) => R::Str(s.clone()),
        Some(Entry::Vec(v)) => R::Vec((**v).clone()),
        Some(Entry::Map(m)) => R::Map(
            m.iter().map(|(k, v)| (k.as_bytes().to_vec(), *v)).collect(),
        ),
        Some(Entry::Json(j)) => R::Json(j.to_string()),
        Some(other) => R::Other(entry_kind_name(other)),
        None => R::None,
    });
    match r {
        R::Str(b) => format!("'{}'", String::from_utf8_lossy(&b)),
        R::Vec(slots) => {
            if slots.is_empty() {
                return "[]".to_string();
            }
            let parts: Vec<String> =
                slots.iter().map(|x| inspect_slot(*x, depth + 1)).collect();
            format!("[ {} ]", parts.join(", "))
        }
        R::Map(entries) => {
            if entries.is_empty() {
                return "{}".to_string();
            }
            let parts: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}: {}",
                        String::from_utf8_lossy(k),
                        inspect_slot(*v, depth + 1)
                    )
                })
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }
        R::Json(s) => s,
        R::Other(name) => format!("[object {}]", name),
        R::None => String::new(),
    }
}

/// Slot cru de Vec/Map: < 2^48 = numero JS; >= 2^48 e handle valido =
/// inspect recursivo; senao numero.
fn inspect_slot(raw: i64, depth: usize) -> String {
    // Sentinela bool em slot de Vec/Map (gerado pelo codegen).
    if raw == i64::MIN {
        return "false".to_string();
    }
    if raw == i64::MIN + 1 {
        return "true".to_string();
    }
    let h = raw as u64;
    if h < (1u64 << 48) {
        return format_js_number(raw as f64);
    }
    let exists = with_entry(h, |e| e.is_some());
    if !exists {
        return format_js_number(raw as f64);
    }
    inspect_handle(h, depth)
}
