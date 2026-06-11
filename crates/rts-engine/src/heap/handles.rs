//! Slab-based handle table for runtime-managed values.
//!
//! Handles are opaque `u64` values. Layout:
//!
//! ```text
//! [63..48] generation (16 bits)
//! [47.. 5] per-shard table slot (43 bits)
//! [ 4.. 0] shard index (5 bits, log2(N_SHARDS))
//! ```
//!
//! Encoding the shard index in the low 5 bits of the slot field means
//! `shard_for_handle` is O(1) and allocation round-robin always routes
//! correctly: shard N only ever emits handles whose low bits equal N.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

// Layout do handle (gen/slot/shard) e' compartilhado com `ui::store` via
// `crate::abi::handles` (#283). Mudancas aqui invalidam handles existentes.
use crate::abi::handles::{
    HANDLE_GEN_SHIFT as GEN_SHIFT, HANDLE_N_SHARDS as N_SHARDS,
    HANDLE_SHARD_BITS as SHARD_BITS, HANDLE_SHARD_MASK as SHARD_MASK,
    HANDLE_SLOT_MASK as SLOT_MASK,
};

/// A TLS client stream stored in the HandleTable. Definição movida do namespace
/// `tls` pro motor (heap no engine); a lógica de I/O do `tls` referencia este
/// tipo via facade. Carrega `rustls::ClientConnection` → engine puxa rustls (TEMP;
/// Fase 2 troca por `Entry::Backend(dyn Traceable)` e devolve o payload pro backend).
pub struct TlsClientStream {
    pub conn: rustls::ClientConnection,
    pub tcp: std::net::TcpStream,
}

impl std::fmt::Debug for TlsClientStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsClientStream").finish_non_exhaustive()
    }
}

const SENTINEL_INVALID: u64 = 0;

/// Wrapper que armazena a regex compilada + flags JS canonicas.
/// O crate `regex` nao expoe flags pos-compile de forma uniforme; RTS
/// guarda flags do callsite para `re.flags/global/ignoreCase/multiline`.
/// (#1107) Engine de regex usado. `Fast` (crate `regex`, RE2) eh o
/// caminho rapido O(n) garantido; `Fancy` (crate `fancy-regex`) eh
/// usado quando o pattern tem features que RE2 nao suporta
/// (lookbehind/lookahead, backreferences).
#[derive(Debug, Clone)]
pub enum RegexEngine {
    Fast(regex::Regex),
    Fancy(fancy_regex::Regex),
}

/// Match agnostico ao engine: somente boundaries (start/end) e a string
/// matched. Suficiente para is_match, find, e iteracao top-level.
#[derive(Debug, Clone)]
pub struct EngineMatch {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Captures agnostico ao engine: lista de Option<(start, end, text)> em
/// ordem (grupo 0 = full match, grupos 1..N = capture groups). None
/// quando o grupo nao participou.
#[derive(Debug, Clone)]
pub struct EngineCaptures {
    pub groups: Vec<Option<EngineMatch>>,
}

impl RegexEngine {
    pub fn is_match(&self, s: &str) -> bool {
        match self {
            RegexEngine::Fast(r) => r.is_match(s),
            RegexEngine::Fancy(r) => r.is_match(s).unwrap_or(false),
        }
    }

    pub fn find(&self, s: &str) -> Option<EngineMatch> {
        match self {
            RegexEngine::Fast(r) => r.find(s).map(|m| EngineMatch {
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
            }),
            RegexEngine::Fancy(r) => r.find(s).ok().flatten().map(|m| EngineMatch {
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
            }),
        }
    }

    pub fn captures(&self, s: &str) -> Option<EngineCaptures> {
        match self {
            RegexEngine::Fast(r) => r.captures(s).map(|caps| EngineCaptures {
                groups: (0..caps.len())
                    .map(|i| caps.get(i).map(|m| EngineMatch {
                        start: m.start(),
                        end: m.end(),
                        text: m.as_str().to_string(),
                    }))
                    .collect(),
            }),
            RegexEngine::Fancy(r) => r.captures(s).ok().flatten().map(|caps| EngineCaptures {
                groups: (0..caps.len())
                    .map(|i| caps.get(i).map(|m| EngineMatch {
                        start: m.start(),
                        end: m.end(),
                        text: m.as_str().to_string(),
                    }))
                    .collect(),
            }),
        }
    }

