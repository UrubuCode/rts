pub use rts_engine::abi::*;

pub mod signature;

// Fase 2 classes — TODAS as classes globais migraram pro caminho do builder
// (`register_<spec>_class_spec` auto-gerado pela macro `#[rts_class]`, foldado
// via `leak_class` em `register_builtins`). O const ficou vazio; o codegen lê
// as classes pelo registry (`global_class_lookup`/`registry_classes_ordered`).
pub const GLOBAL_CLASS_SPECS: &[&GlobalClassSpec] = &[];

pub fn global_class_lookup(name: &str) -> Option<&'static GlobalClassSpec> {
    registry().read().unwrap().classes.get(name).copied()
}

// Fase 2 — TODAS as namespaces migraram pro caminho do builder do `rts-engine`
// (registradas em `register_builtins` via `<ns>::register`). O const ficou
// vazio, igual a `GLOBAL_CLASS_SPECS`; o codegen lê tudo pelo registry. `gc`
// usa `gc::register()` (macro); `collections` um `register()` hand-written que
// agrega os dois `part` (map/vec) via `append_engine_members`.
pub const SPECS: &[&NamespaceSpec] = &[];

/// Índice O(1) sobre os const arrays — o **registry** que o codegen lê (Track A,
/// F3a, `RTS_ENGINE.md` §10.2). `register_builtins()` o semeia de `SPECS`/
/// `GLOBAL_CLASS_SPECS` (a única origem hoje); o codegen consulta `lookup`/
/// `global_class_lookup` por aqui em vez de varrer os arrays. Mantém
/// `&'static NamespaceMember` como moeda do codegen. Evolução: trocar `OnceLock`
/// por `RwLock` + inserção runtime habilita builder/módulos externos (F3b/Fase 2).
struct Registry {
    namespaces: std::collections::HashMap<&'static str, &'static NamespaceSpec>,
    classes: std::collections::HashMap<&'static str, &'static GlobalClassSpec>,
    /// Ordem de iteração (= ordem do const array no seed, + append em runtime).
    /// Os HashMaps são por-chave (ordem-independente); estes Vecs preservam a
    /// ordem que o gerador de `rts.d.ts`/`apis` precisa (determinística).
    specs_ordered: Vec<&'static NamespaceSpec>,
    classes_ordered: Vec<&'static GlobalClassSpec>,
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
    let mut specs_ordered = Vec::with_capacity(SPECS.len());
    for s in SPECS {
        namespaces.insert(s.name, *s);
        specs_ordered.push(*s);
    }
    let mut classes = std::collections::HashMap::with_capacity(GLOBAL_CLASS_SPECS.len());
    let mut classes_ordered = Vec::with_capacity(GLOBAL_CLASS_SPECS.len());
    for s in GLOBAL_CLASS_SPECS {
        classes.insert(s.name, *s);
        classes_ordered.push(*s);
    }

