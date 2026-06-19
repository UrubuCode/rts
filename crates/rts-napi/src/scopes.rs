//! Handle scopes N-API (Etapa 8).
//!
//! Um `napi_value` criado dentro do frame nativo de um addon **não** aparece no
//! stack map do Cranelift (o frame é código opaco ao RTS), então o GC o
//! coletaria no meio da chamada. Handle scopes resolvem isso: cada scope mantém
//! seus handles registrados como **GC roots extras** via `global_roots::add`.
//!
//! ## Por que chunks fixos em `Box`, não `Vec`
//!
//! `global_roots` registra o ENDEREÇO de um `u64` e o scanner lê `*(addr)`. Um
//! `Vec<u64>` realoca ao crescer → o endereço-base muda → o scanner leria
//! memória morta (UAF do handle vivo). Usamos `ScopeChunk { slots: [u64; N] }`
//! em `Box` (endereço estável); ao encher, encadeamos um novo chunk. Cada slot
//! usado é registrado individualmente (`&slots[i]`), e desregistrado ao fechar.
//!
//! Ver docs/specs/napi-implementation.md.

use rts_engine::collector::global_roots;

use crate::env::{value_from_handle, RtsNapiEnv};
use crate::types::{
    napi_env, napi_escapable_handle_scope, napi_handle_scope, napi_status, napi_value,
};

use napi_status::{napi_escape_called_twice, napi_invalid_arg, napi_ok};

/// Nº de slots por chunk. Boxed para endereço estável.
const CHUNK_SLOTS: usize = 32;

struct ScopeChunk {
    slots: Box<[u64; CHUNK_SLOTS]>,
    used: usize,
    next: Option<Box<ScopeChunk>>,
}

impl ScopeChunk {
    fn new() -> Box<Self> {
        Box::new(Self {
            slots: Box::new([0u64; CHUNK_SLOTS]),
            used: 0,
            next: None,
        })
    }
}

/// Um handle scope: lista encadeada de chunks. Cada slot registrado é um GC root.
pub struct Scope {
    head: Box<ScopeChunk>,
    /// `true` para escapable scope (permite `napi_escape_handle` uma vez).
    escapable: bool,
    /// Já houve um `escape` neste scope? (escapable só promove uma vez.)
    escaped: bool,
}

impl Scope {
    fn new(escapable: bool) -> Self {
        Self {
            head: ScopeChunk::new(),
            escapable,
            escaped: false,
        }
    }

    /// Grava `handle` no scope e o registra como GC root. Cresce com um novo
    /// chunk se o atual encher.
    fn add_handle(&mut self, handle: u64) {
        // Acha o último chunk com espaço (ou cria um novo).
        let mut chunk = self.head.as_mut();
        loop {
            if chunk.used < CHUNK_SLOTS {
                let idx = chunk.used;
                chunk.slots[idx] = handle;
                chunk.used += 1;
                let addr = &chunk.slots[idx] as *const u64 as usize;
                global_roots::add(addr);
                return;
            }
            if chunk.next.is_none() {
                chunk.next = Some(ScopeChunk::new());
            }
            chunk = chunk.next.as_mut().unwrap();
        }
    }

    /// Desregistra todos os roots deste scope (ao fechar).
    fn unregister_all(&self) {
        let mut chunk = Some(self.head.as_ref());
        while let Some(c) = chunk {
            for i in 0..c.used {
                let addr = &c.slots[i] as *const u64 as usize;
                global_roots::remove(addr);
            }
            chunk = c.next.as_deref();
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        self.unregister_all();
    }
}

/// Pilha de scopes de um `RtsNapiEnv`.
pub struct ScopeStack {
    stack: Vec<Scope>,
}

impl ScopeStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Registra um `napi_value`-handle no scope topo (se houver). Chamado por
    /// toda fn de criação ANTES de retornar o valor ao addon (anti-UAF).
    pub fn track(&mut self, handle: u64) {
        // Sentinelas (gen==0) não são GC handles — não rastreamos.
        if handle == 0 || is_sentinel(handle) {
            return;
        }
        if let Some(top) = self.stack.last_mut() {
            top.add_handle(handle);
        }
    }
}

impl Default for ScopeStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper para as fns de criação: registra `handle` no scope topo do `env`
/// (anti-UAF). Seguro com `env` nulo (caminho de teste) — no-op.
///
/// # Safety
/// `env` deve ser um `napi_env` válido produzido pelo loader, ou nulo.
pub unsafe fn track_in_env(env: napi_env, handle: u64) {
    if env.0.is_null() {
        return;
    }
    if let Some(e) = unsafe { RtsNapiEnv::from_raw(env) } {
        e.scopes.track(handle);
    }
}

fn is_sentinel(h: u64) -> bool {
    // gen é os 16 bits altos; sentinelas JS (i64::MIN..) têm gen != 0 mas
    // decodificam para slot inválido. Reusa a checagem de values.
    crate::values::is_sentinel(h)
}

