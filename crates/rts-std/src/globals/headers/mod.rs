//! `Headers` global class (#289) — Fetch API multimap case-insensitive de
//! header name -> lista de valores.
//!
//! DRAIN_MOTOR §11 (owner 2026-07-24): reimplementado como `#[rtse::class]`
//! (portado do hand-written `register_headers_class_spec` deletado em sessão
//! anterior — `git show HEAD:crates/rts-std/src/globals/headers/mod.rs` antes
//! do restore). Estado = `IndexMap<String, Vec<String>>` normal Rust, boxed via
//! `Entry::Rtse` (mesmo padrão de `UrlSearchParams`/`Point`).
//!
//! Tudo que o macro expressa (ctor 0/1-arg, append/set/get/has/delete, e os
//! F8 array-returns `Vec<String>` — getSetCookie/keys/values) é autoria direta
//! aqui. `entries()` (retorno aninhado `[string,string][]`, F8 não cobre nested
//! arrays ainda) e `forEach` (callback função-VALOR, sem suporte a `PolyValue`
//! como param) ficam hand-written no mesmo arquivo, lendo a MESMA struct via
//! `with_rtse` — igual ao resíduo de `search_params.rs`.

use indexmap::IndexMap;

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_entry, with_entry, with_rtse};

fn norm_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Multimap case-insensitive de header name -> lista de valores (ordem de
/// inserção preservada, como o `IndexMap` original).
#[rtse::class("Headers")]
#[derive(Clone)]
pub struct Headers {
    map: IndexMap<String, Vec<String>>,
}

impl Headers {
    /// Parseia um array de pares `[name, value]` (Entry::Vec de Entry::Vec de
    /// string handles) — a mesma forma que `new Headers(arr)` aceitava no
    /// builder hand-written original.
    fn pairs_from_array(arr_h: u64) -> Vec<(String, String)> {
        with_entry(arr_h, |e| match e {
            Some(Entry::Vec(v)) => v
                .iter()
                .filter_map(|&pair_raw| {
                    let pair_h = pair_raw as u64;
                    let pair_data: Option<(u64, u64)> = with_entry(pair_h, |pe| match pe {
                        Some(Entry::Vec(pv)) if pv.len() >= 2 => {
                            Some((pv[0] as u64, pv[1] as u64))
                        }
                        _ => None,
                    });
                    pair_data.and_then(|(kh, vh)| {
                        let k = with_entry(kh, |ke| match ke {
                            Some(Entry::String(b)) => {
                                Some(String::from_utf8_lossy(b).into_owned())
                            }
                            _ => None,
                        })?;
                        let v = with_entry(vh, |ve| match ve {
                            Some(Entry::String(b)) => {
                                Some(String::from_utf8_lossy(b).into_owned())
                            }
                            _ => None,
                        })?;
                        Some((k, v))
                    })
                })
                .collect(),
            _ => Vec::new(),
        })
    }
}

#[rtse::class("Headers")]
impl Headers {
    /// `new Headers()` — vazio.
    #[rtse::ctor]
    fn new() -> Self {
        Headers {
            map: IndexMap::new(),
        }
    }

    /// `new Headers(init)` — array de pares `[name, value]` (F2: overload por
    /// aridade — ctor de 0 args acima cobre o caso vazio).
    #[rtse::ctor]
    fn new_from(init: Handle) -> Self {
        let mut map: IndexMap<String, Vec<String>> = IndexMap::new();
        for (k, v) in Headers::pairs_from_array(init) {
            map.entry(norm_name(&k)).or_default().push(v);
        }
        Headers { map }
    }

    /// `h.append(name, value)`.
    #[rtse::method]
    fn append(self: &mut Headers, name: &str, value: &str) {
        self.map
            .entry(norm_name(name))
            .or_default()
            .push(value.to_string());
    }

    /// `h.set(name, value)` — substitui todos os valores existentes.
    #[rtse::method]
    fn set(self: &mut Headers, name: &str, value: &str) {
        self.map.insert(norm_name(name), vec![value.to_string()]);
    }

    /// `h.get(name)` — junta valores com ", ". `""` se não houver (a distinção
    /// `null` da spec exige retorno de String nullable, não suportado pelo
    /// macro ainda — mesmo gap documentado em `URLSearchParams::get`).
    #[rtse::method]
    fn get(self: &Headers, name: &str) -> String {
        self.map
            .get(&norm_name(name))
            .map(|vals| vals.join(", "))
            .unwrap_or_default()
    }

    /// `h.has(name)`.
    #[rtse::method]
    fn has(self: &Headers, name: &str) -> bool {
        self.map.contains_key(&norm_name(name))
    }

    /// `h.delete(name)`.
    #[rtse::method]
    fn delete(self: &mut Headers, name: &str) {
        self.map.shift_remove(&norm_name(name));
    }

    /// `h.getSetCookie()` — valores raw de "set-cookie", sem juntar (F8:
    /// `Vec<String>` return → array real).
    #[rtse::method]
    fn get_set_cookie(self: &Headers) -> Vec<String> {
        self.map.get("set-cookie").cloned().unwrap_or_default()
    }

    /// `h.keys()` — nomes normalizados, ordem de inserção (F8).
    #[rtse::method]
    fn keys(self: &Headers) -> Vec<String> {
        self.map.keys().cloned().collect()
    }

    /// `h.values()` — valores juntados por ", ", por nome (F8).
    #[rtse::method]
    fn values(self: &Headers) -> Vec<String> {
        self.map.values().map(|vs| vs.join(", ")).collect()
    }
}

// ---------------------------------------------------------------------------
// Resíduo hand-written (gaps do macro): retorno aninhado `[string,string][]`
// (F8 não cobre nested arrays) e callback função-VALOR (`forEach`, sem suporte
// a `PolyValue` como param). Ambos leem a MESMA struct `Headers` via
// `with_rtse` — uma única representação de storage, duas formas de autoria.
// ---------------------------------------------------------------------------

/// `h.entries()` — Vec de `[name_handle, joined_value_handle]`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_ENTRIES(h: u64) -> u64 {
    use rts_engine::heap::shapes::{handle_word_auto, string_word};
    let pairs: Vec<(String, String)> = with_rtse::<Headers, _>(h, |s| {
        s.map(|s| {
            s.map
                .iter()
                .map(|(k, vs)| (k.clone(), vs.join(", ")))
                .collect()
        })
        .unwrap_or_default()
    });
    let out: Vec<i64> = pairs
        .into_iter()
        .map(|(k, v)| {
            let kw = string_word(k.as_bytes()) as i64;
            let vw = string_word(v.as_bytes()) as i64;
            handle_word_auto(alloc_entry(Entry::Vec(Box::new(vec![kw, vw])))) as i64
        })
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// `h.forEach(callback)` — invoca `callback(value, key, headers)` por par, em
/// ordem de inserção (snapshot).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_FOR_EACH(h: u64, cb: u64) -> u64 {
    use rts_engine::heap::poly::POLY_UNDEFINED;
    use rts_engine::heap::shapes::{handle_word_auto, string_word};
    let pairs: Vec<(String, String)> = with_rtse::<Headers, _>(h, |s| {
        s.map(|s| {
            s.map
                .iter()
                .map(|(k, vs)| (k.clone(), vs.join(", ")))
                .collect()
        })
        .unwrap_or_default()
    });
    let self_word = handle_word_auto(h);
    for (k, v) in pairs {
        let vw = string_word(v.as_bytes());
        let kw = string_word(k.as_bytes());
        crate::gc_surface::__rtsadp_fn_invoke(cb, vw, kw, self_word, POLY_UNDEFINED, POLY_UNDEFINED);
    }
    POLY_UNDEFINED
}