    // Camadas registradas via o builder do `rts-engine` (Fase 2). Foldadas
    // direto nos maps locais — NÃO via `register_namespace`/`register_class`
    // (que usam `registry()`, o mesmo `OnceLock` em init → reentrância/deadlock).
    // `leak_namespace` só toca `jit_symbols` (outro OnceLock), seguro aqui.
    let mut engine = rts_engine::Engine::new();
    // gc (macro) + collections (owner hand-written agregando map/vec parts):
    crate::namespaces::gc::register(&mut engine);
    crate::namespaces::collections::register(&mut engine);
    // Migradas à mão (helper `func`/`pure_func` + builder):
    crate::namespaces::hint::register(&mut engine);
    crate::namespaces::hash::register(&mut engine);
    crate::namespaces::alloc::register(&mut engine);
    crate::namespaces::time::register(&mut engine);
    crate::namespaces::trace::register(&mut engine);
    crate::namespaces::env::register(&mut engine);
    crate::namespaces::path::register(&mut engine);
    crate::namespaces::fmt::register(&mut engine);
    crate::namespaces::ptr::register(&mut engine);
    // Migradas via `register()` auto-gerado pela macro `#[rts_namespace]`
    // (Fase 2). A macro emite a mesma metadata do const `SPEC`, então
    // `leak_namespace` produz um `NamespaceMember` byte-equivalente.
    crate::namespaces::io::register(&mut engine);
    crate::namespaces::json::register(&mut engine);
    crate::namespaces::date::register(&mut engine);
    crate::namespaces::fs::register(&mut engine);
    crate::namespaces::math::register(&mut engine);
    crate::namespaces::net::register(&mut engine);
    crate::namespaces::num::register(&mut engine);
    crate::namespaces::mem::register(&mut engine);
    crate::namespaces::bigfloat::register(&mut engine);
    crate::namespaces::buffer::register(&mut engine);
    crate::namespaces::ffi::register(&mut engine);
    crate::namespaces::atomic::register(&mut engine);
    crate::namespaces::sync::register(&mut engine);
    crate::namespaces::globals::string::register(&mut engine);
    crate::namespaces::process::register(&mut engine);
    crate::namespaces::promise::register(&mut engine);
    crate::namespaces::os::register(&mut engine);
    crate::namespaces::http_server::register(&mut engine);
    crate::namespaces::crypto::register(&mut engine);
    crate::namespaces::regex::register(&mut engine);
    crate::namespaces::audio::register(&mut engine);
    #[cfg(feature = "asio")]
    crate::namespaces::asio_audio::register(&mut engine);
    crate::namespaces::runtime::register(&mut engine);
    crate::namespaces::test::register(&mut engine);
    crate::namespaces::thread::register(&mut engine);
    crate::namespaces::parallel::register(&mut engine);
    crate::namespaces::tls::register(&mut engine);
    crate::namespaces::globals::json::register(&mut engine);
    crate::namespaces::globals::json5::register(&mut engine);
    crate::namespaces::globals::console::register(&mut engine);
    crate::namespaces::globals::timers::register(&mut engine);
    crate::namespaces::globals::fetch::register(&mut engine);
    crate::namespaces::globals::text_encoding::register(&mut engine);
    crate::namespaces::globals::performance::register(&mut engine);
    crate::namespaces::globals::url::register(&mut engine);
    crate::namespaces::globals::global_this::register(&mut engine);
    crate::namespaces::events::register(&mut engine);
    // Classes globais (Fase 2 classes) — `register_<spec>` auto-gerado pela
    // macro `#[rts_class]` (nome = `register_<SPEC_IDENT lowercased>`, único por
    // módulo). Cobrem exatamente as 62 entradas do antigo const GLOBAL_CLASS_SPECS.
    {
        use crate::namespaces::globals as g;
        g::string::register_string_class_spec(&mut engine);
        g::number::register_number_class_spec(&mut engine);
        g::date::register_class_spec(&mut engine);
        g::regexp::register_regexp_class_spec(&mut engine);
        crate::namespaces::collections::register_mapset_class_spec(&mut engine);
        g::array::register_array_class_spec(&mut engine);
        g::error::register_class_spec(&mut engine);
        g::error::register_type_error_class_spec(&mut engine);
        g::error::register_range_error_class_spec(&mut engine);
        g::error::register_ref_error_class_spec(&mut engine);
        g::error::register_syntax_error_class_spec(&mut engine);
        g::error::register_uri_error_class_spec(&mut engine);
        g::error::register_eval_error_class_spec(&mut engine);
        g::error::register_aggregate_error_class_spec(&mut engine);
        g::events::register_class_spec(&mut engine);
        g::text_encoding::register_text_encoder_class_spec(&mut engine);
        g::text_encoding::register_text_decoder_class_spec(&mut engine);
        g::fetch::register_response_class_spec(&mut engine);
        g::fetch::register_request_class_spec(&mut engine);
        g::fetch::register_promise_class_spec(&mut engine);
        g::url::register_url_class_spec(&mut engine);
        g::url::register_urlsp_class_spec(&mut engine);
        g::function::register_function_class_spec(&mut engine);
        g::symbol::register_symbol_class_spec(&mut engine);
        g::boolean::register_boolean_class_spec(&mut engine);
        g::bigint::register_bigint_class_spec(&mut engine);
        g::weakmap::register_weakmap_class_spec(&mut engine);
        g::weakset::register_weakset_class_spec(&mut engine);
        g::weakref::register_weakref_class_spec(&mut engine);
        g::finalization_registry::register_finalization_registry_class_spec(&mut engine);
        g::headers::register_headers_class_spec(&mut engine);
        g::abort::register_abort_controller_class_spec(&mut engine);
        g::abort::register_abort_signal_class_spec(&mut engine);
        g::event_target::register_event_target_class_spec(&mut engine);
        g::event_target::register_event_class_spec(&mut engine);
        g::dom_exception::register_dom_exception_class_spec(&mut engine);
        g::blob::register_blob_class_spec(&mut engine);
        g::blob::register_file_class_spec(&mut engine);
        g::form_data::register_form_data_class_spec(&mut engine);
        g::dataview::register_array_buffer_class_spec(&mut engine);
        g::dataview::register_data_view_class_spec(&mut engine);
        g::intl::register_number_format_class_spec(&mut engine);
        g::intl::register_date_time_format_class_spec(&mut engine);
        g::intl::register_collator_class_spec(&mut engine);
        g::intl::register_segmenter_class_spec(&mut engine);
        g::intl::register_plural_rules_class_spec(&mut engine);
        g::intl::register_list_format_class_spec(&mut engine);
        g::intl::register_relative_time_format_class_spec(&mut engine);
        g::readable_stream::register_readable_stream_class_spec(&mut engine);
        g::readable_stream::register_reader_class_spec(&mut engine);
        g::readable_stream::register_controller_class_spec(&mut engine);
        g::readable_stream::register_transform_stream_class_spec(&mut engine);
        g::readable_stream::register_writable_stream_class_spec(&mut engine);
        g::readable_stream::register_writer_class_spec(&mut engine);
        g::readable_stream::register_text_encoder_stream_class_spec(&mut engine);
        g::readable_stream::register_text_decoder_stream_class_spec(&mut engine);
        g::readable_stream::register_compression_stream_class_spec(&mut engine);
        g::message_channel::register_message_channel_class_spec(&mut engine);
        g::message_channel::register_message_port_class_spec(&mut engine);
    }
    for module in engine.registry().modules() {
        let spec = leak_namespace(module);
        namespaces.insert(spec.name, spec);
        specs_ordered.push(spec);
    }
    // Folda as classes do builder (análogo aos módulos). `engine.registry().
    // classes()` vem do `register_<spec>` acima; vazio se nenhum chamado.
    for class in engine.registry().classes() {
        let spec = leak_class(class);
        classes.insert(spec.name, spec);
        classes_ordered.push(spec);
    }

