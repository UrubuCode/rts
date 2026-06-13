//! Referências persistentes N-API (Etapa 9).
//!
//! Um `napi_ref` mantém um valor vivo ALÉM do handle scope (ex.: um constructor
//! guardado entre chamadas). Refcount:
//! - **strong** (refcount > 0): conta como GC root — o valor não é coletado.
//! - **weak** (refcount == 0): NÃO conta como root; `get_reference_value`
//!   devolve `undefined` se o alvo já foi coletado.
//!
//! Strong roots usam um `Box<u64>` de endereço estável registrado em
//! `global_roots` (o scanner lê `*(addr)`). Ao virar weak (unref → 0) o root é
//! removido; ao virar strong de novo (ref) é re-registrado.
//!
//! Ver docs/specs/napi-implementation.md.

use rts_engine::collector::global_roots;
use rts_engine::heap::handles::with_entry;

use crate::env::{value_from_handle, RtsNapiEnv};
use crate::types::{napi_env, napi_ref, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

struct RefEntry {
    /// Handle do alvo. Mantido num `Box` para endereço estável (o root aponta
    /// para dentro dele).
    target: Box<u64>,
    refcount: u32,
    /// `true` se o `&*target` está registrado como GC root.
    rooted: bool,
}

impl RefEntry {
    fn root_addr(&self) -> usize {
        &*self.target as *const u64 as usize
    }
    fn set_strong(&mut self, strong: bool) {
        if strong && !self.rooted {
            global_roots::add(self.root_addr());
            self.rooted = true;
        } else if !strong && self.rooted {
            global_roots::remove(self.root_addr());
            self.rooted = false;
        }
    }
}

/// Slab de referências (Vec + free list). `napi_ref` opaco = índice+1.
pub struct RefTable {
    entries: Vec<Option<RefEntry>>,
    free: Vec<usize>,
}

impl RefTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free: Vec::new(),
        }
    }

    fn insert(&mut self, target: u64, refcount: u32) -> usize {
        let mut entry = RefEntry {
            target: Box::new(target),
            refcount,
            rooted: false,
        };
        entry.set_strong(refcount > 0);
        let idx = if let Some(i) = self.free.pop() {
            self.entries[i] = Some(entry);
            i
        } else {
            self.entries.push(Some(entry));
            self.entries.len() - 1
        };
        idx
    }

    fn get_mut(&mut self, idx: usize) -> Option<&mut RefEntry> {
        self.entries.get_mut(idx).and_then(|o| o.as_mut())
    }

    fn remove(&mut self, idx: usize) -> bool {
        if let Some(slot) = self.entries.get_mut(idx) {
            if let Some(mut entry) = slot.take() {
                entry.set_strong(false); // desregistra root
                self.free.push(idx);
                return true;
            }
        }
        false
    }
}

impl Default for RefTable {
    fn default() -> Self {
        Self::new()
    }
}

