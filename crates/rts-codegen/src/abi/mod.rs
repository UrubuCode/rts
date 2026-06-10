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
    registry().classes.get(name).copied()
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

static REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(register_builtins)
}

/// Semeia o registry a partir dos const arrays (a única origem de builtins hoje).
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
    let spec = registry().namespaces.get(ns_name).copied()?;
    let member = spec.members.iter().find(|m| m.name == fn_name)?;
    Some((spec, member))
}
