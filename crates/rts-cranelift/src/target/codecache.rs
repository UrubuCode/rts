//! The compiled-code cache: a function compiled once is not compiled again.
//!
//! # The model: relocation replay, not position-independent code
//!
//! Compiled code bakes in addresses that are only true in the process that chose
//! them -- the heap's base, the cell cursor, the run table. Making the code
//! position independent would put a load on every heap access. Instead those
//! three are lowered as *relocations* (see `lower::memory::baked` and
//! [`crate::mem::RegionBases::immediate_values`]): the machine code carries an
//! absolute relocation naming `rts.imm.base`, `rts.imm.cursor` or `rts.imm.runs`,
//! which `place_in_memory` resolves to THIS run's addresses exactly as it
//! resolves a runtime entry point. The hot path is the same `movabs` it was.
//!
//! A cached function is therefore its machine bytes plus its relocation list,
//! with every relocation target stored by NAME. On a hit the names are resolved
//! against this run's module -- callees, cache sites, entry points and the three
//! immediates alike -- and the bytes go to `define_function_bytes` exactly as a
//! freshly compiled function's would. Nothing is patched by hand: no pattern is
//! searched for in machine code, so a constant that happens to look like an
//! address cannot be mistaken for one.
//!
//! # The key
//!
//! `sha256(env || name || function)` truncated to 128 bits, where
//!
//! * `function` is everything lowering can read from the body
//!   ([`crate::ir::Function::write_cache_key`], exhaustive over its fields);
//! * `env` is everything else lowering and the code generator read: the
//!   executable's own identity (any rebuild of rts invalidates everything),
//!   the cache format, the code generator's version, ISA triple and flags
//!   (opt level, regalloc, verifier, CPU features), the calling convention, the
//!   heap's *shape* (addressing form, stride, selector bits, whether a cursor
//!   and a run table exist, a symbolic base's name -- never an address), every
//!   type layout, every function and signature in the registry, and every name,
//!   linkage and signature the module has declared (callees, cache sites, entry
//!   points, immediates).
//!
//! The env is whole-program on purpose. Adding a function changes it and so
//! recompiles everything; editing a function's body changes only that
//! function's key. That is the coarse-but-sound end of the trade, and the
//! finer one (hash only the callees a body names) is the next slice.
//!
//! A value that differs per run and is baked into the *IR* (rather than into
//! lowering) simply makes that function's key differ per run: a miss, never a
//! wrong hit.
//!
//! # What is not cached
//!
//! A function with a relocation this module cannot name and replay (a library
//! call, a known symbol, an offset into a function, or an architecture-specific
//! kind outside the table below) is compiled every time. The store is
//! best-effort: any read or write failure is a miss.
//!
//! # File format
//!
//! One file, `code-v1.bin`, in the cache directory: the 8 bytes `RTSJIT01`,
//! then entries of `key[16] | length u32 | check[16] | payload`, where `check` is
//! the first 16 bytes of `sha256(payload)` and is verified on every hit. A file
//! is replaced by writing a sibling and renaming over it, so a reader never sees
//! a half-written one; two processes writing at once can lose one writer's new
//! entries, never corrupt the file.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use cranelift_codegen::binemit::Reloc;
use cranelift_codegen::isa::{CallConv, TargetIsa};
use cranelift_module::{DataId, FuncId as MachineFuncId, ModuleDeclarations, ModuleReloc, ModuleRelocTarget};
use sha2::{Digest, Sha256};

use super::Emitted;
use crate::fault::{Fault, FaultKind, FaultTable, Position};
use crate::ir::{FuncId, FuncRegistry, Function, SigId};
use crate::mem::RegionBases;
use crate::observe::PositionMap;
use crate::symbols::{EntryTable, RtEntry};
use crate::types::TypeRegistry;

