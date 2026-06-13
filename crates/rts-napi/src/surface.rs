//! Superfície N-API restante (não implementada) — stubs `napi_generic_failure`.
//! Existem na export table para o `load_all()` do `napi-sys` completar.
//! Um addon que CHAMAR uma destas recebe falha graciosa. Ver
//! docs/specs/napi-implementation.md.

#![allow(clippy::too_many_arguments)]
use crate::types::napi_status;
use napi_status::napi_generic_failure;

macro_rules! surface_stub {
    ($name:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            _a: usize, _b: usize, _c: usize, _d: usize, _e: usize, _f: usize,
        ) -> napi_status { napi_generic_failure }
    };
}

surface_stub!(napi_acquire_threadsafe_function);
surface_stub!(napi_add_async_cleanup_hook);
surface_stub!(napi_add_env_cleanup_hook);
surface_stub!(napi_add_finalizer);
surface_stub!(napi_adjust_external_memory);
surface_stub!(napi_async_destroy);
surface_stub!(napi_async_init);
surface_stub!(napi_call_threadsafe_function);
surface_stub!(napi_cancel_async_work);
surface_stub!(napi_check_object_type_tag);
surface_stub!(napi_close_callback_scope);
surface_stub!(napi_coerce_to_string);
surface_stub!(napi_create_arraybuffer);
surface_stub!(napi_create_async_work);
surface_stub!(napi_create_bigint_uint64);
surface_stub!(napi_create_bigint_words);
surface_stub!(napi_create_dataview);
surface_stub!(napi_create_external_arraybuffer);
surface_stub!(napi_create_external_buffer);
surface_stub!(napi_create_promise);
surface_stub!(napi_create_threadsafe_function);
surface_stub!(napi_create_typedarray);
surface_stub!(napi_define_class);
surface_stub!(napi_delete_async_work);
surface_stub!(napi_detach_arraybuffer);
surface_stub!(napi_fatal_error);
surface_stub!(napi_fatal_exception);
surface_stub!(napi_get_arraybuffer_info);
surface_stub!(napi_get_dataview_info);
surface_stub!(napi_get_prototype);
surface_stub!(napi_get_threadsafe_function_context);
surface_stub!(napi_get_typedarray_info);
surface_stub!(napi_get_uv_event_loop);
surface_stub!(napi_get_value_bigint_uint64);
surface_stub!(napi_get_value_bigint_words);
surface_stub!(napi_is_arraybuffer);
surface_stub!(napi_is_dataview);
surface_stub!(napi_is_detached_arraybuffer);
surface_stub!(napi_is_typedarray);
surface_stub!(napi_make_callback);
surface_stub!(napi_module_register);
surface_stub!(napi_new_instance);
surface_stub!(napi_open_callback_scope);
surface_stub!(napi_queue_async_work);
surface_stub!(napi_ref_threadsafe_function);
surface_stub!(napi_reject_deferred);
surface_stub!(napi_release_threadsafe_function);
surface_stub!(napi_remove_async_cleanup_hook);
surface_stub!(napi_remove_env_cleanup_hook);
surface_stub!(napi_resolve_deferred);
surface_stub!(napi_run_script);
surface_stub!(napi_type_tag_object);
surface_stub!(napi_unref_threadsafe_function);
surface_stub!(node_api_create_buffer_from_arraybuffer);
surface_stub!(node_api_create_external_string_latin1);
surface_stub!(node_api_create_external_string_utf16);
surface_stub!(node_api_create_sharedarraybuffer);
surface_stub!(node_api_create_syntax_error);
surface_stub!(node_api_get_module_file_name);
surface_stub!(node_api_is_sharedarraybuffer);
surface_stub!(node_api_post_finalizer);
surface_stub!(node_api_throw_syntax_error);

pub fn force_link_surface() -> usize {
    let fns: &[*const ()] = &[
        napi_acquire_threadsafe_function as *const (),
        napi_add_async_cleanup_hook as *const (),
        napi_add_env_cleanup_hook as *const (),
        napi_add_finalizer as *const (),
        napi_adjust_external_memory as *const (),
        napi_async_destroy as *const (),
        napi_async_init as *const (),
        napi_call_threadsafe_function as *const (),
        napi_cancel_async_work as *const (),
        napi_check_object_type_tag as *const (),
        napi_close_callback_scope as *const (),
        napi_coerce_to_string as *const (),
        napi_create_arraybuffer as *const (),
        napi_create_async_work as *const (),
        napi_create_bigint_uint64 as *const (),
        napi_create_bigint_words as *const (),
        napi_create_dataview as *const (),
        napi_create_external_arraybuffer as *const (),
        napi_create_external_buffer as *const (),
        napi_create_promise as *const (),
        napi_create_threadsafe_function as *const (),
        napi_create_typedarray as *const (),
        napi_define_class as *const (),
        napi_delete_async_work as *const (),
        napi_detach_arraybuffer as *const (),
        napi_fatal_error as *const (),
        napi_fatal_exception as *const (),
        napi_get_arraybuffer_info as *const (),
        napi_get_dataview_info as *const (),
        napi_get_prototype as *const (),
        napi_get_threadsafe_function_context as *const (),
        napi_get_typedarray_info as *const (),
        napi_get_uv_event_loop as *const (),
        napi_get_value_bigint_uint64 as *const (),
        napi_get_value_bigint_words as *const (),
        napi_is_arraybuffer as *const (),
        napi_is_dataview as *const (),
        napi_is_detached_arraybuffer as *const (),
        napi_is_typedarray as *const (),
        napi_make_callback as *const (),
        napi_module_register as *const (),
        napi_new_instance as *const (),
        napi_open_callback_scope as *const (),
        napi_queue_async_work as *const (),
        napi_ref_threadsafe_function as *const (),
        napi_reject_deferred as *const (),
        napi_release_threadsafe_function as *const (),
        napi_remove_async_cleanup_hook as *const (),
        napi_remove_env_cleanup_hook as *const (),
        napi_resolve_deferred as *const (),
        napi_run_script as *const (),
        napi_type_tag_object as *const (),
        napi_unref_threadsafe_function as *const (),
        node_api_create_buffer_from_arraybuffer as *const (),
        node_api_create_external_string_latin1 as *const (),
        node_api_create_external_string_utf16 as *const (),
        node_api_create_sharedarraybuffer as *const (),
        node_api_create_syntax_error as *const (),
        node_api_get_module_file_name as *const (),
        node_api_is_sharedarraybuffer as *const (),
        node_api_post_finalizer as *const (),
        node_api_throw_syntax_error as *const (),
    ];
    std::hint::black_box(fns.iter().map(|p| *p as usize).fold(0usize, usize::wrapping_add))
}
