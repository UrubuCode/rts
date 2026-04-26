//! Environment records para closures — fase 1 da #195.
//!
//! Cada captura de closure vira um slot i64 num env record alocado via
//! `HandleTable`. A fn lifted recebe o handle como param e lê/escreve via
//! `env_get`/`env_set`. Permite capturas re-entrantes e per-iteração de
//! loop sem o esquema promote-to-global que tem limitações estruturais.

use super::handles::{table, Entry};

/// Aloca um env record com `slot_count` slots, todos inicializados em 0.
/// Retorna o handle. `slot_count` negativo ou maior que 2^16 é tratado
/// como erro (handle 0).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_ALLOC(slot_count: i32) -> u64 {
    if slot_count < 0 || slot_count > 65536 {
        return 0;
    }
    let slots = vec![0i64; slot_count as usize];
    table().lock().unwrap().alloc(Entry::Env(slots))
}

/// Lê o valor de um slot. Retorna 0 em caso de handle inválido ou slot
/// fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_GET(env: u64, slot: i32) -> i64 {
    if slot < 0 {
        return 0;
    }
    let table = table().lock().unwrap();
    let Some(Entry::Env(slots)) = table.get(env) else {
        return 0;
    };
    slots.get(slot as usize).copied().unwrap_or(0)
}

/// Escreve um valor em um slot. Retorna 1 em sucesso, 0 em handle
/// inválido ou slot fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_SET(env: u64, slot: i32, value: i64) -> i64 {
    if slot < 0 {
        return 0;
    }
    let mut table = table().lock().unwrap();
    let Some(Entry::Env(slots)) = table.get_mut(env) else {
        return 0;
    };
    let Some(cell) = slots.get_mut(slot as usize) else {
        return 0;
    };
    *cell = value;
    1
}

/// Libera o env record. Retorna 1 em sucesso, 0 se handle já era inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_ENV_FREE(env: u64) -> i64 {
    if table().lock().unwrap().free(env) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_get_set_roundtrip() {
        let env = __RTS_FN_NS_GC_ENV_ALLOC(4);
        assert_ne!(env, 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 0), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_SET(env, 2, 42), 1);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 2), 42);
        assert_eq!(__RTS_FN_NS_GC_ENV_FREE(env), 1);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 2), 0);
    }

    #[test]
    fn out_of_range_slot_returns_zero() {
        let env = __RTS_FN_NS_GC_ENV_ALLOC(2);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, 5), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_SET(env, 5, 99), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(env, -1), 0);
        __RTS_FN_NS_GC_ENV_FREE(env);
    }

    #[test]
    fn invalid_handle_safe() {
        assert_eq!(__RTS_FN_NS_GC_ENV_GET(0, 0), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_SET(999_999, 0, 1), 0);
        assert_eq!(__RTS_FN_NS_GC_ENV_FREE(0), 0);
    }
}