    Registry {
        namespaces,
        classes,
        specs_ordered,
        classes_ordered,
    }
}

/// Namespaces na ordem de iteração (const seed + módulos do builder/externos
/// appendados). Usado pelo gerador de `rts.d.ts` (`emit_types`) e por `rts apis`
/// para uma saída determinística que inclui módulos registrados em runtime.
pub fn registry_specs_ordered() -> Vec<&'static NamespaceSpec> {
    registry().read().unwrap().specs_ordered.clone()
}

/// Classes globais na ordem de iteração (const seed + builder/externos).
pub fn registry_classes_ordered() -> Vec<&'static GlobalClassSpec> {
    registry().read().unwrap().classes_ordered.clone()
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
    let mut reg = registry().write().unwrap();
    if reg.namespaces.insert(spec.name, spec).is_none() {
        reg.specs_ordered.push(spec); // novo nome → entra na ordem de iteração
    }
}

/// Resolve uma namespace pelo nome no registry (builtins seeds + módulos do
/// builder/externos). Usado pelo resolvedor de import (`builtin_module`) para
/// que módulos do builder sejam importáveis, não só os do const `SPECS`.
pub fn registry_namespace(name: &str) -> Option<&'static NamespaceSpec> {
    registry().read().unwrap().namespaces.get(name).copied()
}