/// Reabre a classe `Headers` gerada pelo macro (ctor/append/set/get/has/
/// delete/getSetCookie/keys/values) e anexa os membros residuais
/// hand-written (`entries`/`forEach`) — `ClassBuilder::done()` SUBSTITUI (não
/// funde) a entrada anterior, então relemos os membros do macro e os
/// re-emitimos junto dos residuais numa única `.done()` (mesmo padrão de
/// `url::register_urlsp_class_spec`).
pub fn register_headers_class_spec(e: &mut rts_engine::Engine) {
    // 1) Macro-authored ctor/instance methods.
    register(e);
    let macro_members: Vec<rts_engine::Member> = e
        .registry()
        .class("Headers")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    // 2) Reabre a classe, reproduz os membros do macro, e anexa os residuais.
    let mut cb = e
        .class("Headers")
        .doc("Headers — Fetch API multimap case-insensitive de header name -> valores.");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(rts_engine::Member {
        name: "entries".to_string(),
        kind: rts_engine::MemberKind::InstanceMethod,
        sig: rts_engine::Sig::new(vec![rts_engine::AbiType::Handle], rts_engine::AbiType::Handle),
        symbol: "__RTS_FN_GL_HEADERS_ENTRIES".to_string(),
        fn_ptr: rts_engine::FnPtr(__RTS_FN_GL_HEADERS_ENTRIES as *const u8),
        flags: rts_engine::MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: "entries(): IterableIterator<[string, string]>".to_string(),
        doc: "h.entries() — Vec de [name_handle, joined_value_handle].".to_string(),
        pure: true,
        emit: None,
    })
    .member(rts_engine::Member {
        name: "forEach".to_string(),
        kind: rts_engine::MemberKind::InstanceMethod,
        sig: rts_engine::Sig::new(
            vec![rts_engine::AbiType::Handle, rts_engine::AbiType::PolyValue],
            rts_engine::AbiType::Handle,
        ),
        symbol: "__RTS_FN_GL_HEADERS_FOR_EACH".to_string(),
        fn_ptr: rts_engine::FnPtr(__RTS_FN_GL_HEADERS_FOR_EACH as *const u8),
        flags: rts_engine::MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature:
            "forEach(callback: (value: string, key: string, headers: Headers) => void): void"
                .to_string(),
        doc: "h.forEach(callback) — invoca callback(value, key, self) por par (snapshot)."
            .to_string(),
        pure: false,
        emit: None,
    })
    .done();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::alloc_rtse;

    fn h(pairs: &[(&str, &str)]) -> u64 {
        let mut map: IndexMap<String, Vec<String>> = IndexMap::new();
        for (k, v) in pairs {
            map.entry(norm_name(k)).or_default().push(v.to_string());
        }
        alloc_rtse("Headers", Headers { map })
    }

    #[test]
    fn get_joins_values_with_comma_space() {
        let handle = h(&[("a", "1"), ("a", "2")]);
        let vh = __RTS_FN_GL_HEADERS_get(handle, "a".as_ptr() as i64, 1);
        let s = with_entry(vh, |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        });
        assert_eq!(s, "1, 2");
    }

    #[test]
    fn has_and_delete() {
        let handle = h(&[("a", "1")]);
        assert!(__RTS_FN_GL_HEADERS_has(handle, "A".as_ptr() as i64, 1) != 0);
        __RTS_FN_GL_HEADERS_delete(handle, "a".as_ptr() as i64, 1);
        assert_eq!(__RTS_FN_GL_HEADERS_has(handle, "a".as_ptr() as i64, 1), 0);
    }

    #[test]
    fn set_replaces_all() {
        let handle = h(&[("a", "1"), ("a", "2")]);
        __RTS_FN_GL_HEADERS_set(handle, "a".as_ptr() as i64, 1, "9".as_ptr() as i64, 1);
        with_rtse::<Headers, _>(handle, |s| {
            assert_eq!(s.unwrap().map.get("a"), Some(&vec!["9".to_string()]));
        });
    }

    #[test]
    fn append_preserves_dupes() {
        let handle = h(&[]);
        __RTS_FN_GL_HEADERS_append(handle, "a".as_ptr() as i64, 1, "1".as_ptr() as i64, 1);
        __RTS_FN_GL_HEADERS_append(handle, "a".as_ptr() as i64, 1, "2".as_ptr() as i64, 1);
        with_rtse::<Headers, _>(handle, |s| {
            assert_eq!(
                s.unwrap().map.get("a"),
                Some(&vec!["1".to_string(), "2".to_string()])
            );
        });
    }

    #[test]
    fn entries_returns_pairs() {
        let handle = h(&[("a", "1"), ("b", "2")]);
        let out = __RTS_FN_GL_HEADERS_ENTRIES(handle);
        let count = with_entry(out, |e| match e {
            Some(Entry::Vec(v)) => v.len(),
            _ => 0,
        });
        assert_eq!(count, 2);
    }
}
