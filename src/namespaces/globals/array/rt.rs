//! Array — runtime implementations.
//!
//! Muitas operacoes delegam para collections::vec, mas algumas precisam
//! de implementacao especifica aqui (shift, unshift, indexOf, includes, etc).

use crate::abi::handles::{StrHandle, alloc_str_handle, vec_from_handle, map_from_handle, alloc_vec_handle, modify_vec_handle};

/// Array() — creates an empty array (Vec handle).
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_NEW_EMPTY() -> u64 {
    crate::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW()
}

/// Array(handle) — creates array from iterable handle.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_NEW_FROM_HANDLE(iterable: u64) -> u64 {
    // Se for um Vec, clona
    if let Some(vec) = vec_from_handle(iterable) {
        alloc_vec_handle(&vec)
    } else {
        // Fallback: retorna vazio
        crate::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW()
    }
}

/// Array.from(iterable) — creates a new array from an iterable.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_FROM(iterable: u64) -> u64 {
    __RTS_FN_GL_ARRAY_NEW_FROM_HANDLE(iterable)
}

/// Array.isArray(value) — returns true if value is an array (Vec handle).
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_IS_ARRAY(value: u64) -> bool {
    vec_from_handle(value).is_some()
}

/// arr.shift() — removes and returns the first element.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_SHIFT(handle: u64) -> i64 {
    let mut result = 0i64;
    let mut removed = false;
    
    modify_vec_handle(handle, |vec| {
        if !vec.is_empty() {
            result = vec[0];
            vec.remove(0);
            removed = true;
        }
    });
    
    if !removed { 0 } else { result }
}

/// arr.unshift(value) — inserts element at the beginning.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_UNSHIFT(handle: u64, value: i64) {
    modify_vec_handle(handle, |vec| {
        vec.insert(0, value);
    });
}

/// arr.indexOf(value) — returns index of element or -1 if not found.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_INDEX_OF(handle: u64, value: i64) -> i64 {
    if let Some(vec) = vec_from_handle(handle) {
        match vec.iter().position(|&v| v == value) {
            Some(idx) => idx as i64,
            None => -1,
        }
    } else {
        -1
    }
}

/// arr.includes(value) — returns true if array includes the value.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_INCLUDES(handle: u64, value: i64) -> bool {
    if let Some(vec) = vec_from_handle(handle) {
        vec.contains(&value)
    } else {
        false
    }
}

/// arr.reverse() — reverses the array in place.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_REVERSE(handle: u64) {
    modify_vec_handle(handle, |vec| {
        vec.reverse();
    });
}

/// arr.slice(start, end) — returns a shallow copy from start to end.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_SLICE(handle: u64, start: i64, end: i64) -> u64 {
    if let Some(vec) = vec_from_handle(handle) {
        let len = vec.len() as i64;
        let start_idx = if start < 0 { (len + start).max(0) } else { start } as usize;
        let end_idx = if end < 0 { (len + end).max(0) } else { end } as usize;
        let end_idx = end_idx.min(len as usize);
        
        if start_idx >= end_idx || start_idx >= vec.len() {
            crate::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW()
        } else {
            let slice: Vec<i64> = vec[start_idx..end_idx].to_vec();
            alloc_vec_handle(&slice)
        }
    } else {
        crate::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW()
    }
}

/// arr.concat(other) — concatenates another array.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_CONCAT(handle: u64, other: u64) -> u64 {
    if let Some(vec) = vec_from_handle(handle) {
        let mut result = vec.clone();
        if let Some(other_vec) = vec_from_handle(other) {
            result.extend(other_vec.iter());
        }
        alloc_vec_handle(&result)
    } else {
        crate::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW()
    }
}

/// arr.flat(depth) — flattens nested arrays up to depth.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_FLAT(handle: u64, depth: i64) -> u64 {
    // Implementacao simplificada: se depth <= 0, retorna copia
    // Se depth > 0, tenta achatar um nivel por iteracao
    if let Some(vec) = vec_from_handle(handle) {
        if depth <= 0 {
            return alloc_vec_handle(&vec);
        }
        // Achata um nivel: se elemento for handle de vec, expande
        let mut result = Vec::new();
        for &val in vec.iter() {
            // Tenta ver se val é handle de vec
            if let Some(inner_vec) = vec_from_handle(val as u64) {
                result.extend(inner_vec.iter());
            } else {
                result.push(val);
            }
        }
        // Se depth > 1, recursivamente achata
        if depth > 1 {
            let flat_handle = alloc_vec_handle(&result);
            return __RTS_FN_GL_ARRAY_FLAT(flat_handle, depth - 1);
        }
        alloc_vec_handle(&result)
    } else {
        crate::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW()
    }
}

/// arr.sort(comparator) — sorts array. comparator=0 para natural.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_SORT(handle: u64, _comparator: i64) {
    modify_vec_handle(handle, |vec| {
        vec.sort();
    });
}

/// arr.find(predicate_fn_ptr) — returns first element matching predicate.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_FIND(handle: u64, predicate_fn: i64) -> i64 {
    if let Some(vec) = vec_from_handle(handle) {
        // Chama o predicado para cada elemento
        for &val in vec.iter() {
            // Predicate signature: fn(i64) -> bool (retorna i64 0/1)
            type PredFn = extern "C" fn(i64) -> i64;
            if predicate_fn != 0 {
                let pred = unsafe { std::mem::transmute::<i64, PredFn>(predicate_fn) };
                if pred(val) != 0 {
                    return val;
                }
            }
        }
    }
    0
}

/// arr.findIndex(predicate_fn_ptr) — returns index of first match.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_FIND_INDEX(handle: u64, predicate_fn: i64) -> i64 {
    if let Some(vec) = vec_from_handle(handle) {
        for (idx, &val) in vec.iter().enumerate() {
            type PredFn = extern "C" fn(i64) -> i64;
            if predicate_fn != 0 {
                let pred = unsafe { std::mem::transmute::<i64, PredFn>(predicate_fn) };
                if pred(val) != 0 {
                    return idx as i64;
                }
            }
        }
    }
    -1
}

/// arr.every(predicate_fn_ptr) — returns true if all match.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_EVERY(handle: u64, predicate_fn: i64) -> bool {
    if let Some(vec) = vec_from_handle(handle) {
        if vec.is_empty() {
            return true;
        }
        type PredFn = extern "C" fn(i64) -> i64;
        for &val in vec.iter() {
            if predicate_fn == 0 {
                return false;
            }
            let pred = unsafe { std::mem::transmute::<i64, PredFn>(predicate_fn) };
            if pred(val) == 0 {
                return false;
            }
        }
        true
    } else {
        true
    }
}

/// arr.some(predicate_fn_ptr) — returns true if any matches.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_ARRAY_SOME(handle: u64, predicate_fn: i64) -> bool {
    if let Some(vec) = vec_from_handle(handle) {
        type PredFn = extern "C" fn(i64) -> i64;
        for &val in vec.iter() {
            if predicate_fn != 0 {
                let pred = unsafe { std::mem::transmute::<i64, PredFn>(predicate_fn) };
                if pred(val) != 0 {
                    return true;
                }
            }
        }
    }
    false
}