/// Registra uma classe global no registry em runtime.
pub fn register_class(spec: &'static GlobalClassSpec) {
    let mut reg = registry().write().unwrap();
    if reg.classes.insert(spec.name, spec).is_none() {
        reg.classes_ordered.push(spec);
    }
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

fn leak_str(x: &str) -> &'static str {
    Box::leak(x.to_string().into_boxed_str())
}

/// Converte um `rts_engine::Member` (builder, owned) num `&'static`-friendly
/// `NamespaceMember`, vazando strings/slices e gravando `(symbol, fn_ptr)` em
/// [`jit_symbols`]. Compartilhado por [`leak_namespace`] e [`leak_class`].
///
/// **Invariante crítica:** membros `alias`/`external` carregam `fn_ptr` null e
/// REUSAM o `symbol` do membro dono (real). Inserir 0 aqui SOBRESCREVERIA o
/// endereço real → call para 0x0 (ACCESS_VIOLATION, não-determinístico por
/// ordem de HashMap). Só grava quando há ponteiro próprio; o alias resolve pelo
/// dono.
fn leak_member(
    jit: &mut std::collections::HashMap<&'static str, usize>,
    m: &rts_engine::Member,
) -> NamespaceMember {
    let symbol = leak_str(&m.symbol);
    if !m.fn_ptr.0.is_null() {
        jit.insert(symbol, m.fn_ptr.addr());
    }
    NamespaceMember {
        name: leak_str(&m.name),
        kind: m.kind,
        symbol,
        args: Box::leak(m.sig.args.clone().into_boxed_slice()),
        returns: m.sig.returns,
        doc: leak_str(&m.doc),
        ts_signature: leak_str(&m.ts_signature),
        intrinsic: m.intrinsic,
        pure: m.pure,
        aliases: Box::leak(
            m.aliases
                .iter()
                .map(|a| leak_str(a))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
        variadic: m.variadic,
        // default_args vem do `Sig` do membro (registry-driven). Vazio na
        // maioria; populado via `Sig::with_defaults` para os métodos cujo arg
        // omitido tem default não-trivial (≠ zero-do-tipo). O emissor genérico
        // `lower_global_instance_call` injeta esse valor em vez de hardcodar.
        default_args: Box::leak(m.sig.default_args.clone().into_boxed_slice()),
        flags: m.flags,
    }
}

/// Converte um `rts_engine::Module` (builder, dados owned) num
/// `&'static NamespaceSpec` vazando os campos (vivem o processo todo, como os
/// const arrays) e gravando os `(symbol, fn_ptr)` em [`jit_symbols`] para o JIT.
/// É a ponte que deixa o builder alimentar o codegen sem reescrever os
/// call-sites (a moeda continua `NamespaceMember`). F3b + F4.
pub fn leak_namespace(module: &rts_engine::Module) -> &'static NamespaceSpec {
    let mut jit = jit_symbols().write().unwrap();
    let members: Vec<NamespaceMember> = module
        .members
        .iter()
        .map(|m| leak_member(&mut jit, m))
        .collect();
    drop(jit);
    Box::leak(Box::new(NamespaceSpec {
        name: leak_str(&module.name),
        doc: leak_str(&module.doc),
        members: Box::leak(members.into_boxed_slice()),
    }))
}

/// Converte uma `rts_engine::Class` (builder, owned) num
/// `&'static GlobalClassSpec`, análogo a [`leak_namespace`] (classes globais:
/// `new Date()`, `d.getFullYear()`). Grava os símbolos no JIT via [`leak_member`]
/// (que pula fn_ptr null de membros `external`). Fase 2 classes.
pub fn leak_class(class: &rts_engine::Class) -> &'static GlobalClassSpec {
    let mut jit = jit_symbols().write().unwrap();
    let members: Vec<NamespaceMember> = class
        .members
        .iter()
        .map(|m| leak_member(&mut jit, m))
        .collect();
    drop(jit);
    Box::leak(Box::new(GlobalClassSpec {
        name: leak_str(&class.name),
        doc: leak_str(&class.doc),
        members: Box::leak(members.into_boxed_slice()),
        instanceof_predicate: class.instanceof_predicate.as_deref().map(leak_str),
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
