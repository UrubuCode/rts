//! Regex-backed `String` method helpers (`match`/`matchAll`/`search` +
//! replace-with-regex machinery) plus the string-vs-regex `_AUTO` dispatchers the
//! engine's lowering calls for `String.prototype.{match,search,matchAll}`.
//!
//! These are NOT the `rts:string` namespace anymore — that namespace was drained.
//! The regex helpers are plain `pub fn` (called from the `__rtsadp_str_*_auto`
//! trampolines below + the adapter's `re.exec`); the three `__rtsadp_str_*_auto`
//! trampolines are codegen-owned extern symbols the engine emits for a
//! string-OR-regex pattern argument (runtime `Entry::Regex` probe).
//!
//! `StrPtr` at the ABI boundary delivers (ptr, len); the helpers rebuild `&str`.

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract — UTF-8 valido cobrindo `len` bytes.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// (#208) `str.match(pattern)` — primeiro match, retorna handle de string
/// com o conteudo encontrado, ou 0 se nao acha. Pattern e' string regex.
pub fn match_str(s_ptr: *const u8, s_len: i64, p_ptr: *const u8, p_len: i64) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, Entry};
    let (Some(s), Some(p)) = (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) else {
        return 0;
    };
    let Ok(rx) = regex::Regex::new(p) else {
        return 0;
    };
    match rx.find(s) {
        Some(m) => alloc_entry(Entry::String(m.as_str().as_bytes().to_vec())),
        None => 0,
    }
}

/// `str.match(regex_handle)` — variante que aceita handle de
/// Entry::Regex (de literal /pat/ ou new RegExp). Se nao for regex
/// valido, retorna 0.
/// - Sem flag `g`: retorna Vec [fullMatch, ...grupos] (ou 0 se nao acha).
/// - Com flag `g`: retorna Vec flat de todos os fullMatches.
pub fn match_regex(s_ptr: *const u8, s_len: i64, regex_handle: u64) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return 0;
    };
    let s_owned = s.to_string();
    enum MatchResultExt {
        First {
            items: Vec<Option<Vec<u8>>>,
            index: usize,
            names: Vec<Option<String>>,
        },
        All(Vec<Vec<u8>>),
        None,
    }
    let mr: MatchResultExt = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            if rts_rx.global {
                let all: Vec<Vec<u8>> = rts_rx
                    .engine
                    .find_all(&s_owned)
                    .into_iter()
                    .map(|m| m.text.into_bytes())
                    .collect();
                MatchResultExt::All(all)
            } else {
                match rts_rx.engine.captures(&s_owned) {
                    Some(caps) => {
                        let index = caps
                            .groups
                            .first()
                            .and_then(|o| o.as_ref())
                            .map(|m| m.start)
                            .unwrap_or(0);
                        let items: Vec<Option<Vec<u8>>> = caps
                            .groups
                            .iter()
                            .map(|o| o.as_ref().map(|m| m.text.clone().into_bytes()))
                            .collect();
                        let names = rts_rx.engine.capture_names();
                        MatchResultExt::First {
                            items,
                            index,
                            names,
                        }
                    }
                    None => MatchResultExt::None,
                }
            }
        }
        _ => MatchResultExt::None,
    });
    match mr {
        MatchResultExt::First {
            items,
            index,
            names,
        } => {
            if items.is_empty() {
                return 0;
            }
            // Objeto SHAPED com chaves "0","1",... + "length" + "index" +
            // "input" + "groups" (slots = PolyValue words, motor novo).
            use rts_engine::heap::poly::POLY_UNDEFINED;
            use rts_engine::heap::shapes::{
                alloc_shaped_object_owned, handle_word_auto, legacy_i64_to_word, string_word,
            };
            let mut keys: Vec<String> = Vec::with_capacity(items.len() + 4);
            let mut vals: Vec<i64> = Vec::with_capacity(items.len() + 4);
            for (i, opt) in items.iter().enumerate() {
                keys.push(i.to_string());
                vals.push(match opt {
                    Some(bytes) => string_word(bytes) as i64,
                    None => POLY_UNDEFINED as i64,
                });
            }
            keys.push("length".to_string());
            vals.push(legacy_i64_to_word(items.len() as i64) as i64);
            keys.push("index".to_string());
            vals.push(legacy_i64_to_word(index as i64) as i64);
            keys.push("input".to_string());
            vals.push(string_word(s_owned.as_bytes()) as i64);
            // (#1086) groups: objeto shaped <name, string|undefined> ou undefined.
            let any_named = names.iter().any(|n| n.is_some());
            let groups_v: i64 = if !any_named {
                POLY_UNDEFINED as i64
            } else {
                let mut gkeys: Vec<String> = Vec::new();
                let mut gvals: Vec<i64> = Vec::new();
                for (i, n) in names.iter().enumerate() {
                    if let Some(name) = n {
                        gkeys.push(name.clone());
                        gvals.push(match items.get(i).and_then(|o| o.as_ref()) {
                            Some(bytes) => string_word(bytes) as i64,
                            None => POLY_UNDEFINED as i64,
                        });
                    }
                }
                handle_word_auto(alloc_shaped_object_owned(gkeys, &gvals)) as i64
            };
            keys.push("groups".to_string());
            vals.push(groups_v);
            alloc_shaped_object_owned(keys, &vals)
        }
        MatchResultExt::All(items) => {
            if items.is_empty() {
                return 0;
            }
            // Elementos como string WORDS (motor novo) — não handles crus.
            let slots: Vec<i64> = items
                .into_iter()
                .map(|bytes| rts_engine::heap::shapes::string_word(&bytes) as i64)
                .collect();
            alloc_entry(Entry::Vec(Box::new(slots)))
        }
        MatchResultExt::None => 0,
    }
}

