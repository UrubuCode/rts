pub use rts_engine::abi::*;

pub mod signature;

pub const GLOBAL_CLASS_SPECS: &[&GlobalClassSpec] = &[
    &crate::namespaces::globals::string::STRING_CLASS_SPEC,
    &crate::namespaces::globals::number::NUMBER_CLASS_SPEC,
    &crate::namespaces::globals::date::CLASS_SPEC,
    &crate::namespaces::globals::regexp::REGEXP_CLASS_SPEC,
    &crate::namespaces::globals::error::CLASS_SPEC,
    &crate::namespaces::globals::error::TYPE_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::RANGE_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::REF_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::SYNTAX_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::URI_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::EVAL_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::AGGREGATE_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::events::CLASS_SPEC,
    &crate::namespaces::globals::text_encoding::TEXT_ENCODER_CLASS_SPEC,
    &crate::namespaces::globals::text_encoding::TEXT_DECODER_CLASS_SPEC,
    &crate::namespaces::globals::fetch::RESPONSE_CLASS_SPEC,
    &crate::namespaces::globals::fetch::REQUEST_CLASS_SPEC,
    &crate::namespaces::globals::fetch::PROMISE_CLASS_SPEC,
    &crate::namespaces::globals::url::URL_CLASS_SPEC,
    &crate::namespaces::globals::url::URLSP_CLASS_SPEC,
    &crate::namespaces::globals::function::FUNCTION_CLASS_SPEC,
    &crate::namespaces::globals::symbol::SYMBOL_CLASS_SPEC,
    &crate::namespaces::globals::boolean::BOOLEAN_CLASS_SPEC,
    &crate::namespaces::globals::bigint::BIGINT_CLASS_SPEC,
    &crate::namespaces::globals::weakmap::WEAKMAP_CLASS_SPEC,
    &crate::namespaces::globals::weakset::WEAKSET_CLASS_SPEC,
    &crate::namespaces::globals::weakref::WEAKREF_CLASS_SPEC,
    &crate::namespaces::globals::finalization_registry::FINALIZATION_REGISTRY_CLASS_SPEC,
    &crate::namespaces::globals::headers::HEADERS_CLASS_SPEC,
    &crate::namespaces::globals::abort::ABORT_CONTROLLER_CLASS_SPEC,
    &crate::namespaces::globals::abort::ABORT_SIGNAL_CLASS_SPEC,
    &crate::namespaces::globals::event_target::EVENT_TARGET_CLASS_SPEC,
    &crate::namespaces::globals::event_target::EVENT_CLASS_SPEC,
    &crate::namespaces::globals::dom_exception::DOM_EXCEPTION_CLASS_SPEC,
    &crate::namespaces::globals::blob::BLOB_CLASS_SPEC,
    &crate::namespaces::globals::blob::FILE_CLASS_SPEC,
    &crate::namespaces::globals::form_data::FORM_DATA_CLASS_SPEC,
    &crate::namespaces::globals::dataview::ARRAY_BUFFER_CLASS_SPEC,
    &crate::namespaces::globals::dataview::DATA_VIEW_CLASS_SPEC,
    &crate::namespaces::globals::intl::NUMBER_FORMAT_CLASS_SPEC,
    &crate::namespaces::globals::intl::DATE_TIME_FORMAT_CLASS_SPEC,
    &crate::namespaces::globals::intl::COLLATOR_CLASS_SPEC,
    &crate::namespaces::globals::intl::SEGMENTER_CLASS_SPEC,
    &crate::namespaces::globals::intl::PLURAL_RULES_CLASS_SPEC,
    &crate::namespaces::globals::intl::LIST_FORMAT_CLASS_SPEC,
    &crate::namespaces::globals::intl::RELATIVE_TIME_FORMAT_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::READABLE_STREAM_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::READER_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::CONTROLLER_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::TRANSFORM_STREAM_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::WRITABLE_STREAM_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::WRITER_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::TEXT_ENCODER_STREAM_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::TEXT_DECODER_STREAM_CLASS_SPEC,
    &crate::namespaces::globals::readable_stream::COMPRESSION_STREAM_CLASS_SPEC,
    &crate::namespaces::globals::message_channel::MESSAGE_CHANNEL_CLASS_SPEC,
    &crate::namespaces::globals::message_channel::MESSAGE_PORT_CLASS_SPEC,
];

pub fn global_class_lookup(name: &str) -> Option<&'static GlobalClassSpec> {
    registry().read().unwrap().classes.get(name).copied()
}

