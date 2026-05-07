pub use rts_abi::*;

pub mod signature;

pub const GLOBAL_CLASS_SPECS: &[&GlobalClassSpec] = &[
    &crate::namespaces::globals::string::abi::STRING_CLASS_SPEC,
    &crate::namespaces::globals::number::abi::NUMBER_CLASS_SPEC,
    &crate::namespaces::globals::date::abi::CLASS_SPEC,
    &crate::namespaces::globals::regexp::abi::CLASS_SPEC,
    &crate::namespaces::globals::error::abi::CLASS_SPEC,
    &crate::namespaces::globals::error::abi::TYPE_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::abi::RANGE_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::abi::REF_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::error::abi::SYNTAX_ERROR_CLASS_SPEC,
    &crate::namespaces::globals::events::abi::CLASS_SPEC,
    &crate::namespaces::globals::text_encoding::class_spec::TEXT_ENCODER_CLASS_SPEC,
    &crate::namespaces::globals::text_encoding::class_spec::TEXT_DECODER_CLASS_SPEC,
    &crate::namespaces::globals::fetch::class_spec::RESPONSE_CLASS_SPEC,
    &crate::namespaces::globals::fetch::class_spec::PROMISE_CLASS_SPEC,
    &crate::namespaces::globals::url::class_spec::URL_CLASS_SPEC,
    &crate::namespaces::globals::url::class_spec::URLSP_CLASS_SPEC,
    &crate::namespaces::globals::function::abi::FUNCTION_CLASS_SPEC,
    &crate::namespaces::globals::symbol::SYMBOL_CLASS_SPEC,
    &crate::namespaces::globals::boolean::BOOLEAN_CLASS_SPEC,
    &crate::namespaces::globals::weakmap::WEAKMAP_CLASS_SPEC,
    &crate::namespaces::globals::weakset::WEAKSET_CLASS_SPEC,
];

pub fn global_class_lookup(name: &str) -> Option<&'static GlobalClassSpec> {
    GLOBAL_CLASS_SPECS.iter().copied().find(|s| s.name == name)
}

pub const SPECS: &[&NamespaceSpec] = &[
    &crate::namespaces::gc::abi::SPEC,
    &crate::namespaces::io::abi::SPEC,
    &crate::namespaces::json::abi::SPEC,
    &crate::namespaces::date::abi::SPEC,
    &crate::namespaces::fs::abi::SPEC,
    &crate::namespaces::math::abi::SPEC,
    &crate::namespaces::net::abi::SPEC,
    &crate::namespaces::num::abi::SPEC,
    &crate::namespaces::mem::abi::SPEC,
    &crate::namespaces::trace::abi::SPEC,
    &crate::namespaces::alloc::abi::SPEC,
    &crate::namespaces::bigfloat::abi::SPEC,
    &crate::namespaces::time::abi::SPEC,
    &crate::namespaces::env::abi::SPEC,
    &crate::namespaces::path::abi::SPEC,
    &crate::namespaces::buffer::abi::SPEC,
    &crate::namespaces::ffi::abi::SPEC,
    &crate::namespaces::atomic::abi::SPEC,
    &crate::namespaces::sync::abi::SPEC,
    &crate::namespaces::globals::string::abi::SPEC,
    &crate::namespaces::process::abi::SPEC,
    &crate::namespaces::promise::abi::SPEC,
    &crate::namespaces::ptr::abi::SPEC,
    &crate::namespaces::os::abi::SPEC,
    &crate::namespaces::collections::abi::SPEC,
    &crate::namespaces::hash::abi::SPEC,
    &crate::namespaces::hint::abi::SPEC,
    &crate::namespaces::http_server::abi::SPEC,
    &crate::namespaces::fmt::abi::SPEC,
    &crate::namespaces::crypto::abi::SPEC,
    &crate::namespaces::regex::abi::SPEC,
    &crate::namespaces::ui::abi::SPEC,
    &crate::namespaces::runtime::abi::SPEC,
    &crate::namespaces::test::abi::SPEC,
    &crate::namespaces::thread::abi::SPEC,
    &crate::namespaces::parallel::abi::SPEC,
    &crate::namespaces::tls::abi::SPEC,
    &crate::namespaces::globals::json::abi::SPEC,
    &crate::namespaces::globals::console::abi::SPEC,
    &crate::namespaces::globals::timers::abi::SPEC,
    &crate::namespaces::globals::fetch::abi::SPEC,
    &crate::namespaces::globals::text_encoding::abi::SPEC,
    &crate::namespaces::globals::performance::abi::SPEC,
    &crate::namespaces::globals::url::abi::SPEC,
    &crate::namespaces::events::abi::SPEC,
];

pub fn lookup(qualified: &str) -> Option<(&'static NamespaceSpec, &'static NamespaceMember)> {
    let (ns_name, fn_name) = qualified.split_once('.')?;
    let spec = SPECS.iter().copied().find(|spec| spec.name == ns_name)?;
    let member = spec.members.iter().find(|m| m.name == fn_name)?;
    Some((spec, member))
}