// ── as 5 fns N-API ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_handle_scope(
    env: napi_env,
    result: *mut napi_handle_scope,
) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    e.scopes.stack.push(Scope::new(false));
    if !result.is_null() {
        // O "handle" do scope é o seu índice+1 (opaco ao addon).
        let idx = e.scopes.stack.len();
        unsafe { *result = napi_handle_scope(idx as *mut std::ffi::c_void) };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_handle_scope(
    env: napi_env,
    _scope: napi_handle_scope,
) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    // Scopes fecham em ordem inversa: pop do topo (Drop desregistra os roots).
    if e.scopes.stack.pop().is_none() {
        return napi_invalid_arg;
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_escapable_handle_scope(
    env: napi_env,
    result: *mut napi_escapable_handle_scope,
) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    e.scopes.stack.push(Scope::new(true));
    if !result.is_null() {
        let idx = e.scopes.stack.len();
        unsafe { *result = napi_escapable_handle_scope(idx as *mut std::ffi::c_void) };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_escapable_handle_scope(
    env: napi_env,
    _scope: napi_escapable_handle_scope,
) -> napi_status {
    unsafe { napi_close_handle_scope(env, napi_handle_scope(std::ptr::null_mut())) }
}

/// Promove `escapee` ao scope PAI (sobrevive ao fechamento do scope atual).
/// Só pode ser chamado uma vez por escapable scope.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_escape_handle(
    env: napi_env,
    _scope: napi_escapable_handle_scope,
    escapee: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let Some(e) = (unsafe { RtsNapiEnv::from_raw(env) }) else {
        return napi_invalid_arg;
    };
    let n = e.scopes.stack.len();
    if n < 1 {
        return napi_invalid_arg;
    }
    // Marca o topo como escaped (uma vez só).
    {
        let top = &mut e.scopes.stack[n - 1];
        if !top.escapable {
            return napi_invalid_arg;
        }
        if top.escaped {
            return napi_escape_called_twice;
        }
        top.escaped = true;
    }
    // Registra o handle no scope PAI (n-2), se houver.
    let handle = crate::env::handle_from_value(escapee);
    if n >= 2 {
        let parent = &mut e.scopes.stack[n - 2];
        parent.add_handle(handle);
    }
    if !result.is_null() {
        unsafe { *result = value_from_handle(handle) };
    }
    napi_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::{alloc_entry, Entry};
    use std::ptr;

    fn make_env() -> napi_env {
        Box::new(RtsNapiEnv::new(8)).into_raw()
    }

    #[test]
    fn open_track_close_registers_and_unregisters_roots() {
        global_roots::clear();
        let env = make_env();
        let mut scope = napi_handle_scope(ptr::null_mut());
        unsafe { napi_open_handle_scope(env, &mut scope) };

        // Cria > CHUNK_SLOTS handles para forçar múltiplos chunks.
        let e = unsafe { RtsNapiEnv::from_raw(env) }.unwrap();
        let n = CHUNK_SLOTS + 5;
        for _ in 0..n {
            let h = alloc_entry(Entry::String(b"x".to_vec()));
            e.scopes.track(h);
        }
        assert_eq!(global_roots::len(), n, "cada handle vira um root");

        unsafe { napi_close_handle_scope(env, scope) };
        assert_eq!(global_roots::len(), 0, "fechar desregistra todos os roots");
    }

    #[test]
    fn escape_promotes_to_parent() {
        global_roots::clear();
        let env = make_env();
        let e = unsafe { RtsNapiEnv::from_raw(env) }.unwrap();

        // Scope pai.
        let mut parent = napi_handle_scope(ptr::null_mut());
        unsafe { napi_open_handle_scope(env, &mut parent) };
        // Escapable filho.
        let mut child = napi_escapable_handle_scope(ptr::null_mut());
        unsafe { napi_open_escapable_handle_scope(env, &mut child) };

        let h = alloc_entry(Entry::String(b"survivor".to_vec()));
        e.scopes.track(h); // registrado no filho
        assert_eq!(global_roots::len(), 1);

        // Escapa para o pai.
        let mut out = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe { napi_escape_handle(env, child, value_from_handle(h), &mut out) },
            napi_ok
        );
        // Agora há 2 roots (filho + pai). Fecha o filho → resta 1 (no pai).
        assert_eq!(global_roots::len(), 2);
        unsafe { napi_close_escapable_handle_scope(env, child) };
        assert_eq!(global_roots::len(), 1, "handle escapado sobrevive no pai");

        unsafe { napi_close_handle_scope(env, parent) };
        assert_eq!(global_roots::len(), 0);
    }

    #[test]
    fn escape_twice_fails() {
        global_roots::clear();
        let env = make_env();
        let e = unsafe { RtsNapiEnv::from_raw(env) }.unwrap();
        let mut child = napi_escapable_handle_scope(ptr::null_mut());
        unsafe { napi_open_escapable_handle_scope(env, &mut child) };
        let h = alloc_entry(Entry::String(b"x".to_vec()));
        e.scopes.track(h);
        let mut out = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe { napi_escape_handle(env, child, value_from_handle(h), &mut out) },
            napi_ok
        );
        assert_eq!(
            unsafe { napi_escape_handle(env, child, value_from_handle(h), &mut out) },
            napi_escape_called_twice
        );
    }
}