pub const SPECS: &[&NamespaceSpec] = &[
    &crate::namespaces::gc::SPEC,
    &crate::namespaces::io::SPEC,
    &crate::namespaces::json::SPEC,
    &crate::namespaces::date::SPEC,
    &crate::namespaces::fs::SPEC,
    &crate::namespaces::math::SPEC,
    &crate::namespaces::net::SPEC,
    &crate::namespaces::num::SPEC,
    &crate::namespaces::mem::SPEC,
    &crate::namespaces::trace::SPEC,
    &crate::namespaces::alloc::SPEC,
    &crate::namespaces::bigfloat::SPEC,
    &crate::namespaces::time::SPEC,
    &crate::namespaces::env::SPEC,
    &crate::namespaces::path::SPEC,
    &crate::namespaces::buffer::SPEC,
    &crate::namespaces::ffi::SPEC,
    &crate::namespaces::atomic::SPEC,
    &crate::namespaces::sync::SPEC,
    &crate::namespaces::globals::string::SPEC,
    &crate::namespaces::process::SPEC,
    &crate::namespaces::promise::SPEC,
    &crate::namespaces::ptr::SPEC,
    &crate::namespaces::os::SPEC,
    &crate::namespaces::collections::SPEC,
    &crate::namespaces::hash::SPEC,
    &crate::namespaces::hint::SPEC,
    &crate::namespaces::http_server::SPEC,
    &crate::namespaces::fmt::SPEC,
    &crate::namespaces::crypto::SPEC,
    &crate::namespaces::regex::SPEC,
    &crate::namespaces::audio::SPEC,
    #[cfg(feature = "asio")]
    &crate::namespaces::asio_audio::SPEC,
    &crate::namespaces::runtime::SPEC,
    &crate::namespaces::test::SPEC,
    &crate::namespaces::thread::SPEC,
    &crate::namespaces::parallel::SPEC,
    &crate::namespaces::tls::SPEC,
    &crate::namespaces::globals::json::SPEC,
    &crate::namespaces::globals::json5::SPEC,
    &crate::namespaces::globals::console::SPEC,
    &crate::namespaces::globals::timers::SPEC,
    &crate::namespaces::globals::fetch::SPEC,
    &crate::namespaces::globals::text_encoding::SPEC,
    &crate::namespaces::globals::performance::SPEC,
    &crate::namespaces::globals::url::SPEC,
    &crate::namespaces::globals::global_this::SPEC,
    &crate::namespaces::events::SPEC,
];

/// Índice O(1) sobre os const arrays — o **registry** que o codegen lê (Track A,
/// F3a, `RTS_ENGINE.md` §10.2). `register_builtins()` o semeia de `SPECS`/
/// `GLOBAL_CLASS_SPECS` (a única origem hoje); o codegen consulta `lookup`/
/// `global_class_lookup` por aqui em vez de varrer os arrays. Mantém
/// `&'static NamespaceMember` como moeda do codegen. Evolução: trocar `OnceLock`
/// por `RwLock` + inserção runtime habilita builder/módulos externos (F3b/Fase 2).
struct Registry {
    namespaces: std::collections::HashMap<&'static str, &'static NamespaceSpec>,
    classes: std::collections::HashMap<&'static str, &'static GlobalClassSpec>,
}

// `RwLock` (não `OnceLock` puro) porque, além do seed dos const arrays, o
// registry aceita inserção em runtime: módulos registrados via o builder do
// `rts-engine` (camadas `rts-std`/externos) entram por `register_namespace`/
// `register_class` antes da compilação. Leituras devolvem `&'static` (copiadas
// pra fora do guard), então o lock não vaza nos call-sites do codegen.
static REGISTRY: std::sync::OnceLock<std::sync::RwLock<Registry>> = std::sync::OnceLock::new();

fn registry() -> &'static std::sync::RwLock<Registry> {
    REGISTRY.get_or_init(|| std::sync::RwLock::new(register_builtins()))
}

/// Semeia o registry a partir dos const arrays (a origem dos builtins hoje).
fn register_builtins() -> Registry {
    let mut namespaces = std::collections::HashMap::with_capacity(SPECS.len());
    for s in SPECS {
        namespaces.insert(s.name, *s);
    }
    let mut classes = std::collections::HashMap::with_capacity(GLOBAL_CLASS_SPECS.len());
    for s in GLOBAL_CLASS_SPECS {
        classes.insert(s.name, *s);
    }
    Registry {
        namespaces,
        classes,
    }
}

pub fn lookup(qualified: &str) -> Option<(&'static NamespaceSpec, &'static NamespaceMember)> {
    let (ns_name, fn_name) = qualified.split_once('.')?;
    let spec = registry().read().unwrap().namespaces.get(ns_name).copied()?;
    let member = spec.members.iter().find(|m| m.name == fn_name)?;
    Some((spec, member))
}

