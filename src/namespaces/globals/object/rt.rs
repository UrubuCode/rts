//! Object — runtime implementations.

use crate::abi::handles::{alloc_str_handle, map_from_handle, vec_from_handle, alloc_map_handle, alloc_vec_handle, modify_map_handle};

/// Object() — creates a new empty object (Map handle).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_CREATE() -> u64 {
    crate::namespaces::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_NEW()
}

/// Object.entries(obj) — returns array of [key, value] pairs.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_ENTRIES(handle: u64) -> u64 {
    if let Some(map) = map_from_handle(handle) {
        let mut pairs = Vec::new();
        for (key, value) in map.iter() {
            let key_handle = alloc_str_handle(key.as_bytes());
            let pair_vec = vec![key_handle as i64, *value];
            let pair_handle = alloc_vec_handle(&pair_vec);
            pairs.push(pair_handle as i64);
        }
        alloc_vec_handle(&pairs)
    } else {
        alloc_vec_handle(&Vec::new())
    }
}

/// Object.assign(target, source) — copies properties from source to target.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_ASSIGN(target: u64, source: u64) {
    if let Some(source_map) = map_from_handle(source) {
        let source_clone: Vec<(String, i64)> = source_map.iter().map(|(k, v)| (k.clone(), *v)).collect();
        modify_map_handle(target, |map| {
            if let Some(m) = map {
                for (key, value) in source_clone.iter() {
                    m.insert(key.clone(), *value);
                }
            }
        });
    }
}

/// Object.fromEntries(iterable) — creates object from key-value pairs.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_FROM_ENTRIES(iterable: u64) -> u64 {
    if let Some(pairs) = vec_from_handle(iterable) {
        let mut map = indexmap::IndexMap::new();
        for &pair_handle_val in pairs.iter() {
            if let Some(pair_vec) = vec_from_handle(pair_handle_val as u64) {
                if pair_vec.len() >= 2 {
                    let key_handle = pair_vec[0] as u64;
                    let value = pair_vec[1];
                    if let Some(key_str) = map_from_handle(key_handle).and_then(|m| m.first().map(|(k, _)| k.clone())) {
                        map.insert(key_str, value);
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_DEFINE_PROP(handle: u64, prop_ptr: u64, prop_len: u64, value: i64) {
    use std::slice;
    let prop_bytes = unsafe { slice::from_raw_parts(prop_ptr as *const u8, prop_len as usize) };
    if let Ok(prop) = std::str::from_utf8(prop_bytes) {
        modify_map_handle(handle, |map| {
            if let Some(m) = map {
                m.insert(prop.to_string(), value);
            }
        });
    }
}

/// Object.freeze(obj) — marks object as immutable (placeholder).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_FREEZE(_handle: u64) {
    // Placeholder
}

/// Object.seal(obj) — seals object (placeholder).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_SEAL(_handle: u64) {
    // Placeholder
}

/// obj.toString() — returns "[object Object]".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_TO_STRING(_handle: u64) -> u64 {
    alloc_str_handle(b"[object Object]")
}

/// obj.hasOwnProperty(prop) — checks if object has own property.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY(handle: u64, prop_ptr: u64, prop_len: u64) -> i64 {
    use std::slice;
    let prop_bytes = unsafe { slice::from_raw_parts(prop_ptr as *const u8, prop_len as usize) };
    if let Ok(prop) = std::str::from_utf8(prop_bytes) {
        if let Some(map) = map_from_handle(handle) {
            return i64::from(map.contains_key(prop));
        }
    }
    0
}

