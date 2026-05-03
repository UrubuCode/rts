//! Object — runtime implementations.

use crate::abi::handles::{alloc_str_handle, map_from_handle, vec_from_handle, alloc_map_handle, alloc_vec_handle};

/// Object() — creates a new empty object (Map handle).
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_CREATE() -> u64 {
    crate::namespaces::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_NEW()
}

/// Object.entries(obj) — returns array of [key, value] pairs.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_ENTRIES(handle: u64) -> u64 {
    if let Some(map) = map_from_handle(handle) {
        // Cria um Vec de handles, cada um sendo um par [key_handle, value_handle]
        let mut pairs = Vec::new();
        for (key, value) in map.iter() {
            // Cria um pequeno vec para o par [key, value]
            let key_handle = alloc_str_handle(key.as_bytes());
            let pair_vec = vec![key_handle as i64, *value];
            let pair_handle = alloc_vec_handle(&pair_vec);
            pairs.push(pair_handle as i64);
        }
        alloc_vec_handle(&pairs)
    } else {
        alloc_vec_handle(&[])
    }
}

/// Object.assign(target, source) — copies properties from source to target.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_ASSIGN(target: u64, source: u64) {
    if let Some(target_map) = map_from_handle(target) {
        if let Some(source_map) = map_from_handle(source) {
            for (key, value) in source_map.iter() {
                unsafe {
                    let target_mut = &mut *(target as *mut std::collections::HashMap<String, i64>);
                    target_mut.insert(key.clone(), *value);
                }
            }
        }
    }
}

/// Object.fromEntries(iterable) — creates object from key-value pairs.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_FROM_ENTRIES(iterable: u64) -> u64 {
    // Espera-se um Vec de pares [key_handle, value]
    if let Some(pairs) = vec_from_handle(iterable) {
        let mut map = std::collections::HashMap::new();
        for &pair_handle_val in pairs.iter() {
            if let Some(pair_vec) = vec_from_handle(pair_handle_val as u64) {
                if pair_vec.len() >= 2 {
                    // key é um handle de string
                    let key_handle = pair_vec[0] as u64;
                    let value = pair_vec[1];
                    // Extrai a string do handle
                    if let Some(s) = crate::abi::handles::str_from_handle(key_handle) {
                        map.insert(s.to_string(), value);
                    }
                }
            }
        }
        alloc_map_handle(&map)
    } else {
        crate::namespaces::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_NEW()
    }
}

/// Object.defineProperty(obj, prop, value) — defines a property.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_DEFINE_PROP(handle: u64, prop_ptr: u64, prop_len: u64, value: i64) {
    if let Some(_) = map_from_handle(handle) {
        if let Some(prop) = crate::abi::handles::str_from_ptr(prop_ptr, prop_len) {
            unsafe {
                let map_mut = &mut *(handle as *mut std::collections::HashMap<String, i64>);
                map_mut.insert(prop.to_string(), value);
            }
        }
    }
}

/// Object.freeze(obj) — marks object as immutable (placeholder).
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_FREEZE(_handle: u64) {
    // Placeholder: em implementacao futura, pode setar flag de frozen no handle metadata
}

/// Object.seal(obj) — seals object (placeholder).
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_SEAL(_handle: u64) {
    // Placeholder: em implementacao futura, pode setar flag de sealed
}

/// obj.toString() — returns "[object Object]".
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_OBJECT_TO_STRING(_handle: u64) -> u64 {
    alloc_str_handle(b"[object Object]")
}