    pub fn captures_all(&self, s: &str) -> Vec<EngineCaptures> {
        match self {
            RegexEngine::Fast(r) => r
                .captures_iter(s)
                .map(|caps| EngineCaptures {
                    groups: (0..caps.len())
                        .map(|i| caps.get(i).map(|m| EngineMatch {
                            start: m.start(),
                            end: m.end(),
                            text: m.as_str().to_string(),
                        }))
                        .collect(),
                })
                .collect(),
            RegexEngine::Fancy(r) => r
                .captures_iter(s)
                .filter_map(|res| res.ok())
                .map(|caps| EngineCaptures {
                    groups: (0..caps.len())
                        .map(|i| caps.get(i).map(|m| EngineMatch {
                            start: m.start(),
                            end: m.end(),
                            text: m.as_str().to_string(),
                        }))
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn find_all(&self, s: &str) -> Vec<EngineMatch> {
        match self {
            RegexEngine::Fast(r) => r
                .find_iter(s)
                .map(|m| EngineMatch {
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                })
                .collect(),
            RegexEngine::Fancy(r) => r
                .find_iter(s)
                .filter_map(|res| res.ok())
                .map(|m| EngineMatch {
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                })
                .collect(),
        }
    }

    /// Nomes dos capture groups (None para grupos nao-nomeados).
    /// Indices alinhados com `groups` de EngineCaptures.
    pub fn capture_names(&self) -> Vec<Option<String>> {
        match self {
            RegexEngine::Fast(r) => r
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect(),
            RegexEngine::Fancy(r) => r
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect(),
        }
    }

    pub fn captures_len(&self) -> usize {
        match self {
            RegexEngine::Fast(r) => r.captures_len(),
            RegexEngine::Fancy(r) => r.captures_len(),
        }
    }

    /// Source pattern como string (usado por `re.source`).
    pub fn source(&self) -> String {
        match self {
            RegexEngine::Fast(r) => r.as_str().to_string(),
            RegexEngine::Fancy(r) => r.as_str().to_string(),
        }
    }

    /// Replace primeiro match com `replacement` (sintaxe Rust-regex:
    /// `$N` para grupo numerico, `${name}` para named). Para Fancy, faz
    /// substituicao manual mas suporta apenas `${name}` / `$N` simples.
    pub fn replace_first(&self, s: &str, replacement: &str) -> String {
        match self {
            RegexEngine::Fast(r) => r.replace(s, replacement).into_owned(),
            RegexEngine::Fancy(_) => self.replace_n(s, replacement, 1),
        }
    }

    pub fn replace_all(&self, s: &str, replacement: &str) -> String {
        match self {
            RegexEngine::Fast(r) => r.replace_all(s, replacement).into_owned(),
            RegexEngine::Fancy(_) => self.replace_n(s, replacement, usize::MAX),
        }
    }

    fn replace_n(&self, s: &str, replacement: &str, limit: usize) -> String {
        let mut out = String::with_capacity(s.len());
        let mut last_end = 0usize;
        let mut count = 0usize;
        for caps in self.captures_all(s) {
            if count >= limit { break; }
            let m0 = match caps.groups.first().and_then(|o| o.clone()) {
                Some(m) => m,
                None => continue,
            };
            out.push_str(&s[last_end..m0.start]);
            out.push_str(&substitute_replacement(replacement, &caps));
            last_end = m0.end;
            count += 1;
        }
        out.push_str(&s[last_end..]);
        out
    }
}

/// Aplica substituicao no replacement string. Suporta:
/// - `$N` (N de 0-9) -> grupo numerico
/// - `${name}` -> grupo nomeado
/// - `$&` -> match completo
/// - `$$` -> literal `$`
fn substitute_replacement(repl: &str, caps: &EngineCaptures) -> String {
    let mut out = String::with_capacity(repl.len());
    let bytes = repl.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        if i + 1 >= bytes.len() {
            out.push('$');
            i += 1;
            continue;
        }
        let next = bytes[i + 1];
        if next == b'$' {
            out.push('$');
            i += 2;
        } else if next == b'&' {
            if let Some(Some(m)) = caps.groups.first() {
                out.push_str(&m.text);
            }
            i += 2;
        } else if next == b'{' {
            if let Some(end_rel) = repl[i + 2..].find('}') {
                let name = &repl[i + 2..i + 2 + end_rel];
                // tenta nome (em order) — fancy-regex nao tem name lookup
                // direto via EngineCaptures, entao recompila names.
                // Heuristica: index numerico OK; nome requer caller fornecer
                // tabela name->idx separadamente. Aqui ignoramos name lookup
                // (caller pre-processa ${name} -> $N quando precisa).
                if let Ok(n) = name.parse::<usize>() {
                    if let Some(Some(m)) = caps.groups.get(n) {
                        out.push_str(&m.text);
                    }
                }
                i = i + 2 + end_rel + 1;
            } else {
                out.push('$');
                i += 1;
            }
        } else if next.is_ascii_digit() {
            // $N (1 ou 2 digitos)
            let n_end = if i + 2 < bytes.len() && bytes[i + 2].is_ascii_digit() {
                i + 3
            } else {
                i + 2
            };
            let n: usize = repl[i + 1..n_end].parse().unwrap_or(0);
            if let Some(Some(m)) = caps.groups.get(n) {
                out.push_str(&m.text);
            }
            i = n_end;
        } else {
            out.push('$');
            i += 1;
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct RtsRegex {
    /// Mantido para compat com callsites que assumem `regex::Regex`.
    /// Quando o pattern requer fancy-regex, este campo eh inicializado
    /// com um placeholder vazio (`.*?` ou similar) e o `engine` carrega
    /// a versao fancy real. Callsites novos devem ler `engine`.
    pub regex: regex::Regex,
    /// (#1107) Engine real usado. Para patterns sem lookaround eh
    /// Fast(regex::Regex) (mesma instancia que `regex`); para patterns
    /// com lookaround eh Fancy(fancy_regex::Regex).
    pub engine: RegexEngine,
    pub global: bool,
    /// Flags JS canonicas em ordem (`d g i m s u y` apenas as setadas).
    pub flags: String,
    /// (#782) `lastIndex` JS — posicao para o proximo `exec`/`test` em
    /// regex global/sticky. Avancado pelo `exec`/`test` em regex `g`,
    /// resetado para 0 quando o match falha apos o final.
    pub last_index: usize,
}

/// Value kinds stored behind a handle. Extensible as namespaces grow.
#[derive(Debug)]
pub enum Entry {
    /// UTF-8 string owned on the heap.
    String(Vec<u8>),
    /// Fixed-point decimal number — `FixedDecimal` migrou pro heap do motor.
    BigFixed(Box<crate::heap::fixed::FixedDecimal>),
    /// Raw byte buffer — Vec<u8> com capacidade igual ao size.
    /// Usado pelo namespace `buffer` para dados binarios, FFI, etc.
    Buffer(Vec<u8>),
    /// Child process handle owned via std::process::Child — usado pelo
    /// namespace `process` para spawn/wait/kill.
    ProcessChild(Box<std::process::Child>),
    /// IndexMap<String, i64> — namespace `collections` (map_*).
    /// Valor i64 cobre inteiros, handles, e bool (0/1).
    /// IndexMap preserva ordem de inserção (necessário para ordem de
    /// enumeração JS: integer keys ascendentes + string keys em ordem de
    /// inserção). Ver `MAP_KEY_AT` para a lógica de ordenação.
    Map(Box<indexmap::IndexMap<String, i64>>),
    /// Vec<i64> — namespace `collections` (vec_*).
    Vec(Box<Vec<i64>>),
    /// Regex compilada — namespace `regex`. Armazena tambem a flag `global`
    /// (JS `/pat/g`) porque o crate `regex` nao expoe esse conceito separado.
    Regex(Box<RtsRegex>),
    /// CString owned — namespace `ffi` (cstring_*). Box pra estabilizar
    /// o ponteiro retornado por `cstring_ptr` enquanto o slot vive.
    CString(Box<std::ffi::CString>),
    /// OsString owned — namespace `ffi` (osstr_*).
    OsString(Box<std::ffi::OsString>),
    /// AtomicI64 owned — namespace `atomic` (i64_*). Box pra estabilizar
    /// o endereco enquanto o slot vive.
    AtomicI64(Box<std::sync::atomic::AtomicI64>),
    /// AtomicBool owned — namespace `atomic` (bool_*).
    AtomicBool(Box<std::sync::atomic::AtomicBool>),
    /// AtomicU64 backing an f64 via bit-transmute — namespace `atomic` (f64_*).
    /// Stored as AtomicU64 because Rust has no AtomicF64; ops use
    /// f64::to_bits / f64::from_bits.
    AtomicF64(Box<std::sync::atomic::AtomicU64>),
    /// Mutex<i64> owned — namespace `sync` (mutex_*). `Arc` permite que
    /// o guard armazenado no mapa thread-local mantenha um clone do
    /// Arc, garantindo que o Mutex viva enquanto houver guard, mesmo
    /// que o handle seja liberado antes do unlock (#280 — antes era
    /// `Box` + transmute para 'static, UB se free vinha antes de unlock).
    SyncMutex(std::sync::Arc<std::sync::Mutex<i64>>),
    /// RwLock<i64> owned — namespace `sync` (rwlock_*). Mesma logica de
    /// `Arc` que `SyncMutex`.
    SyncRwLock(std::sync::Arc<std::sync::RwLock<i64>>),
    /// OnceLock owned — namespace `sync` (once_*). Usa `std::sync::Once`
    /// internamente para executar fn_ptr exatamente uma vez.
    SyncOnce(Box<std::sync::Once>),
    /// TcpListener bound — namespace `net` (tcp_listen).
    TcpListener(Box<std::net::TcpListener>),
    /// TcpStream conectado — namespace `net` (tcp_accept/connect).
    TcpStream(Box<std::net::TcpStream>),
    /// UdpSocket bound — namespace `net` (udp_bind). Inclui slot pro
    /// ultimo peer observado em recv (udp_last_peer).
    UdpSocket(Box<UdpEntry>),
    /// TLS client stream — namespace `tls`. Wraps um TcpStream com
    /// rustls::ClientConnection. Criado por `tls.client(tcp_handle, sni)`
    /// que consome o handle do tcp.
    TlsClient(Box<TlsClientStream>),
    /// JoinHandle<u64> owned — namespace `thread` (spawn/join/detach).
    /// Box pra estabilizar o endereco. Consumido por `join`/`detach`
    /// (substituido por `Free`).
    JoinHandle(Box<std::thread::JoinHandle<u64>>),
    /// Environment record para closures — Vec<i64> com slots por captura.
    /// Usado por `gc.env_*` para implementar capturas reais sem promote-
    /// to-global. Cada slot armazena um valor i64 (cobre int/handle/bool).
    Env(Vec<i64>),
    /// Closure: par (fn_ptr, env_handle) para o refator #195 (env-record real).
    /// `fn_ptr` aponta para variante `__lifted_N__envabi` da fn liftada (que
    /// recebe `env: u64` como primeiro arg); `env` eh handle de outra entry
    /// `Entry::Env`. Construido por `gc.closure_alloc`. Ainda no-op para o
    /// codegen vivo — promote-to-global continua atendendo capturas atuais
    /// ate a Fase 2 do plano migrar arrows simples para este caminho.
    Closure { fn_ptr: i64, env: u64 },
    /// JSON value boxed — namespace `json`. serde_json::Value preserva
    /// distincao entre null/bool/number/string/array/object necessaria
    /// pro stringify nao virar lossy.
    Json(Box<serde_json::Value>),
    /// Instancia de classe com layout nativo (#147 — passo 4).
    /// `class` aponta pro handle do tag string `__rts_class`; `bytes`
    /// armazena os fields conforme o `ClassLayout` calculado em
    /// compile-time. Slot 0 é reservado para o tag mas armazenamos o
    /// class handle redundantemente em `class` para acesso O(1) sem
    /// decodificar o slot 0.
    Instance(Box<Instance>),
    /// `Date` instance — milliseconds since Unix epoch (UTC).
    /// Created by `new Date()` / `new Date(ms)` in the globals::date module.
    DateMs(i64),
    /// `Error` instance — message string + name tag + optional cause.
    /// Created by `new Error(msg)` / `new TypeError(msg, { cause })` etc.
    /// `cause` armazena handle do valor passado em options.cause, 0 = sem cause.
    ErrorObj { message: String, name: String, cause: u64 },
    /// `EventEmitter` instance — Arc<Mutex<dyn Any+Send>> so the inner lock
    /// can be held independently of the shard lock. The concrete type is
    /// `globals::events::instance::EmitterData`; downcast at access sites.
    EventEmitter(std::sync::Arc<std::sync::Mutex<dyn std::any::Any + Send>>),
    /// EventEmitter primitivo do namespace `events` (rts:events). Armazena
    /// listeners por nome de evento como function pointers (i64 raw).
    /// Distinto do `EventEmitter` global acima — coexistem.
    RtsEventsEmitter(Box<RtsEventsEmitter>),
    /// Promise<T> síncrona — valor já resolvido como i64 (handle ou primitivo).
    /// `.then(fn)` chama fn(value) imediatamente. `.catch(fn)` é passthrough.
    /// Caminho rápido para Promises que nasceram resolvidas (ex:
    /// `Promise.resolve(v)`).
    Promise(i64),
    /// Promise<T> assíncrona — slot com state pending/fulfilled/rejected
    /// e fila de waiters (oneshot tokio). Issue #412 / epic #411.
    /// Criada por `async function f()` quando o body bloqueia (await, IO,
    /// thread.spawn etc) ou explicitamente por `new Promise(executor)`.
    PromiseAsync(std::sync::Arc<PromiseSlot>),
    /// Response do `fetch()` — status HTTP + body bytes + URL final.
    HttpResponse(Box<HttpResponseData>),
    /// Function reificada — handle de uma fn invocavel via .call/.apply/.bind.
    /// `fn_ptr` aponta pro codigo (de `func_addr` ou compilado via eval).
    /// Sintetizada quando codegen ve member access em user fn ident, ou
    /// criada por `new Function("body")` via runtime.eval. Ver issue #359.
    Function(Box<FunctionData>),
    /// Symbol primitive (#216). `description` opcional. Cada `Symbol(...)`
    /// chamada cria handle unico — comparacao por identidade de handle.
    /// Symbol.for usa registry separado pra retornar mesmo handle.
    Symbol { description: Option<String> },
    /// WeakMap (#217 v0). v0 comporta como Map forte sem coleta automatica
    /// quando a key e' freed — Box<HashMap<u64,i64>> indexado por handle.
    WeakMap(Box<std::collections::HashMap<u64, i64>>),
    /// WeakSet (#217 v0). v0 comporta como Set forte sem coleta automatica.
    WeakSet(Box<std::collections::HashSet<u64>>),
    /// WeakRef (#685 v0). Armazena handle do target (strong ref por enquanto).
    /// `deref()` retorna o handle armazenado.
    WeakRef(u64),
    /// FinalizationRegistry (#685 v0). Stub — armazena callback handle e lista
    /// (target, heldValue) sem nunca disparar callback (sem GC weak real).
    FinalizationRegistry { callback: u64, entries: Vec<(u64, i64)> },
    /// Proxy (#218). `target` e' o objeto subjacente, `handler` e' um Map
    /// com traps `get`, `set`, `has`, `deleteProperty` (handles de Function
    /// reificadas). Quando ausente, MAP_GET_CHAIN/MAP_SET/etc fazem fallback
    /// direto pra target. Acesso transparente a Maps padrao via dispatch
    /// em `is_proxy(handle)`.
    Proxy { target: u64, handler: u64 },
    /// Streaming hasher (#289 node:crypto.createHash). State machine
    /// real do crate `sha2` — `update()` incremental, `finalize()` nao
    /// reprocessa buffer.
    Hasher(Box<HasherState>),
    /// `new Boolean(x)` boxed primitive (#879). Wraps a primitive bool so
    /// `typeof new Boolean(...)` returns "object" while `valueOf()` recovers
    /// the underlying bool. `Boolean(x)` (no `new`) keeps primitive path.
    BooleanBox(bool),
    /// (cross-runtime #244) `new String(x)` boxed primitive — wraps string
    /// handle so `typeof new String(...)` returns "object" enquanto
    /// `valueOf()` recupera o handle string original. `String(x)` (sem
    /// `new`) mantem caminho primitive.
    StringBox(u64),
    /// (cross-runtime #245) `new Number(x)` boxed primitive — wraps f64
    /// so `typeof new Number(...)` returns "object" while `valueOf()`
    /// recovers the underlying number. `Number(x)` (no `new`) keeps
    /// primitive path.
    NumberBox(f64),
    /// (#289) `Headers` instance — multimap case-insensitive key -> list
    /// of values. Headers.get junta com ", "; getSetCookie retorna a lista
    /// raw de "set-cookie" sem juntar.
    Headers(Box<indexmap::IndexMap<String, Vec<String>>>),
    /// (#477) Lazy generator state-machine. `fn_ptr` aponta para a fn de
    /// estado sintetizada pelo desugar (`extern "C" fn(u64) -> i64`, recebe o
    /// proprio handle do generator e devolve o valor yieldado/return). `state`
    /// eh o label de retomada (switch), `frame` os locais persistidos entre
    /// suspensoes, `ret` o valor de `return X` no corpo, `done` se o generator
    /// terminou. Diferente de `Entry::Vec` (eager-buffer): aqui o corpo SO'
    /// avanca ate o proximo yield a cada `.next()` (lazy real, suporta
    /// generators infinitos).
    GenState(Box<GenStateData>),
    /// Tombstone left by `free`. Reused on next `alloc` with a bumped
    /// generation so dangling handles fail validation.
    Free,
}

/// Estado de um generator lazy (`Entry::GenState`). Ver issue #477.
#[derive(Debug)]
pub struct GenStateData {
    /// Ponteiro da fn de estado: `extern "C" fn(u64) -> i64`.
    pub fn_ptr: u64,
    /// Label de retomada (switch sobre estado). Inicia em 0.
    pub state: i64,
    /// Locais persistidos entre suspensoes (slots indexados pelo desugar).
    pub frame: Vec<i64>,
    /// Valor de `return X` no corpo (UNDEFINED se ausente).
    pub ret: i64,
    /// Generator esgotado (proximo `.next()` => `{value:undefined,done:true}`).
    pub done: bool,
    /// (#477 fatia 2) Estado de entrada do `finally` da try-region ativa, ou
    /// -1 se nenhuma. Setado por `ENTER_TRY`, limpo por `END_FINALLY`. Permite
    /// `.return(v)`/`.throw(e)` redirecionarem para o finally em vez de so'
    /// terminar — o `yield` no finally intercepta/absorve a completion abrupta.
    pub finally_state: i64,
    /// Tipo de completion abrupta pendente: 0=nenhuma, 1=return, 2=throw.
    pub pending_kind: i64,
    /// Valor da completion abrupta pendente (ret value ou erro).
    pub pending_val: i64,
    /// (#207 async-SM) Este GenState eh uma `async function` (await=suspensao
    /// que cede a microtask queue), nao um generator (yield). Roteia
    /// SUSPEND/RESOLVE/AWAITED em vez de YIELD/DONE.
    pub is_async: bool,
    /// (#207) Promise resultado da async fn (resolvida quando o corpo termina
    /// ou rejeitada em throw). `None` ate ASYNC_SM_START alocar.
    pub result_promise: Option<std::sync::Arc<PromiseSlot>>,
    /// (#207) Promise que o `await` corrente esta esperando (setado por
    /// ASYNC_SM_SUSPEND). O drain enfileira AsyncResume sobre essa source.
    pub pending_await: Option<std::sync::Arc<PromiseSlot>>,
    /// (#207) Valor injetado pela retomada do await (settle da pending_await).
    pub awaited_val: i64,
    /// (#207) True se a promise awaited rejeitou (await deve relancar via
    /// error slot na retomada).
    pub awaited_rejected: bool,
    /// (#211 value-passing) Valor passado em `gen.next(v)`, injetado de volta
    /// como resultado do `yield` na retomada (`const x = yield ...` -> x = v).
    /// UNDEFINED quando `.next()` chamado sem argumento.
    pub sent: i64,
    /// (#211 try/catch) Estado de entrada do `catch` da try-region ativa, ou -1.
    /// Setado por ENTER_TRY_CATCH, limpo por EXIT_TRY_CATCH (saida normal do
    /// body) e ao despachar o throw. `.throw(e)` suspenso dentro da try salta
    /// para esse estado com `e` em `pending_val` (lido via CAUGHT).
    pub catch_state: i64,
    /// (cross-runtime #392) async generator (`async function*`): combina yield +
    /// await. `.next()` (AGEN_NEXT) bombeia ate o proximo yield/done e devolve
    /// Promise<{value,done}> ja' resolvida.
    pub is_async_gen: bool,
    /// (cross-runtime #392) Promise do `.next()` corrente do async gen, resolvida
    /// com `{value,done}` ao alcancar o proximo yield/done.
    pub next_promise: Option<std::sync::Arc<PromiseSlot>>,
}

/// State enum dos algoritmos suportados em `Entry::Hasher`. Wrap em Box
/// no Entry pra nao inflar o tamanho dos demais variants.
#[derive(Debug)]
pub enum HasherState {
    Sha256(sha2::Sha256),
}

#[derive(Debug)]
pub struct HttpResponseData {
    pub status: u16,
    pub url: String,
    pub body: Vec<u8>,
}

/// Dados de uma Function reificada (#359 / globals/function).
///
/// `fn_ptr` aponta pro codigo executavel — pode ser:
/// - endereco de user fn estatica (de `func_addr` em codegen),
/// - ou de fn compilada em runtime via `runtime.eval` (`new Function`).
///
/// `bound_this` e `bound_args` materializam `.bind()` (partial application).
/// Quando handle vem de bind, ao invocar via `.call` o trampolim usa
/// bound_this em vez do thisArg passado, e prepende bound_args.
///
/// `is_arrow` ignora thisArg em `.call/.apply` (spec arrow functions).
///
/// `keep_alive` mantem viva a JITModule de origem se for `new Function`.
/// Quando o ultimo handle Function for liberado, o module pode ser dropado.
#[derive(Debug)]
pub struct FunctionData {
    pub fn_ptr: u64,
    pub arity: u8,
    pub name: Box<str>,
    pub bound_this: i64,
    pub has_bound_this: bool,
    pub bound_args: Vec<i64>,
    pub is_arrow: bool,
    /// True quando a fn compilada tem `this` como primeiro parâmetro
    /// (método de classe não-estático). CALL/APPLY prepend effective_this
    /// antes de invoke_n quando este flag está ativo.
    pub has_this_param: bool,
    /// Tipos ABI dos parâmetros (codificação: 0=i64, 1=f64, 2=bool, 3=i32).
    /// Vazio = assume todos i64. Usado por invoke_n para coerção correta
    /// quando método tem `number` (f64) ou outros tipos não-i64.
    pub param_kinds: Vec<u8>,
    /// Tipo ABI do retorno: 0=i64, 1=f64, 2=bool, 3=i32, 4=void. 0 default.
    pub return_kind: u8,
    /// (#1281 packed) Endereco de um shim `extern "C" fn(*const i64, len) -> i64`
    /// que desempacota os args do buffer e chama a fn original (coercoes f64/i32
    /// embutidas em IR). 0 = sem shim (usa o caminho legado invoke_typed por
    /// aridade, teto 16). Quando != 0, o invoker usa invoke_packed — aridade
    /// arbitraria, portavel, sem teto. A fn original NUNCA muda de assinatura
    /// (chamadas diretas intactas); o shim eh uma fn sintetica separada.
    pub packed_shim: u64,
    pub source: Option<Box<str>>,
    /// Mantem viva a JITModule de origem se a fn veio de `new Function`
    /// (compilada em runtime). Mutex existe so' por Sync — JITModule e'
    /// Send mas nao Sync, e Entry precisa ser Sync pra atravessar shards.
    /// Nunca destravado em runtime — overhead zero no hot path.
    pub keep_alive: Option<std::sync::Arc<std::sync::Mutex<dyn std::any::Any + Send>>>,
    /// (#264) Handle do object usado como `fn.prototype` (constructor function).
    /// 0 = ainda nao alocado. Lazy alloc no primeiro acesso a member `prototype`
    /// via `__RTS_FN_GL_FUNCTION_PROTOTYPE_GET`. Eh um Map handle (collections)
    /// onde callers fazem `fn.prototype.method = handle`.
    pub prototype_handle: u64,
    /// (cross-runtime #195) Indice do parametro rest (`...args`) na lista de
    /// params DECLARADOS (capturas + fixos + rest). -1 = nao-variadic. Quando
    /// >= 0, o invoker (FUNCTION_CALL / INVOKE_AUTO) empacota `all_args[idx..]`
    /// num handle de array (Entry::Vec) antes do dispatch, pra que o corpo veja
    /// `rest` como UM Handle de array em vez de args soltos. Como as capturas
    /// sao prepended (`all_args = bound_args ++ reais`) e o rest e' sempre o
    /// ultimo param, `idx = arity - 1` cobre capturas+fixos automaticamente.
    /// Setado no reify de lambdas liftadas variadic; demais construtores: -1.
    pub rest_param_idx: i32,
}

/// Cleanup ativo de recursos do SO quando um Entry e' descartado (#279).
///
/// Nao usamos `impl Drop for Entry` para nao quebrar call sites que
/// movem variantes via `mem::replace(entry, Entry::Free)` + pattern
/// match (E0509). Em vez disso, esta funcao e' chamada explicitamente
/// em `HandleTable::free` antes de substituir o slot por `Free`, e
/// tambem percorrida no `Drop` do HandleTable inteiro.
///
/// Tipos cobertos:
/// - `ProcessChild`: drop padrao nao chama wait — gera zumbi ate o pai
///   morrer. Chamamos `try_wait` para reaproveitar o status sem
///   bloquear; se ainda nao terminou, deixamos o SO tratar.
/// - `TcpStream`/`TlsClient`: shutdown(Both) acorda peers em vez de
///   esperar timeout do TCP.
///
/// Demais tipos (Buffer, Map, Regex, Mutex, etc) liberam memoria
/// corretamente via Drop padrao do Box/Vec — nao precisam de logica
/// extra aqui.
fn cleanup_entry(entry: &mut Entry) {
    match entry {
        Entry::ProcessChild(child) => {
            let _ = child.try_wait();
        }
        Entry::TcpStream(stream) => {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        Entry::TlsClient(tls) => {
            let _ = tls.tcp.shutdown(std::net::Shutdown::Both);
        }
        // Nota (#264): Entry::Function.prototype_handle nao precisa de
        // cleanup explicito aqui — o GC scanner via mark_handle propaga
        // marca transitiva, entao o Map prototype eh coletado no proximo
        // sweep depois que a Function for coletada.
        _ => {}
    }
}

/// `Entry` é o payload concreto do collector (Fase 2 GC). Implementa o contrato
/// `crate::Traceable` — o protocolo que deixa o coletor genérico (a migrar
/// pro `rts-engine` no SPLIT) andar o grafo SEM conhecer as variants. As variants
/// pesadas (tokio/regex/rustls) ficam aqui no runtime; o engine fica zero-dep.
///
/// `trace_children` espelha exatamente o match de `HandleTable::mark`; `finalize`
/// reusa `cleanup_entry`. Comportamento idêntico ao mark+sweep atual.
impl crate::Traceable for Entry {
    fn trace_children(&self, visit: &mut dyn FnMut(u64)) {
        match self {
            // (#264) Function: prototype + bound_this + bound_args.
            Entry::Function(d) => {
                if d.prototype_handle != 0 {
                    visit(d.prototype_handle);
                }
                if d.has_bound_this && d.bound_this != 0 {
                    visit(d.bound_this as u64);
                }
                for v in &d.bound_args {
                    if *v != 0 {
                        visit(*v as u64);
                    }
                }
            }
            // (#398) Map values podem ser handles (string/map/vec/etc).
            Entry::Map(m) => {
                for v in m.values() {
                    if *v != 0 {
                        visit(*v as u64);
                    }
                }
            }
            // (#398) Vec elements idem.
            Entry::Vec(v) => {
                for h in v.iter() {
                    if *h != 0 {
                        visit(*h as u64);
                    }
                }
            }
            // (#218) Proxy: target + handler vivos enquanto proxy estiver vivo.
            Entry::Proxy { target, handler } => {
                if *target != 0 {
                    visit(*target);
                }
                if *handler != 0 {
                    visit(*handler);
                }
            }
            // (#477) Generator frame: slots podem ser handles vivos.
            Entry::GenState(g) => {
                for v in &g.frame {
                    if *v != 0 {
                        visit(*v as u64);
                    }
                }
                if g.ret != 0 {
                    visit(g.ret as u64);
                }
            }
            _ => {}
        }
    }

    fn finalize(&mut self) {
        cleanup_entry(self);
    }
}

impl Drop for HandleTable {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            cleanup_entry(&mut slot.entry);
        }
    }
}

/// Storage para `Entry::RtsEventsEmitter`. Listeners agrupados por nome
/// de evento; cada listener é um endereço de função (`func_addr` raw),
/// chamado via transmute → `extern "C" fn`.
#[derive(Debug, Default)]
pub struct RtsEventsEmitter {
    pub listeners: std::collections::HashMap<String, Vec<u64>>,
}

// `PromiseSlot` (state machine completo + waiters via tokio oneshot)
// vive em `crate::namespaces::gc::promise_slot` no main crate. Aqui
// `Entry::PromiseAsync` armazena `Arc<PromiseSlot>` por valor (Arc
// nao precisa de tokio em runtime_support — so' no main crate quando
// usar wait_blocking/resolve/reject que dependem de tokio::oneshot).
//
// Forward declaration: o struct esta em `promise_slot.rs` (main crate
// only). Aqui declaramos so' o tipo opaco — nao usamos seus metodos
// dentro do runtime_support.
pub struct PromiseSlot {
    /// 0=pending, 1=fulfilled, 2=rejected.
    pub state: std::sync::atomic::AtomicU8,
    pub value: std::sync::Mutex<i64>,
    /// Storage opaco pelo runtime_support — main crate define como
    /// `Mutex<Vec<tokio::sync::oneshot::Sender<(u8, i64)>>>`.
    /// Aqui guardamos so' como `Box<dyn Any + Send + Sync>` pra
    /// nao puxar tokio. Main crate downcasta no acesso.
    pub waiters: std::sync::Mutex<Box<dyn std::any::Any + Send + Sync>>,
}

impl std::fmt::Debug for PromiseSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.load(std::sync::atomic::Ordering::Acquire);
        let value = *self.value.lock().unwrap_or_else(|e| e.into_inner());
        f.debug_struct("PromiseSlot")
            .field("state", &state)
            .field("value", &value)
            .finish()
    }
}

/// UDP socket + ultimo peer observado em recv. Box estabiliza o
/// endereco. `last_peer` e None ate a primeira recv bem-sucedida.
#[derive(Debug)]
pub struct UdpEntry {
    pub socket: std::net::UdpSocket,
    pub last_peer: Option<std::net::SocketAddr>,
}

/// Instancia com layout nativo (#147). Armazenada em `Entry::Instance`.
#[derive(Debug)]
pub struct Instance {
    /// Handle do tag string `__rts_class` para a classe desta instancia.
    pub class: u64,
    /// Bytes do layout — tamanho determinado em compile-time pelo
    /// `ClassLayout`. Slot 0 (offset 0) reservado para o tag, demais
    /// slots para fields conforme `ClassLayout::fields`.
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
struct Slot {
    generation: u16,
    /// Set during GC mark phase. Cleared at sweep start and after each cycle.
    marked: bool,
    entry: Entry,
}

#[derive(Debug, Default)]
pub struct HandleTable {
    slots: Vec<Slot>,
    /// Indices of `Free` slots available for reuse.
    free_list: Vec<u32>,
}

/// Contador global de slots vivos cross-shard. Incrementado em cada
/// alloc novo (slot inedito) e decrementado em cada free. Reuso de
/// slot via free_list nao mexe no contador.
///
/// Usado para cap de seguranca: programas patologicos que alocam
/// strings/handles em loop sem GC (ex: codegen ainda nao emite
/// string_free) acabam vazando memoria sem limite. Cap converte
/// vazamento silencioso em diagnostico claro.
pub(crate) static LIVE_HANDLES: AtomicUsize = AtomicUsize::new(0);

/// Limite duro de handles vivos simultaneos. Cada handle (string/vec/map/
/// buffer/etc) custa entre dezenas de bytes (string curta) e MBs (buffer
/// grande). 5M handles = ~5GB no pior caso de strings curtas; o cap evita
/// passar disso e dar OOM no SO. Caso real de teste-suite passa muito
/// abaixo (dezenas a poucos milhares de handles vivos).
const HANDLES_MAX: usize = 5_000_000;

impl HandleTable {
    /// Allocate `entry` in this shard. `shard_idx` is encoded in the low
    /// SHARD_BITS of the slot field so `shard_for_handle` can route back
    /// without extra metadata.
    pub fn alloc_in_shard(&mut self, entry: Entry, shard_idx: usize) -> u64 {
        // Cap em slots vivos (alloc - free). Reuso via free_list ja
        // recuperou o slot anterior — incrementa de novo aqui pra
        // manter "live = total alloc - total free" simetrico.
        let prev = LIVE_HANDLES.fetch_add(1, Ordering::Relaxed);
        if prev >= HANDLES_MAX {
            LIVE_HANDLES.fetch_sub(1, Ordering::Relaxed);
            eprintln!(
                "RTS runtime: handle table exceeded limit of {HANDLES_MAX} live handles; aborting (likely string/array allocations in unbounded loop without GC — codegen does not emit auto-free yet)"
            );
            std::process::abort();
        }
        if let Some(table_slot) = self.free_list.pop() {
            let slot = &mut self.slots[table_slot as usize];
            slot.generation = slot.generation.wrapping_add(1);
            slot.entry = entry;
            return encode(slot.generation, shard_idx, table_slot);
        }
        let table_slot = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
            marked: false,
            entry,
        });
        encode(1, shard_idx, table_slot)
    }

    pub fn free(&mut self, handle: u64) -> bool {
        let Some((expected_gen, _, table_slot)) = decode(handle) else {
            return false;
        };
        let Some(slot) = self.slots.get_mut(table_slot as usize) else {
            return false;
        };
        if slot.generation != expected_gen {
            return false;
        }
        // Slot ja' liberado (gen bate, mas entry e' Free) — handle stale
        // mas nao double-free real. Nao decrementa pra evitar wrap-around
        // do counter atomico em cenarios de auto-free agressivo.
        if matches!(slot.entry, Entry::Free) {
            return false;
        }
        cleanup_entry(&mut slot.entry);
        slot.entry = Entry::Free;
        self.free_list.push(table_slot);
        LIVE_HANDLES.fetch_sub(1, Ordering::Relaxed);
        true
    }

    /// Resolve um handle ao seu Entry. Retorna None quando o handle eh
    /// invalido (sentinela, slot inexistente, gen nao bate, ja liberado).
    ///
    /// **Use-after-free safety (#203)**: o caller nunca recebe acesso a
    /// memoria de um Entry que foi liberado, mesmo que o slot tenha sido
    /// reutilizado por outra alocacao — a comparacao de generation
    /// invalida handles stale. Type confusion entre String/Buffer/etc
    /// fica impossivel: caller deve fazer pattern match em Entry::X
    /// e tratar mismatches como invalido.
    ///
    /// Todos os call sites em `src/namespaces/*/{ops,*.rs}` seguem o
    /// padrao `match table.get(h) { Some(Entry::Tag(...)) => ..., _ => fallback }`
    /// e nao usam `unwrap()` — verificado por audit em #203.
    pub fn get(&self, handle: u64) -> Option<&Entry> {
        let (expected_gen, _, table_slot) = decode(handle)?;
        let slot = self.slots.get(table_slot as usize)?;
        if slot.generation != expected_gen || matches!(slot.entry, Entry::Free) {
            return None;
        }
        Some(&slot.entry)
    }

    pub fn get_mut(&mut self, handle: u64) -> Option<&mut Entry> {
        let (expected_gen, _, table_slot) = decode(handle)?;
        let slot = self.slots.get_mut(table_slot as usize)?;
        if slot.generation != expected_gen || matches!(slot.entry, Entry::Free) {
            return None;
        }
        Some(&mut slot.entry)
    }

    /// Retorna handles de todos os slots vivos deste shard. Caller
    /// passa o `shard_idx` (que e' constante para o shard inteiro)
    /// pra reconstruir os handles. Usado pelo collector no sweep.
    pub fn live_handles_snapshot(&self, shard_idx: usize) -> Vec<u64> {
        let mut out = Vec::with_capacity(self.slots.len());
        for (idx, slot) in self.slots.iter().enumerate() {
            if matches!(slot.entry, Entry::Free) {
                continue;
            }
            out.push(encode(slot.generation, shard_idx, idx as u32));
        }
        out
    }

    /// Conta handles vivos (nao-Free) neste shard.
    pub fn live_handle_count(&self) -> usize {
        self.slots.iter().filter(|s| !matches!(s.entry, Entry::Free)).count()
    }

    /// Mark a handle as reachable (GC root). No-op for invalid/freed handles.
    /// Returns Vec<u64> com handles filhos para o caller propagar marca
    /// transitivamente. Cobre:
    /// - Function.prototype_handle (#264) + bound_this + bound_args
    /// - Map.values() (#398) — slot eh i64 raw OU handle dependendo do uso
    /// - Vec.elements (#398) — idem
    /// - Proxy { target, handler } (#218)
    /// Slots i64 raw em Map/Vec sao incluidos no worklist; mark_handle
    /// global filtra os que decodificam pra slot valido (handles reais).
    pub fn mark(&mut self, handle: u64) -> Vec<u64> {
        let mut children: Vec<u64> = Vec::new();
        let Some((expected_gen, _, table_slot)) = decode(handle) else { return children };
        let Some(slot) = self.slots.get_mut(table_slot as usize) else { return children };
        if slot.generation == expected_gen && !matches!(slot.entry, Entry::Free) {
            if crate::collector::debug::is_enabled()
                && matches!(slot.entry, Entry::TcpListener(_) | Entry::TcpStream(_))
            {
                eprintln!("[gc] MARK handle={handle:#x} slot={table_slot} kind=Tcp*");
            }
            slot.marked = true;
            // Enumera os filhos via o contrato `Traceable` (mesma lógica, agora
            // atrás da ABI do collector — Fase 2 GC). O coletor genérico do engine
            // chamará isto sem conhecer as variants concretas.
            crate::Traceable::trace_children(&slot.entry, &mut |c| children.push(c));
        }
        children
    }

    /// Sweep: free all unmarked live entries, then reset all mark bits.
    /// Returns number of handles freed.
    pub fn sweep_unmarked(&mut self) -> usize {
        let mut freed = 0;
        for (idx, slot) in self.slots.iter_mut().enumerate() {
            if matches!(slot.entry, Entry::Free) {
                continue;
            }
            if !slot.marked {
                if crate::collector::debug::is_enabled() {
                    let kind = match &slot.entry {
                        Entry::String(_) => "String",
                        Entry::Buffer(_) => "Buffer",
                        Entry::TcpListener(_) => "TcpListener",
                        Entry::TcpStream(_) => "TcpStream",
                        Entry::Map(_) => "Map",
                        Entry::Vec(_) => "Vec",
                        _ => "Other",
                    };
                    eprintln!("[gc] SWEEP slot={idx} kind={kind}");
                }
                cleanup_entry(&mut slot.entry);
                slot.entry = Entry::Free;
                self.free_list.push(idx as u32);
                LIVE_HANDLES.fetch_sub(1, Ordering::Relaxed);
                freed += 1;
            } else {
                slot.marked = false;
            }
        }
        freed
    }
}

/// Encodes generation + shard_idx + per-shard table_slot into a u64 handle.
fn encode(generation: u16, shard_idx: usize, table_slot: u32) -> u64 {
    let slot_field = ((table_slot as u64) << SHARD_BITS) | (shard_idx as u64 & SHARD_MASK);
    ((generation as u64) << GEN_SHIFT) | (slot_field & SLOT_MASK)
}

/// Decodes a handle into (generation, shard_idx, per-shard table_slot).
pub fn decode(handle: u64) -> Option<(u16, usize, u32)> {
    if handle == SENTINEL_INVALID {
        return None;
    }
    let generation = ((handle >> GEN_SHIFT) & 0xFFFF) as u16;
    let slot_field = handle & SLOT_MASK;
    let shard_idx = (slot_field & SHARD_MASK) as usize;
    let table_slot = (slot_field >> SHARD_BITS) as u32;
    Some((generation, shard_idx, table_slot))
}

// ── Sharded table ────────────────────────────────────────────────────────────

pub(crate) fn shards() -> &'static [Mutex<HandleTable>; N_SHARDS] {
    static SHARDS: OnceLock<[Mutex<HandleTable>; N_SHARDS]> = OnceLock::new();
    SHARDS.get_or_init(|| std::array::from_fn(|_| Mutex::new(HandleTable::default())))
}

/// Returns the shard that owns `handle`. O(1) via the shard_idx encoded
/// in the low SHARD_BITS of the slot field.
pub fn shard_for_handle(handle: u64) -> &'static Mutex<HandleTable> {
    let shard_idx = ((handle & SLOT_MASK) & SHARD_MASK) as usize;
    &shards()[shard_idx]
}

thread_local! {
    static ALLOC_SHARD: Cell<usize> = const { Cell::new(0) };
    /// Conta alocações por thread para GC automático periódico.
    static ALLOC_TICK: Cell<u32> = const { Cell::new(0) };
}

/// Frequência do GC automático: a cada N alocações o collector faz
/// um ciclo completo de mark+sweep. Calibrado para cobrir loops de
/// concat sem overhead significativo em workloads leves.
const GC_TICK_INTERVAL: u32 = 256;

/// Permite desligar o GC automatico setando RTS_GC_DISABLE=1.
/// Util para diagnosticar quando o sweep esta liberando handles
/// alcancaveis (bug de stack scan / safepoint cobertura).
fn gc_disabled() -> bool {
    std::env::var("RTS_GC_DISABLE").ok().as_deref() == Some("1")
}

/// Hook instalado por `collector::install_gc_hook()` para disparar
/// finish_cycle sem dependência circular handles → collector.
static GC_COLLECT_HOOK: OnceLock<fn()> = OnceLock::new();

/// Instala o hook de GC automático. Chamado uma vez por `collector` na
/// inicialização do runtime JIT.
pub fn install_gc_hook(f: fn()) {
    let _ = GC_COLLECT_HOOK.set(f);
}

/// Allocates `entry` in the next shard (round-robin per thread).
/// The shard index is encoded in the returned handle so `shard_for_handle`
/// routes correctly without any extra lookup.
///
/// Every `GC_TICK_INTERVAL` allocations triggers an automatic mark+sweep
/// cycle when the JIT stack map registry is active. This reclaims handles
/// that are no longer reachable from any JIT frame.
pub fn alloc_entry(entry: Entry) -> u64 {
    // Periodic GC: tick counter is thread-local (no atomic overhead).
    // We trigger BEFORE the new allocation so the allocation itself is
    // not yet visible to the collector (correct: not yet a root).
    let tick = ALLOC_TICK.with(|t| {
        let v = t.get().wrapping_add(1);
        t.set(v);
        v
    });
    if tick % GC_TICK_INTERVAL == 0 && !gc_disabled() {
        if let Some(f) = GC_COLLECT_HOOK.get() {
            f();
        }
    }

    let shard_idx = ALLOC_SHARD.with(|s| {
        let v = s.get();
        s.set((v + 1) % N_SHARDS);
        v
    });
    shards()[shard_idx]
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .alloc_in_shard(entry, shard_idx)
}

/// Mark a handle as reachable in the current GC cycle.
/// Propaga marca transitivamente para handles internos:
/// - Function.prototype_handle/bound_this/bound_args (#264)
/// - Map values, Vec elements (#398)
/// - Proxy.target / Proxy.handler (#218)
pub fn mark_handle(handle: u64) {
    let mut worklist = vec![handle];
    let mut steps = 0u32;
    while let Some(h) = worklist.pop() {
        if h == 0 {
            continue;
        }
        // Guard contra ciclos pathologicos (limite generoso).
        steps += 1;
        if steps > 1_000_000 {
            break;
        }
        let children = shard_for_handle(h)
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .mark(h);
        for child in children {
            worklist.push(child);
        }
    }
}

/// Sweep all shards: free unmarked entries and reset mark bits.
/// Returns total number of handles freed across all shards.
pub fn sweep_all_shards() -> usize {
    let mut total = 0;
    for shard in shards() {
        total += shard
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sweep_unmarked();
    }
    total
}

/// Frees a handle. Returns false if the handle is invalid or already freed.
pub fn free_handle(handle: u64) -> bool {
    shard_for_handle(handle)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .free(handle)
}

/// Immutable access to an entry. `f` receives `None` for invalid handles.
pub fn with_entry<R>(handle: u64, f: impl FnOnce(Option<&Entry>) -> R) -> R {
    if handle == 0 {
        return f(None);
    }
    let guard = shard_for_handle(handle)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    f(guard.get(handle))
}

/// Reads a string handle into an owned Rust `String` (`None` se não for
/// `Entry::String`). Helper puro sobre a heap — movido do `collector/string_pool`
/// do runtime pro motor pra que a camada universal (json/error) o use sem o
/// backend. Re-exportado em `collector/string_pool` pros call-sites antigos.
pub fn read_string_handle(handle: u64) -> Option<String> {
    with_entry(handle, |entry| match entry {
        Some(Entry::String(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    })
}

/// Mutable access to an entry. `f` receives `None` for invalid handles.
pub fn with_entry_mut<R>(handle: u64, f: impl FnOnce(Option<&mut Entry>) -> R) -> R {
    if handle == 0 {
        return f(None);
    }
    let mut guard = shard_for_handle(handle)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    f(guard.get_mut(handle))
}

/// Simultaneous immutable access to two entries.
pub fn with_two_entries<R>(
    ha: u64,
    hb: u64,
    f: impl FnOnce(Option<&Entry>, Option<&Entry>) -> R,
) -> R {
    if ha == 0 && hb == 0 {
        return f(None, None);
    }
    let sa = if ha == 0 {
        0
    } else {
        ((ha & SLOT_MASK) & SHARD_MASK) as usize
    };
    let sb = if hb == 0 {
        0
    } else {
        ((hb & SLOT_MASK) & SHARD_MASK) as usize
    };

    if ha != 0 && hb != 0 && sa == sb {
        let guard = shard_for_handle(ha)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        return f(guard.get(ha), guard.get(hb));
    }

    if sa <= sb {
        let ga = if ha == 0 {
            None
        } else {
            Some(
                shards()[sa]
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            )
        };
        let gb = if hb == 0 {
            None
        } else {
            Some(
                shards()[sb]
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            )
        };
        let ea = ga.as_ref().and_then(|g| g.get(ha));
        let eb = gb.as_ref().and_then(|g| g.get(hb));
        f(ea, eb)
    } else {
        let gb = Some(
            shards()[sb]
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        let ga = Some(
            shards()[sa]
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        let ea = ga.as_ref().and_then(|g| g.get(ha));
        let eb = gb.as_ref().and_then(|g| g.get(hb));
        f(ea, eb)
    }
}

/// Count of currently live handles (allocated minus freed).
pub fn live_handle_count() -> usize {
    LIVE_HANDLES.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_string_entry() {
        let h = alloc_entry(Entry::String(b"hello".to_vec()));
        let guard = shard_for_handle(h).lock().unwrap();
        assert!(matches!(guard.get(h), Some(Entry::String(b)) if b == b"hello"));
        drop(guard);
        assert!(free_handle(h));
        let guard2 = shard_for_handle(h).lock().unwrap();
        assert!(guard2.get(h).is_none());
    }

    #[test]
    fn stale_handle_rejected_after_reuse() {
        let h1 = alloc_entry(Entry::String(b"first".to_vec()));
        free_handle(h1);
        let h2 = alloc_entry(Entry::String(b"second".to_vec()));
        let g1 = shard_for_handle(h1).lock().unwrap();
        assert!(g1.get(h1).is_none(), "stale handle must not resolve");
        drop(g1);
        let g2 = shard_for_handle(h2).lock().unwrap();
        assert!(matches!(g2.get(h2), Some(Entry::String(_))));
    }

    /// #203: passar handle invalido pra get()/get_mut() retorna None,
    /// nunca acessa memoria liberada nem confunde tipos.
    #[test]
    fn invalid_handles_safe() {
        let table = HandleTable::default();
        // Handle 0 (sentinela)
        assert!(table.get(0).is_none());
        // Handle absurdo (slot fora do range, gen nunca alocado)
        assert!(table.get(0xDEAD_BEEF_DEAD_BEEF).is_none());
        // Bits altos zerados (gen=0 + slot inexistente)
        assert!(table.get(999_999).is_none());
    }

    /// #203: type confusion via stale handle e bloqueado.
    /// Free String, aloca Buffer no mesmo slot — stale handle pra String
    /// nao deve resolver (gen incrementada).
    #[test]
    fn type_confusion_via_stale_handle_blocked() {
        let h_str = alloc_entry(Entry::String(b"old".to_vec()));
        free_handle(h_str);
        // Aloca buffer logo apos — pode reusar o mesmo slot, mas com gen+1.
        let h_buf = alloc_entry(Entry::Buffer(vec![0u8; 16]));
        let guard = shard_for_handle(h_str).lock().unwrap();
        assert!(
            guard.get(h_str).is_none(),
            "stale handle nao deve resolver mesmo apos reuso do slot"
        );
        // h_buf e' um handle valido distinto.
        drop(guard);
        let g2 = shard_for_handle(h_buf).lock().unwrap();
        assert!(matches!(g2.get(h_buf), Some(Entry::Buffer(_))));
    }

    #[test]
    fn shard_encoding_is_consistent() {
        // Every handle allocated in shard N must route back to shard N.
        for expected_shard in 0..N_SHARDS {
            let h = shards()[expected_shard]
                .lock()
                .unwrap()
                .alloc_in_shard(Entry::Free, expected_shard);
            let actual_shard = ((h & SLOT_MASK) & SHARD_MASK) as usize;
            assert_eq!(actual_shard, expected_shard);
            free_handle(h);
        }
    }

    #[test]
    fn alloc_distributes_across_shards() {
        // alloc_entry round-robins shards; N_SHARDS consecutive allocs
        // from the same thread should hit all shards.
        let mut shard_indices = std::collections::HashSet::new();
        for _ in 0..N_SHARDS {
            let h = alloc_entry(Entry::Free);
            let shard = ((h & SLOT_MASK) & SHARD_MASK) as usize;
            shard_indices.insert(shard);
            free_handle(h);
        }
        assert_eq!(
            shard_indices.len(),
            N_SHARDS,
            "alloc should visit every shard in one round"
        );
    }

    // ── Leak detection ───────────────────────────────────────────────────────
    //
    // Testes de liveness verificam que o handle resolve antes do free e retorna
    // None depois — sem depender do contador global (que sofre corrida com
    // outros testes paralelos). O contador atomico e' testado via delta local.

    #[test]
    fn handle_dead_after_free() {
        let h = alloc_entry(Entry::String(b"leak?".to_vec()));
        with_entry(h, |e| assert!(e.is_some(), "handle nao vivo apos alloc"));
        free_handle(h);
        with_entry(h, |e| assert!(e.is_none(), "handle ainda vivo apos free — leak!"));
    }

    #[test]
    fn batch_alloc_free_all_handles_die() {
        let handles: Vec<u64> = (0..100)
            .map(|i| alloc_entry(Entry::String(format!("s{i}").into_bytes())))
            .collect();
        for &h in &handles {
            with_entry(h, |e| assert!(e.is_some(), "handle {h} nao vivo antes do free"));
        }
        for h in &handles {
            free_handle(*h);
        }
        for &h in &handles {
            with_entry(h, |e| assert!(e.is_none(), "handle {h} ainda vivo — leak!"));
        }
    }

    #[test]
    fn all_entry_variants_free_cleanly() {
        let handles = vec![
            alloc_entry(Entry::String(b"s".to_vec())),
            alloc_entry(Entry::Buffer(vec![0u8; 8])),
            alloc_entry(Entry::Vec(Box::new(vec![1i64, 2, 3]))),
            alloc_entry(Entry::Map(Box::new(indexmap::IndexMap::new()))),
            alloc_entry(Entry::Env(vec![0i64; 4])),
            alloc_entry(Entry::Json(Box::new(serde_json::Value::Null))),
            alloc_entry(Entry::Promise(42)),
            alloc_entry(Entry::DateMs(0)),
        ];
        for &h in &handles {
            with_entry(h, |e| assert!(e.is_some()));
        }
        for &h in &handles {
            free_handle(h);
        }
        for &h in &handles {
            with_entry(h, |e| assert!(e.is_none(), "variante vazou handle {h}"));
        }
    }

    #[test]
    fn double_free_does_not_underflow_live_count() {
        // LIVE_HANDLES is a global counter shared across all parallel tests.
        // We cannot assert exact absolute values; instead we verify that our
        // own alloc/free pair produces a net delta of zero and that double-free
        // does not decrement below the post-free baseline.
        let h = alloc_entry(Entry::String(b"x".to_vec()));
        let after_alloc = LIVE_HANDLES.load(Ordering::SeqCst);
        assert!(free_handle(h));
        let after_free = LIVE_HANDLES.load(Ordering::SeqCst);
        // The counter must have decreased by at least 1 (our entry) after free.
        assert!(
            after_free < after_alloc,
            "free did not decrement LIVE_HANDLES: after_alloc={after_alloc} after_free={after_free}"
        );
        assert!(!free_handle(h), "double-free deve retornar false");
        let after_double_free = LIVE_HANDLES.load(Ordering::SeqCst);
        // Double-free must not decrement below the post-free level.
        assert!(
            after_double_free >= after_free.saturating_sub(16),
            "double-free corrompeu LIVE_HANDLES: after_free={after_free} after_double_free={after_double_free}"
        );
    }

    #[test]
    fn free_invalid_handle_does_not_change_live_count() {
        let before = LIVE_HANDLES.load(Ordering::SeqCst);
        free_handle(0);
        free_handle(0xDEAD_BEEF_DEAD_BEEF);
        assert_eq!(LIVE_HANDLES.load(Ordering::SeqCst), before);
    }

    #[test]
    fn repeated_alloc_free_cycle_no_leak() {
        for _ in 0..1000 {
            let h = alloc_entry(Entry::Buffer(vec![0u8; 64]));
            with_entry(h, |e| assert!(e.is_some()));
            free_handle(h);
            with_entry(h, |e| assert!(e.is_none(), "cycle leak: handle {h} ainda vivo"));
        }
    }

    #[test]
    fn live_counter_tracks_alloc_free_delta() {
        // Captura delta local: isola o contador antes/depois das nossas operacoes.
        // Outros testes paralelos podem mudar o total, mas nosso delta deve ser exato.
        let before = LIVE_HANDLES.load(Ordering::SeqCst);
        let h = alloc_entry(Entry::DateMs(0));
        assert_eq!(LIVE_HANDLES.load(Ordering::SeqCst), before + 1);
        free_handle(h);
        assert_eq!(LIVE_HANDLES.load(Ordering::SeqCst), before);
    }
}