/// `napi_ref` opaco ↔ índice (idx+1, para que 0 = nulo).
fn ref_to_idx(r: napi_ref) -> Option<usize> {
    let v = r.0 as usize;
    if v == 0 { None } else { Some(v - 1) }
}
fn idx_to_ref(idx: usize) -> napi_ref {
    napi_ref((idx + 1) as *mut std::ffi::c_void)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_reference(
    env: napi_env,
    value: napi_value,
    initial_refcount: u32,
    result: *mut napi_ref,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let target = crate::env::handle_from_value(value);
    let idx = e.refs.insert(target, initial_refcount);
    unsafe { *result = idx_to_ref(idx) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_reference(env: napi_env, ref_: napi_ref) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let Some(idx) = ref_to_idx(ref_) else {
        return napi_invalid_arg;
    };
    if e.refs.remove(idx) { napi_ok } else { napi_invalid_arg }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reference_ref(
    env: napi_env,
    ref_: napi_ref,
    result: *mut u32,
) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let Some(idx) = ref_to_idx(ref_) else {
        return napi_invalid_arg;
    };
    let Some(entry) = e.refs.get_mut(idx) else {
        return napi_invalid_arg;
    };
    entry.refcount += 1;
    entry.set_strong(true);
    if !result.is_null() {
        unsafe { *result = entry.refcount };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reference_unref(
    env: napi_env,
    ref_: napi_ref,
    result: *mut u32,
) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let Some(idx) = ref_to_idx(ref_) else {
        return napi_invalid_arg;
    };
    let Some(entry) = e.refs.get_mut(idx) else {
        return napi_invalid_arg;
    };
    if entry.refcount == 0 {
        return napi_invalid_arg; // unref abaixo de 0
    }
    entry.refcount -= 1;
    if entry.refcount == 0 {
        entry.set_strong(false); // vira weak
    }
    if !result.is_null() {
        unsafe { *result = entry.refcount };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_reference_value(
    env: napi_env,
    ref_: napi_ref,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let Some(idx) = ref_to_idx(ref_) else {
        return napi_invalid_arg;
    };
    let Some(entry) = e.refs.get_mut(idx) else {
        return napi_invalid_arg;
    };
    let target = *entry.target;
    // Weak coletado → o handle não resolve mais → undefined.
    let alive = with_entry(target, |x| x.is_some());
    let out = if alive {
        target
    } else {
        (i64::MIN + 2) as u64 // undefined
    };
    unsafe { *result = value_from_handle(out) };
    napi_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::{alloc_entry, free_handle, Entry};
    use std::ptr;

    fn make_env() -> napi_env {
        Box::new(RtsNapiEnv::new(8)).into_raw()
    }

    #[test]
    fn strong_ref_is_root_weak_is_not() {
        global_roots::clear();
        let env = make_env();
        let h = alloc_entry(Entry::String(b"kept".to_vec()));

        // refcount 1 → strong → root.
        let mut r = napi_ref(ptr::null_mut());
        unsafe { napi_create_reference(env, value_from_handle(h), 1, &mut r) };
        assert_eq!(global_roots::len(), 1, "strong ref é root");

        // unref → 0 → weak → sem root.
        let mut cnt = 99u32;
        unsafe { napi_reference_unref(env, r, &mut cnt) };
        assert_eq!(cnt, 0);
        assert_eq!(global_roots::len(), 0, "weak ref não é root");

        // ref de novo → strong → root.
        unsafe { napi_reference_ref(env, r, &mut cnt) };
        assert_eq!(cnt, 1);
        assert_eq!(global_roots::len(), 1);

        // delete remove o root.
        unsafe { napi_delete_reference(env, r) };
        assert_eq!(global_roots::len(), 0);
    }

    #[test]
    fn get_reference_value_returns_target_then_undefined_after_collect() {
        global_roots::clear();
        let env = make_env();
        let h = alloc_entry(Entry::String(b"weak".to_vec()));
        // weak ref (refcount 0).
        let mut r = napi_ref(ptr::null_mut());
        unsafe { napi_create_reference(env, value_from_handle(h), 0, &mut r) };

        // Antes de coletar: devolve o alvo.
        let mut got = napi_value(ptr::null_mut());
        unsafe { napi_get_reference_value(env, r, &mut got) };
        assert_eq!(crate::env::handle_from_value(got), h);

        // Simula coleta (free do handle).
        free_handle(h);
        unsafe { napi_get_reference_value(env, r, &mut got) };
        assert_eq!(
            crate::env::handle_from_value(got),
            (i64::MIN + 2) as u64,
            "weak coletado → undefined"
        );
    }

    #[test]
    fn create_reference_zero_is_weak() {
        global_roots::clear();
        let env = make_env();
        let h = alloc_entry(Entry::String(b"x".to_vec()));
        let mut r = napi_ref(ptr::null_mut());
        unsafe { napi_create_reference(env, value_from_handle(h), 0, &mut r) };
        assert_eq!(global_roots::len(), 0, "refcount inicial 0 = weak");
    }
}