/// Registra um módulo no registry em runtime (camadas/externos). O `spec` deve
/// ser `'static` — use [`leak_namespace`] para converter um `rts_engine::Module`
/// do builder (owned) numa spec vazada.
pub fn register_namespace(spec: &'static NamespaceSpec) {
    registry().write().unwrap().namespaces.insert(spec.name, spec);
}

/// Registra uma classe global no registry em runtime.
pub fn register_class(spec: &'static GlobalClassSpec) {
    registry().write().unwrap().classes.insert(spec.name, spec);
}

/// `symbol → endereço da fn` dos membros registrados em runtime (do builder).
/// Substitui, para módulos do builder/externos, a lista `add_fn!` hardcoded do
/// JIT: `jit.rs` injeta estes em `JITBuilder::symbol` para que uma chamada a uma
/// fn do builder resolva em runtime (F4). `usize` (não `*const u8`) p/ Send/Sync.
static JIT_SYMBOLS: std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<&'static str, usize>>> =
    std::sync::OnceLock::new();

fn jit_symbols() -> &'static std::sync::RwLock<std::collections::HashMap<&'static str, usize>> {
    JIT_SYMBOLS.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()))
}

/// `(symbol, ptr)` de todo membro registrado em runtime — consumido por `jit.rs`
/// para injetar no `JITBuilder`. Vazio até algo chamar [`leak_namespace`].
pub fn runtime_jit_symbols() -> Vec<(&'static str, *const u8)> {
    jit_symbols()
        .read()
        .unwrap()
        .iter()
        .map(|(name, addr)| (*name, *addr as *const u8))
        .collect()
}

/// Converte um `rts_engine::Module` (builder, dados owned) num
/// `&'static NamespaceSpec` vazando os campos (vivem o processo todo, como os
/// const arrays) e gravando os `(symbol, fn_ptr)` em [`jit_symbols`] para o JIT.
/// É a ponte que deixa o builder alimentar o codegen sem reescrever os
/// call-sites (a moeda continua `NamespaceMember`). F3b + F4.
pub fn leak_namespace(module: &rts_engine::Module) -> &'static NamespaceSpec {
    fn s(x: &str) -> &'static str {
        Box::leak(x.to_string().into_boxed_str())
    }
    let mut jit = jit_symbols().write().unwrap();
    let members: Vec<NamespaceMember> = module
        .members
        .iter()
        .map(|m| {
            let symbol = s(&m.symbol);
            jit.insert(symbol, m.fn_ptr.addr());
            NamespaceMember {
                name: s(&m.name),
                kind: m.kind,
                symbol,
                args: Box::leak(m.sig.args.clone().into_boxed_slice()),
                returns: m.sig.returns,
                doc: s(&m.doc),
                ts_signature: s(&m.ts_signature),
                intrinsic: None,
                pure: false,
                aliases: Box::leak(
                    m.aliases
                        .iter()
                        .map(|a| s(a))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
                variadic: m.variadic,
                default_args: &[],
                flags: m.flags,
            }
        })
        .collect();
    drop(jit);
    Box::leak(Box::new(NamespaceSpec {
        name: s(&module.name),
        doc: s(&module.doc),
        members: Box::leak(members.into_boxed_slice()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn dummy_answer() -> i64 {
        42
    }

    /// Prova da ponte builder→codegen (F3b): uma namespace registrada via o
    /// builder do `rts-engine`, convertida com `leak_namespace` + inserida com
    /// `register_namespace`, é encontrada pelo `lookup` do codegen — sem tocar
    /// os call-sites (a moeda continua `NamespaceMember`).
    #[test]
    fn builder_namespace_resolvable_via_lookup() {
        let mut e = rts_engine::Engine::new();
        e.ns("f3btest")
            .function("answer", dummy_answer as *const u8, rts_engine::sig!(=> I64))
            .done();
        let module = e.registry().module("f3btest").expect("module no engine");
        register_namespace(leak_namespace(module));

        let (ns, m) = lookup("f3btest.answer").expect("lookup acha a ns do builder");
        assert_eq!(ns.name, "f3btest");
        assert_eq!(m.symbol, "__RTS_FN_NS_F3BTEST_ANSWER");
        assert!(matches!(m.returns, AbiType::I64));
        // builtins continuam resolvíveis (registry não foi sobrescrito)
        assert!(lookup("io.print").is_some());
        // F4: o símbolo da fn do builder foi gravado pro JIT injetar
        let syms = runtime_jit_symbols();
        let entry = syms
            .iter()
            .find(|(name, _)| *name == "__RTS_FN_NS_F3BTEST_ANSWER")
            .expect("símbolo no runtime_jit_symbols");
        assert_eq!(entry.1 as usize, dummy_answer as *const u8 as usize);
    }
}