/// Bumped whenever the payload layout or the meaning of the key changes.
const FORMAT: u32 = 1;
const MAGIC: &[u8; 8] = b"RTSJIT01";
const FILE_NAME: &str = "code-v1.bin";
/// When the file would exceed this, the old entries are dropped. The only
/// eviction there is: entries are small and a stale one costs disk, not
/// correctness.
const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;

static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

/// Turns the cache on for this process, storing under `directory`.
///
/// Off unless somebody calls this -- tests and library users get exactly the
/// code generation they always did. `RTS_NO_CODE_CACHE` (any value) keeps it
/// off even when called, which is the way to A/B a program with and without.
pub fn enable_code_cache(directory: PathBuf) {
    if std::env::var_os("RTS_NO_CODE_CACHE").is_some() {
        return;
    }
    let _ = DIRECTORY.set(directory);
}

/// The cache directory, when the cache is on.
pub(super) fn directory() -> Option<&'static Path> {
    DIRECTORY.get().map(PathBuf::as_path)
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

pub(super) type Key = [u8; 16];

struct Entry {
    offset: usize,
    length: usize,
    check: [u8; 16],
}

/// Every entry in the file as it was when read.
pub(super) struct Store {
    data: Vec<u8>,
    index: HashMap<Key, Entry>,
}

