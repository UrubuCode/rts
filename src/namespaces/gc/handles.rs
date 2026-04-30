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

// Layout do handle (gen/slot/shard) e' compartilhado com `ui::store` via
// `crate::abi::handles` (#283). Mudancas aqui invalidam handles existentes.
use crate::abi::handles::{
    HANDLE_GEN_SHIFT as GEN_SHIFT, HANDLE_N_SHARDS as N_SHARDS,
    HANDLE_SHARD_BITS as SHARD_BITS, HANDLE_SHARD_MASK as SHARD_MASK,
    HANDLE_SLOT_MASK as SLOT_MASK,
};

const SENTINEL_INVALID: u64 = 0;

/// Value kinds stored behind a handle. Extensible as namespaces grow.
#[derive(Debug)]
pub enum Entry {
    /// UTF-8 string owned on the heap.
    String(Vec<u8>),
    /// Fixed-point decimal number, see `bigfloat::fixed::FixedDecimal`.
    ///
    /// Path uses `super::super` (gc's parent) to stay valid in both the
    /// main crate (`namespaces::bigfloat`) and the standalone runtime
    /// staticlib (`crate::bigfloat`).
    BigFixed(Box<super::super::bigfloat::fixed::FixedDecimal>),
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
    /// Regex compilada — namespace `regex`.
    Regex(Box<regex::Regex>),
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
    TlsClient(Box<super::super::tls::client::TlsClientStream>),
    /// JoinHandle<u64> owned — namespace `thread` (spawn/join/detach).
    /// Box pra estabilizar o endereco. Consumido por `join`/`detach`
    /// (substituido por `Free`).
    JoinHandle(Box<std::thread::JoinHandle<u64>>),
    /// Environment record para closures — Vec<i64> com slots por captura.
    /// Usado por `gc.env_*` para implementar capturas reais sem promote-
    /// to-global. Cada slot armazena um valor i64 (cobre int/handle/bool).
    Env(Vec<i64>),
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
    /// `Error` instance — message string + name tag.
    /// Created by `new Error(msg)` / `new TypeError(msg)` etc.
    ErrorObj { message: String, name: String },
    /// `EventEmitter` instance — Arc<Mutex<dyn Any+Send>> so the inner lock
    /// can be held independently of the shard lock. The concrete type is
    /// `globals::events::instance::EmitterData`; downcast at access sites.
    EventEmitter(std::sync::Arc<std::sync::Mutex<dyn std::any::Any + Send>>),
    /// EventEmitter primitivo do namespace `events` (rts:events). Armazena
    /// listeners por nome de evento como function pointers (i64 raw).
    /// Distinto do `EventEmitter` global acima — coexistem.
    RtsEventsEmitter(Box<RtsEventsEmitter>),
    /// Tombstone left by `free`. Reused on next `alloc` with a bumped
    /// generation so dangling handles fail validation.
    Free,
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
        _ => {}
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
    entry: Entry,
}

#[derive(Debug, Default)]
pub struct HandleTable {
    slots: Vec<Slot>,
    /// Indices of `Free` slots available for reuse.
    free_list: Vec<u32>,
}

impl HandleTable {
    /// Allocate `entry` in this shard. `shard_idx` is encoded in the low
    /// SHARD_BITS of the slot field so `shard_for_handle` can route back
    /// without extra metadata.
    pub fn alloc_in_shard(&mut self, entry: Entry, shard_idx: usize) -> u64 {
        if let Some(table_slot) = self.free_list.pop() {
            let slot = &mut self.slots[table_slot as usize];
            slot.generation = slot.generation.wrapping_add(1);
            slot.entry = entry;
            return encode(slot.generation, shard_idx, table_slot);
        }
        let table_slot = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 1,
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
        cleanup_entry(&mut slot.entry);
        slot.entry = Entry::Free;
        self.free_list.push(table_slot);
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
}

/// Allocates `entry` in the next shard (round-robin per thread).
/// The shard index is encoded in the returned handle so `shard_for_handle`
/// routes correctly without any extra lookup.
pub fn alloc_entry(entry: Entry) -> u64 {
    let shard_idx = ALLOC_SHARD.with(|s| {
        let v = s.get();
        s.set((v + 1) % N_SHARDS);
        v
    });
    shards()[shard_idx].lock().unwrap().alloc_in_shard(entry, shard_idx)
}

/// Frees a handle. Returns false if the handle is invalid or already freed.
pub fn free_handle(handle: u64) -> bool {
    shard_for_handle(handle).lock().unwrap().free(handle)
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
}
