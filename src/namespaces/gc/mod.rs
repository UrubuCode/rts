//! `gc` namespace — deterministic GC exposed to TypeScript via `"rts"`.
//!
//! Provides arena-based allocation (`gc.alloc`), handle release (`gc.free`),
//! explicit collection (`gc.collect`, `gc.collect_debt`), and diagnostics
//! (`gc.stats`).
//!
//! The GC is also driven automatically by the runtime: `enter_scope` /
//! `exit_scope` / `safe_collect` are called at JS execution boundaries
//! (function call, class method, closure) without requiring TS code to
//! manage the GC manually.

pub mod arena;
pub mod collect;

pub use arena::{
    GcBlob, GcStats, KIND_ARRAY, KIND_BOOL, KIND_BYTES, KIND_NULL, KIND_NUMBER, KIND_OBJECT,
    KIND_STRING,
};
pub use collect::{enter_scope, exit_scope, notify_alloc, safe_collect, scope_depth};

use crate::namespaces::value::RuntimeValue;
use crate::namespaces::{
    DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_u8, arg_to_u64,
};

// ── Spec ────────────────────────────────────────────────────────────────────

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "alloc",
        callee: "gc.alloc",
        doc: "Allocate a tagged blob into the GC arena. Returns a u64 handle.",
        ts_signature: "alloc(kind: u8, payload: str): u64",
    },
    NamespaceMember {
        name: "free",
        callee: "gc.free",
        doc: "Release a handle, making the blob eligible for collection. Returns true if the handle was live.",
        ts_signature: "free(handle: u64): bool",
    },
    NamespaceMember {
        name: "collect",
        callee: "gc.collect",
        doc: "Full GC collection. Only call at a safe quiescence point (no live handles on stack).",
        ts_signature: "collect(): void",
    },
    NamespaceMember {
        name: "collect_debt",
        callee: "gc.collect_debt",
        doc: "Amortised GC — collect proportional to allocation debt. Safe to call at any time.",
        ts_signature: "collect_debt(): void",
    },
    NamespaceMember {
        name: "stats",
        callee: "gc.stats",
        doc: "Returns a JSON string with GC diagnostics: allocated_bytes, generation, live_slots.",
        ts_signature: "stats(): str",
    },
    NamespaceMember {
        name: "compact",
        callee: "gc.compact",
        doc: "Compacta o ValueStore (abi), liberando slots nao referenciados por nenhum binding ativo. Chamar apenas em pontos de quiescencia, ex: entre requisicoes de um servidor HTTP. Retorna o numero de slots liberados.",
        ts_signature: "compact(): i64",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "gc",
    doc: "Deterministic garbage collector (gc-arena). Arena-based allocation with safe collection at quiescence points after function/class/closure execution.",
    members: MEMBERS,
    ts_prelude: &[],
};

// ── Dispatch ────────────────────────────────────────────────────────────────

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    let outcome = match callee {
        "gc.alloc" => {
            let kind = arg_to_u8(args, 0);
            let payload = arg_to_string(args, 1);
            let blob = GcBlob::new(kind, payload.into_bytes());
            let handle = arena::alloc(blob);
            collect::notify_alloc();
            DispatchOutcome::Value(RuntimeValue::Number(handle as f64))
        }

        "gc.free" => {
            let handle = arg_to_u64(args, 0);
            DispatchOutcome::Value(RuntimeValue::Bool(arena::free(handle)))
        }

        "gc.collect" => {
            collect::safe_collect();
            DispatchOutcome::Value(RuntimeValue::Undefined)
        }

        "gc.collect_debt" => {
            arena::collect_debt();
            DispatchOutcome::Value(RuntimeValue::Undefined)
        }

        "gc.stats" => {
            let s = arena::stats();
            let json = format!(
                r#"{{"allocated_bytes":{},"generation":{},"live_slots":{}}}"#,
                s.allocated_bytes, s.generation, s.live_slots,
            );
            DispatchOutcome::Value(RuntimeValue::String(json))
        }

        "gc.compact" => {
            let (freed, _total) = crate::namespaces::abi::compact_value_store();
            DispatchOutcome::Value(RuntimeValue::Number(freed as f64))
        }

        _ => return None,
    };

    Some(outcome)
}