impl Store {
    fn empty() -> Self {
        Store {
            data: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Reads the file; anything wrong with it yields what parsed cleanly.
    fn read(path: &Path) -> Self {
        let Ok(data) = std::fs::read(path) else {
            return Self::empty();
        };
        if data.len() < MAGIC.len() || &data[..MAGIC.len()] != MAGIC {
            return Self::empty();
        }
        let mut index = HashMap::new();
        let mut at = MAGIC.len();
        while at + 36 <= data.len() {
            let mut key = [0u8; 16];
            key.copy_from_slice(&data[at..at + 16]);
            let length = u32::from_le_bytes(data[at + 16..at + 20].try_into().unwrap()) as usize;
            let mut check = [0u8; 16];
            check.copy_from_slice(&data[at + 20..at + 36]);
            let offset = at + 36;
            if offset.checked_add(length).is_none_or(|end| end > data.len()) {
                break;
            }
            index.insert(
                key,
                Entry {
                    offset,
                    length,
                    check,
                },
            );
            at = offset + length;
        }
        Store { data, index }
    }

    /// The payload stored under `key`, if present and intact.
    fn get(&self, key: &Key) -> Option<&[u8]> {
        let entry = self.index.get(key)?;
        let payload = &self.data[entry.offset..entry.offset + entry.length];
        (digest(payload)[..16] == entry.check).then_some(payload)
    }
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn file_path(directory: &Path) -> PathBuf {
    directory.join(FILE_NAME)
}

static LOADED: Mutex<Option<Arc<Store>>> = Mutex::new(None);

fn load(directory: &Path) -> Arc<Store> {
    let mut slot = LOADED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    slot.get_or_insert_with(|| Arc::new(Store::read(&file_path(directory))))
        .clone()
}

/// Adds `fresh` entries to the file on disk. Best-effort.
fn save(directory: &Path, fresh: Vec<(Key, Vec<u8>)>) {
    if fresh.is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(directory);
    let path = file_path(directory);
    // Re-read rather than reuse what this process loaded: another process may
    // have added entries since, and rewriting from the stale copy would drop
    // them.
    let current = Store::read(&path);
    let added: usize = fresh.iter().map(|(_, payload)| payload.len() + 36).sum();
    let keep_old = current.data.len() + added <= MAX_FILE_BYTES;

    let mut out = Vec::with_capacity(current.data.len() + added + MAGIC.len());
    out.extend_from_slice(MAGIC);
    let mut written: std::collections::HashSet<Key> = std::collections::HashSet::new();
    let mut push = |out: &mut Vec<u8>, key: &Key, check: &[u8], payload: &[u8]| {
        if written.insert(*key) {
            out.extend_from_slice(key);
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(check);
            out.extend_from_slice(payload);
        }
    };
    for (key, payload) in &fresh {
        let check = digest(payload);
        push(&mut out, key, &check[..16], payload);
    }
    if keep_old {
        for (key, entry) in &current.index {
            let payload = &current.data[entry.offset..entry.offset + entry.length];
            push(&mut out, key, &entry.check, payload);
        }
    }

    let temporary = directory.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    if std::fs::write(&temporary, &out).is_ok() && std::fs::rename(&temporary, &path).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
}

// ---------------------------------------------------------------------------
// One compilation's view of the cache
// ---------------------------------------------------------------------------

/// What `compile_all` holds while it consults the cache.
pub(super) struct Run {
    env: Sha256,
    store: Arc<Store>,
    directory: &'static Path,
    functions: HashMap<String, u32>,
    data: HashMap<String, u32>,
}

/// A cached function that is ready to define.
pub(super) enum Lookup {
    Hit(Box<Emitted>),
    Miss(Key),
}

impl Run {
    /// Everything the key depends on beside the function itself. `None` when
    /// any of it cannot be established, which turns the cache off for this
    /// compilation rather than keying on a guess.
    pub(super) fn new(
        isa: &dyn TargetIsa,
        call_conv: CallConv,
        heap: Option<&RegionBases>,
        types: &TypeRegistry,
        funcs: &FuncRegistry,
        machine: &ModuleDeclarations,
    ) -> Option<Self> {
        let directory = directory()?;
        let mut text = String::with_capacity(64 * 1024);
        let _ = writeln!(text, "format {FORMAT}");
        let _ = writeln!(text, "exe {}", executable_identity()?);
        let _ = writeln!(text, "cranelift {}", cranelift_codegen::VERSION);
        let _ = writeln!(text, "triple {}", isa.triple());
        let _ = writeln!(text, "flags {}", isa.flags());
        for flag in isa.isa_flags() {
            let _ = writeln!(text, "isa {flag}");
        }
        let _ = writeln!(text, "call_conv {call_conv:?}");
        let _ = writeln!(text, "{}", heap_shape(heap?));
        let mut at = 0;
        while let Some(layout) = types.layout_at(at) {
            let _ = writeln!(text, "type {at} {layout:?}");
            at += 1;
        }
        for index in 0..funcs.len() {
            let _ = writeln!(text, "func {index} {:?}", funcs.signature_of(FuncId(index as u32)));
        }
        let mut index = 0;
        while let Some(signature) = funcs.signature(SigId(index)) {
            let _ = writeln!(text, "sig {index} {signature:?}");
            index += 1;
        }

        let mut functions = HashMap::new();
        let mut data = HashMap::new();
        for (id, decl) in machine.get_functions() {
            let _ = writeln!(
                text,
                "decl func {} {:?} {:?} {}",
                id.as_u32(),
                decl.name,
                decl.linkage,
                decl.signature
            );
            if let Some(name) = &decl.name {
                functions.insert(name.clone(), id.as_u32());
            }
        }
        for (id, decl) in machine.get_data_objects() {
            let _ = writeln!(
                text,
                "decl data {} {:?} {:?} {} {}",
                id.as_u32(),
                decl.name,
                decl.linkage,
                decl.writable,
                decl.tls
            );
            if let Some(name) = &decl.name {
                data.insert(name.clone(), id.as_u32());
            }
        }

        let mut env = Sha256::new();
        env.update(text.as_bytes());
        Some(Run {
            env,
            store: load(directory),
            directory,
            functions,
            data,
        })
    }

    /// Looks one function up, decoding it when it is there.
    pub(super) fn lookup(
        &self,
        id: FuncId,
        declared: MachineFuncId,
        name: &str,
        body: &Function,
    ) -> Lookup {
        let mut hasher = self.env.clone();
        hasher.update((name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        struct Sink<'a>(&'a mut Sha256);
        impl std::fmt::Write for Sink<'_> {
            fn write_str(&mut self, text: &str) -> std::fmt::Result {
                self.0.update(text.as_bytes());
                Ok(())
            }
        }
        let _ = body.write_cache_key(&mut Sink(&mut hasher));
        let full: [u8; 32] = hasher.finalize().into();
        let mut key = [0u8; 16];
        key.copy_from_slice(&full[..16]);

        if let Some(payload) = self.store.get(&key)
            && let Some(emitted) = self.decode(payload, id, declared)
        {
            return Lookup::Hit(Box::new(emitted));
        }
        Lookup::Miss(key)
    }

    /// Rebuilds a function from its payload, resolving every relocation target
    /// by name against THIS run's module. Any doubt is a miss.
    fn decode(&self, payload: &[u8], id: FuncId, declared: MachineFuncId) -> Option<Emitted> {
        let mut r = Reader { bytes: payload, at: 0 };
        let alignment = r.u64()?;
        let size = r.u64()? as usize;
        let length = r.u32()? as usize;
        let bytes = r.take(length)?.to_vec();

        let count = r.u32()? as usize;
        let mut relocs = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = r.u32()?;
            let kind = reloc_from_code(r.u8()?)?;
            let namespace = r.u8()?;
            let name_length = r.u16()? as usize;
            let name = std::str::from_utf8(r.take(name_length)?).ok()?;
            let addend = r.i64()?;
            let width = reloc_width(kind);
            if (offset as usize).checked_add(width)? > bytes.len() {
                return None;
            }
            let index = match namespace {
                0 => *self.functions.get(name)?,
                1 => *self.data.get(name)?,
                _ => return None,
            };
            relocs.push(ModuleReloc {
                offset,
                kind,
                name: ModuleRelocTarget::user(u32::from(namespace), index),
                addend,
            });
        }

        let count = r.u32()? as usize;
        let mut faults = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = r.u32()?;
            let kind = match r.u8()? {
                0 => FaultKind::Unreachable,
                1 => FaultKind::OutOfBounds,
                2 => FaultKind::DivideByZero,
                3 => FaultKind::Other(r.u8()?),
                _ => return None,
            };
            let position = Position(r.u32()?);
            faults.push(Fault {
                offset,
                kind,
                position,
            });
        }

        let count = r.u32()? as usize;
        let mut runs = Vec::with_capacity(count);
        for _ in 0..count {
            runs.push((r.u32()?, r.u32()?, r.u32()?));
        }

        let mut flags = [false; RtEntry::COUNT];
        for flag in &mut flags {
            *flag = r.u8()? != 0;
        }
        if r.at != payload.len() {
            return None;
        }

        Some(Emitted {
            id,
            declared,
            alignment,
            bytes,
            relocs,
            faults: FaultTable::from_sorted(faults),
            positions: PositionMap::from_raw_runs(runs),
            size,
            entries: EntryTable::from_flags(flags),
        })
    }

    /// Serializes a freshly compiled function, or `None` when it has a
    /// relocation this cache cannot replay.
    pub(super) fn encode(&self, machine: &ModuleDeclarations, emitted: &Emitted) -> Option<Vec<u8>> {
        let mut w = Vec::with_capacity(emitted.bytes.len() + 256);
        w.extend_from_slice(&emitted.alignment.to_le_bytes());
        w.extend_from_slice(&(emitted.size as u64).to_le_bytes());
        w.extend_from_slice(&(emitted.bytes.len() as u32).to_le_bytes());
        w.extend_from_slice(&emitted.bytes);

        w.extend_from_slice(&(emitted.relocs.len() as u32).to_le_bytes());
        for reloc in &emitted.relocs {
            let (namespace, name) = match reloc.name {
                ModuleRelocTarget::User { namespace: 0, index } => {
                    (0u8, machine.get_function_decl(MachineFuncId::from_u32(index)).name.as_deref()?)
                }
                ModuleRelocTarget::User { namespace: 1, index } => {
                    (1u8, machine.get_data_decl(DataId::from_u32(index)).name.as_deref()?)
                }
                _ => return None,
            };
            w.extend_from_slice(&reloc.offset.to_le_bytes());
            w.push(reloc_code(reloc.kind)?);
            w.push(namespace);
            w.extend_from_slice(&u16::try_from(name.len()).ok()?.to_le_bytes());
            w.extend_from_slice(name.as_bytes());
            w.extend_from_slice(&reloc.addend.to_le_bytes());
        }

        let faults: Vec<Fault> = emitted.faults.iter().collect();
        w.extend_from_slice(&(faults.len() as u32).to_le_bytes());
        for fault in faults {
            w.extend_from_slice(&fault.offset.to_le_bytes());
            match fault.kind {
                FaultKind::Unreachable => w.push(0),
                FaultKind::OutOfBounds => w.push(1),
                FaultKind::DivideByZero => w.push(2),
                FaultKind::Other(code) => {
                    w.push(3);
                    w.push(code);
                }
            }
            w.extend_from_slice(&fault.position.raw().to_le_bytes());
        }

        let runs = emitted.positions.raw_runs();
        w.extend_from_slice(&(runs.len() as u32).to_le_bytes());
        for (start, end, position) in runs {
            w.extend_from_slice(&start.to_le_bytes());
            w.extend_from_slice(&end.to_le_bytes());
            w.extend_from_slice(&position.to_le_bytes());
        }

        for flag in emitted.entries.flags() {
            w.push(u8::from(flag));
        }
        Some(w)
    }

    /// Writes what this compilation compiled.
    pub(super) fn finish(&self, fresh: Vec<(Key, Vec<u8>)>) {
        save(self.directory, fresh);
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
}

/// The relocation kinds a host-ISA function can carry that this cache replays.
/// Anything else makes the function uncacheable.
fn reloc_code(kind: Reloc) -> Option<u8> {
    Some(match kind {
        Reloc::Abs4 => 1,
        Reloc::Abs8 => 2,
        Reloc::X86PCRel4 => 3,
        Reloc::X86CallPCRel4 => 4,
        Reloc::X86CallPLTRel4 => 5,
        Reloc::X86GOTPCRel4 => 6,
        _ => return None,
    })
}

fn reloc_from_code(code: u8) -> Option<Reloc> {
    Some(match code {
        1 => Reloc::Abs4,
        2 => Reloc::Abs8,
        3 => Reloc::X86PCRel4,
        4 => Reloc::X86CallPCRel4,
        5 => Reloc::X86CallPLTRel4,
        6 => Reloc::X86GOTPCRel4,
        _ => return None,
    })
}

fn reloc_width(kind: Reloc) -> usize {
    match kind {
        Reloc::Abs8 => 8,
        _ => 4,
    }
}

/// The heap's shape as text: every property lowering reads, and no address.
fn heap_shape(bases: &RegionBases) -> String {
    use crate::mem::{Addressing, RegionBase};
    let base = |base: &RegionBase| match base {
        RegionBase::Immediate(_) => "relocated".to_owned(),
        RegionBase::Symbol(name) => format!("symbol {name}"),
    };
    let form = match bases.addressing() {
        Addressing::Single { base: b, stride } => format!("single {} stride {stride}", base(b)),
        Addressing::Sharded {
            table,
            selector_bits,
            stride,
        } => format!("sharded {} bits {selector_bits} stride {stride}", base(table)),
    };
    format!(
        "heap {form} cursor {} runs {} regions {}",
        bases.cursor().is_some(),
        bases.runs().is_some(),
        bases.regions()
    )
}

/// What this executable is: its path, length and modification time. A rebuilt
/// rts has different lowering, so nothing compiled by another build may be
/// replayed.
fn executable_identity() -> Option<&'static str> {
    static IDENTITY: OnceLock<Option<String>> = OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            let path = std::env::current_exe().ok()?;
            let meta = std::fs::metadata(&path).ok()?;
            let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
            Some(format!("{} {} {}", path.display(), meta.len(), modified.as_nanos()))
        })
        .as_deref()
}
