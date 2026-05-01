//! `thread` namespace — ABI registration.
//!
//! ## Guia de escolha entre os 4 tipos de spawn
//!
//! | API                                  | Backend           | Retorno     | Spawn rate | Use quando                           |
//! |--------------------------------------|-------------------|-------------|-----------:|--------------------------------------|
//! | `spawn` + `join`/`detach`            | `std::thread`     | i64 via join|     30k/s  | CPU-bound longo, precisa do retorno  |
//! | `spawn_async_join` + `join_async`    | tokio blocking    | i64 via join|    ~400k/s | Tarefas leves/IO que retornam valor  |
//! | `spawn_async`                        | tokio blocking    | void        |    ~400k/s | Fire-and-forget leve, IO bound       |
//! | `spawn_detached`                     | pool fixo (8w)    | void        |   5000k/s  | Throughput max de submissao (cuidado: queue ilimitada) |
//!
//! ### Bench Monte Carlo Pi 10M (8 workers, CPU-bound)
//!
//! - `spawn` + `join` (std::thread):   30.3 ms  ⭐ vencedor em CPU-bound
//! - `spawn_async_join` (tokio):       34.3 ms  (~12% mais lento)
//! - Bun Workers (referencia):        147.6 ms
//!
//! ### Bench Spawn rate (10k spawns vazios)
//!
//! - `spawn`:           30k/s   (cria OS thread por chamada — ~30µs)
//! - `spawn_async`:    400k/s   (reusa pool blocking do tokio — ~2.5µs) ⭐
//! - `spawn_detached`: 5000k/s  (so' empurra job num Vec — 0.2µs, mas queue cresce sem limite)
//!
//! ### Quando preferir tokio (`spawn_async*`)
//!
//! - Handler HTTP por request (ja' tem runtime tokio vivo via http_server)
//! - Tarefas que aguardam IO (rede, disco) — outras tasks usam a thread enquanto bloqueia
//! - Muitas tarefas concorrentes (>100) — tokio escala melhor que criar thread por chamada
//!
//! ### Quando preferir `std::thread` (`spawn`)
//!
//! - CPU-bound longo (segundos+) — overhead de criar thread e' diluido
//! - Precisa controle fino: stack size, prioridade, nome de thread observavel no debugger
//! - Workload que mantem thread ocupada do inicio ao fim (Monte Carlo, processamento de imagem)
//!
//! ### Quando NAO usar `spawn_detached`
//!
//! - Quando a producao excede a capacidade dos 8 workers — queue cresce ilimitada
//!   ate OOM. Use `spawn_async` que tem pool elastico ate 512 default.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "spawn",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SPAWN",
        args: &[AbiType::U64, AbiType::U64],
        returns: AbiType::Handle,
        doc: "Cria uma nova OS thread (std::thread::spawn) executando `fn_ptr(arg)`. `fn_ptr` e um ponteiro para `extern \"C\" fn(u64) -> u64`. Retorna handle do JoinHandle, 0 em falha. Custo: ~30µs por chamada (cria thread Win32/pthread). Use para CPU-bound longo onde overhead e' diluido. Para tarefas leves use `spawn_async`/`spawn_detached`.",
        ts_signature: "spawn(fn_ptr: number, arg: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "spawn_async",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SPAWN_ASYNC",
        args: &[AbiType::U64, AbiType::U64],
        returns: AbiType::Void,
        doc: "Submete `fn_ptr(arg)` ao runtime tokio compartilhado via `tokio::spawn_blocking` (issue #399). Custo ~2.5µs por chamada — pool elastico de threads (max 512 default) cresce/encolhe sob demanda. Sem retorno: fire-and-forget. Use para tarefas leves, IO-bound, ou quando ja' existe runtime tokio vivo (ex: dentro de handler http_server). Para CPU-bound longo prefira `spawn`. Para retorno de valor use `spawn_async_join`.",
        ts_signature: "spawn_async(fn_ptr: number, arg: number): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "spawn_async_join",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN",
        args: &[AbiType::U64, AbiType::U64],
        returns: AbiType::U64,
        doc: "Como `spawn_async` mas retorna um id u64 que pode ser passado pra `join_async` para aguardar e pegar o valor de retorno. Bench Monte Carlo Pi 10M com 8 workers: 34.3ms (vs 30.3ms do `spawn`+`join` baseado em std::thread — ~12% mais lento em CPU-bound). Use quando precisar do retorno e o workload e' leve/IO-bound; em CPU-bound longo prefira `spawn`.",
        ts_signature: "spawn_async_join(fn_ptr: number, arg: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "join_async",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_JOIN_ASYNC",
        args: &[AbiType::U64],
        returns: AbiType::U64,
        doc: "Aguarda task spawnada via `spawn_async_join` terminar e retorna seu valor (rt.block_on do JoinHandle armazenado). Bloqueia a thread chamadora ate o resultado. Consome o id — chamar 2x retorna 0. 0 tambem se id invalido ou panic na task. Para tasks de std::thread use `join`.",
        ts_signature: "join_async(id: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "spawn_detached",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SPAWN_DETACHED",
        args: &[AbiType::U64, AbiType::U64],
        returns: AbiType::Void,
        doc: "Submete `fn_ptr(arg)` num pool de threads pre-criadas (default 8 workers, env `RTS_THREAD_POOL_SIZE`). Custo de submissao: 0.2µs (so' Vec.push + Condvar.notify). ATENCAO: a queue interna nao tem limite — se a producao excede a capacidade dos 8 workers a memoria cresce ate OOM. Para fire-and-forget seguro com pool elastico (max 512 default) prefira `spawn_async`. Spawn rate medido em bench: 5M/s (vs 400k/s do spawn_async, vs 30k/s do spawn). Sem retorno.",
        ts_signature: "spawn_detached(fn_ptr: number, arg: number): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "spawn_with_ud",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SPAWN_WITH_UD",
        args: &[AbiType::U64, AbiType::U64, AbiType::U64],
        returns: AbiType::Handle,
        doc: "Variante de `spawn` que passa um `userdata` (ex: handle de `this`) como primeiro argumento da fn. `fn_ptr` aponta para `extern \"C\" fn(u64, u64) -> u64` (ud, arg).",
        ts_signature: "spawn_with_ud(fn_ptr: number, arg: number, userdata: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "scope",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SCOPE",
        args: &[AbiType::U64],
        returns: AbiType::Void,
        doc: "Roda `body()` num escopo que aguarda automaticamente todas as threads spawnadas durante sua execucao. Analogo a `std::thread::scope`.",
        ts_signature: "scope(body: () => void): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "scope_with_ud",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SCOPE_WITH_UD",
        args: &[AbiType::U64, AbiType::U64],
        returns: AbiType::Void,
        doc: "Variante com userdata para `thread.scope` quando o body captura `this`.",
        ts_signature: "scope_with_ud(body: number, userdata: number): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "join",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_JOIN",
        args: &[AbiType::U64],
        returns: AbiType::U64,
        doc: "Aguarda a thread terminar e retorna o valor retornado por ela. Consome o handle. 0 se handle invalido ou a thread fez panic.",
        ts_signature: "join(thread: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "detach",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_DETACH",
        args: &[AbiType::U64],
        returns: AbiType::Void,
        doc: "Libera o JoinHandle sem aguardar. A thread continua rodando ate completar.",
        ts_signature: "detach(thread: number): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "id",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_ID",
        args: &[],
        returns: AbiType::U64,
        doc: "Id da thread atual (estavel por thread, atribuido na primeira chamada). Sempre != 0.",
        ts_signature: "id(): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "sleep_ms",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SLEEP_MS",
        args: &[AbiType::I64],
        returns: AbiType::Void,
        doc: "Pausa a thread atual por `ms` milissegundos. Valores negativos sao tratados como 0.",
        ts_signature: "sleep_ms(ms: number): void",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "thread",
    doc: "Primitivas de threads (spawn/join/detach/scope/id/sleep) baseadas em std::thread.",
    members: MEMBERS,
};