/// `str.search(regex_handle)` — index do primeiro match, -1 se nao
/// acha. Aceita Entry::Regex direto.
pub fn search_regex(s_ptr: *const u8, s_len: i64, regex_handle: u64) -> i64 {
    use rts_engine::heap::handles::{with_entry, Entry};
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return -1;
    };
    let s_owned = s.to_string();
    with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rx)) => rx
            .engine
            .find(&s_owned)
            .map(|m| m.start as i64)
            .unwrap_or(-1),
        _ => -1,
    })
}

/// (#208) `str.search(pattern)` — index do primeiro match, ou -1.
/// Pattern e' string regex.
pub fn search_str(s_ptr: *const u8, s_len: i64, p_ptr: *const u8, p_len: i64) -> i64 {
    let (Some(s), Some(p)) = (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) else {
        return -1;
    };
    let Ok(rx) = regex::Regex::new(p) else {
        return -1;
    };
    rx.find(s).map(|m| m.start() as i64).unwrap_or(-1)
}

/// (#208) `str.matchAll(pattern)` — retorna handle de Vec<u64> com handles
/// de strings, um por match. Em JS retorna iterator de RegExpExecArray; em
/// RTS v0 retorna Vec eager (cada elemento e' o conteudo do match).
pub fn match_all_str(s_ptr: *const u8, s_len: i64, p_ptr: *const u8, p_len: i64) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, Entry};
    let empty_vec = || alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let (Some(s), Some(p)) = (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) else {
        return empty_vec();
    };
    let Ok(rx) = regex::Regex::new(p) else {
        return empty_vec();
    };
    let mut handles: Vec<i64> = Vec::new();
    for m in rx.find_iter(s) {
        // String WORD (motor novo), não handle cru.
        handles.push(rts_engine::heap::shapes::string_word(m.as_str().as_bytes()) as i64);
    }
    alloc_entry(Entry::Vec(Box::new(handles)))
}

/// `str.matchAll(regex_handle)` — variante que aceita handle Entry::Regex.
/// Retorna Vec de Maps (JS spec): cada match e' um Map com slots numericos
/// "0".."N", "length", "index", "input", "groups" (Map de named captures
/// ou undefined sentinel). Compativel com `m[0]`/`m.groups.name`/`m.index`.
/// Também consumido pelo adapter `re.exec`.
pub fn match_all_regex(s_ptr: *const u8, s_len: i64, regex_handle: u64) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
    let empty_vec = || alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return empty_vec();
    };
    let s_owned = s.to_string();
    type MatchInfo = (
        Vec<Option<String>>,
        Vec<(String, Option<String>)>,
        usize,
        Option<Vec<Option<(usize, usize)>>>,
        Vec<Option<String>>, // names em paralelo aos indices, para .groups
    );
    let infos: Vec<MatchInfo> = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            let has_indices = rts_rx.flags.contains('d');
            let names: Vec<Option<String>> = rts_rx.engine.capture_names();
            rts_rx
                .engine
                .captures_all(&s_owned)
                .into_iter()
                .map(|caps| {
                    let groups: Vec<Option<String>> = caps
                        .groups
                        .iter()
                        .map(|o| o.as_ref().map(|m| m.text.clone()))
                        .collect();
                    let named: Vec<(String, Option<String>)> = names
                        .iter()
                        .enumerate()
                        .filter_map(|(i, n)| {
                            n.as_ref().map(|name| {
                                (
                                    name.clone(),
                                    caps.groups
                                        .get(i)
                                        .and_then(|o| o.as_ref())
                                        .map(|m| m.text.clone()),
                                )
                            })
                        })
                        .collect();
                    let idx = caps
                        .groups
                        .first()
                        .and_then(|o| o.as_ref())
                        .map(|m| m.start)
                        .unwrap_or(0);
                    let indices = if has_indices {
                        Some(
                            caps.groups
                                .iter()
                                .map(|o| o.as_ref().map(|m| (m.start, m.end)))
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        None
                    };
                    (groups, named, idx, indices, names.clone())
                })
                .collect()
        }
        _ => Vec::new(),
    });
    let outer: Vec<i64> = infos
        .into_iter()
        .map(|(groups, named, idx, indices_opt, names)| {
            // Linha SHAPED (motor novo): "0".."n" + length/index/input/groups/
            // indices, slots = PolyValue words; o elemento do Vec externo é o
            // OBJECT word da linha.
            use rts_engine::heap::poly::POLY_UNDEFINED;
            use rts_engine::heap::shapes::{
                alloc_shaped_object_owned, handle_word_auto, legacy_i64_to_word, string_word,
            };
            let word_pair = |s: usize, e: usize| -> i64 {
                use rts_engine::heap::shapes::legacy_i64_to_word as w;
                let pair = alloc_entry(Entry::Vec(Box::new(vec![
                    w(s as i64) as i64,
                    w(e as i64) as i64,
                ])));
                handle_word_auto(pair) as i64
            };
            let mut keys: Vec<String> = Vec::with_capacity(groups.len() + 5);
            let mut vals: Vec<i64> = Vec::with_capacity(groups.len() + 5);
            for (i, opt) in groups.iter().enumerate() {
                keys.push(i.to_string());
                vals.push(match opt {
                    Some(s) => string_word(s.as_bytes()) as i64,
                    None => POLY_UNDEFINED as i64,
                });
            }
            keys.push("length".to_string());
            vals.push(legacy_i64_to_word(groups.len() as i64) as i64);
            keys.push("index".to_string());
            vals.push(legacy_i64_to_word(idx as i64) as i64);
            keys.push("input".to_string());
            vals.push(string_word(s_owned.as_bytes()) as i64);
            let groups_v = if named.is_empty() {
                POLY_UNDEFINED as i64
            } else {
                let mut gkeys: Vec<String> = Vec::new();
                let mut gvals: Vec<i64> = Vec::new();
                for (n, opt) in named {
                    gkeys.push(n);
                    gvals.push(match opt {
                        Some(s) => string_word(s.as_bytes()) as i64,
                        None => POLY_UNDEFINED as i64,
                    });
                }
                handle_word_auto(alloc_shaped_object_owned(gkeys, &gvals)) as i64
            };
            keys.push("groups".to_string());
            vals.push(groups_v);
            // (#1087) indices quando flag 'd' setada.
            let indices_v = match indices_opt {
                Some(vec) => {
                    let mut ikeys: Vec<String> = Vec::new();
                    let mut ivals: Vec<i64> = Vec::new();
                    for (i, opt) in vec.iter().enumerate() {
                        ikeys.push(i.to_string());
                        ivals.push(match opt {
                            Some((s, e)) => word_pair(*s, *e),
                            None => POLY_UNDEFINED as i64,
                        });
                    }
                    ikeys.push("length".to_string());
                    ivals.push(legacy_i64_to_word(vec.len() as i64) as i64);
                    let any_named = names.iter().any(|n| n.is_some());
                    let g_v: i64 = if any_named {
                        let mut gkeys: Vec<String> = Vec::new();
                        let mut gvals: Vec<i64> = Vec::new();
                        for (i, name_opt) in names.iter().enumerate() {
                            if let Some(name) = name_opt {
                                gkeys.push(name.clone());
                                gvals.push(match vec.get(i).and_then(|o| *o) {
                                    Some((s, e)) => word_pair(s, e),
                                    None => POLY_UNDEFINED as i64,
                                });
                            }
                        }
                        handle_word_auto(alloc_shaped_object_owned(gkeys, &gvals)) as i64
                    } else {
                        POLY_UNDEFINED as i64
                    };
                    ikeys.push("groups".to_string());
                    ivals.push(g_v);
                    handle_word_auto(alloc_shaped_object_owned(ikeys, &ivals)) as i64
                }
                None => POLY_UNDEFINED as i64,
            };
            keys.push("indices".to_string());
            vals.push(indices_v);
            handle_word_auto(alloc_shaped_object_owned(keys, &vals)) as i64
        })
        .collect();
    alloc_entry(Entry::Vec(Box::new(outer)))
}

// ── _AUTO trampolines (dispatch string-vs-regex no RUNTIME, não no codegen) ─────
// match/search/matchAll aceitam pattern string OU regex. O codegen emite UM
// símbolo genérico por método (`__rtsadp_str_*_auto`, codegen-owned); estes
// recebem HANDLES (recv + pattern), extraem ptr/len e inspecionam `Entry::Regex`
// em runtime. Bônus: cobrem `text.match(r)` onde `r` é var RegExp (handle).

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_PTR(h: u64) -> *const u8;
    fn __RTS_FN_NS_GC_STRING_LEN(h: u64) -> i64;
}

#[inline]
fn handle_is_regex(h: u64) -> bool {
    rts_engine::heap::handles::with_entry(h, |e| {
        matches!(e, Some(rts_engine::heap::handles::Entry::Regex(_)))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_match_auto(recv: u64, pattern: u64) -> u64 {
    let (sp, sl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(recv), __RTS_FN_NS_GC_STRING_LEN(recv)) };
    if handle_is_regex(pattern) {
        match_regex(sp, sl, pattern)
    } else {
        let (pp, pl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(pattern), __RTS_FN_NS_GC_STRING_LEN(pattern)) };
        match_str(sp, sl, pp, pl)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_search_auto(recv: u64, pattern: u64) -> i64 {
    let (sp, sl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(recv), __RTS_FN_NS_GC_STRING_LEN(recv)) };
    if handle_is_regex(pattern) {
        search_regex(sp, sl, pattern)
    } else {
        let (pp, pl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(pattern), __RTS_FN_NS_GC_STRING_LEN(pattern)) };
        search_str(sp, sl, pp, pl)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_match_all_auto(recv: u64, pattern: u64) -> u64 {
    let (sp, sl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(recv), __RTS_FN_NS_GC_STRING_LEN(recv)) };
    if handle_is_regex(pattern) {
        match_all_regex(sp, sl, pattern)
    } else {
        let (pp, pl) = unsafe { (__RTS_FN_NS_GC_STRING_PTR(pattern), __RTS_FN_NS_GC_STRING_LEN(pattern)) };
        match_all_str(sp, sl, pp, pl)
    }
}
