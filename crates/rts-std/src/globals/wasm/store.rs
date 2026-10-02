//! Where a compiled module, a live instance and an exported function live.
//!
//! # Reuse-check
//!
//! `rts-cranelift` answers nothing here: decoding and running foreign `.wasm`
//! bytecode is not a capability of this workspace's own compiler, and
//! `rts-node/src/wasi/host.rs` had already checked (`src/shape/`, `src/sched/`,
//! `src/abi/` — none of them decode a `.wasm`). `wasmi` is the engine, and it is
//! the one this workspace already depends on rather than a second choice: that
//! module compiles and runs modules with it today. **`node:wasi` keeps its own
//! `Engine` until it is reconnected to this surface** — the duplication is real,
//! named, and is the point of the lot that follows this one, which deletes the
//! divergence `wasi/mod.rs` states at its head ("no `WebAssembly` global") by
//! consuming an `Instance` from here instead of raw bytes.
//!
//! The shape of the tables is `storage.rs`'s and `wasi/mod.rs`'s, for the reason
//! both of them give: a host crate outside `rts-core` has no cell table to key an
//! `Aside<T>` by, so state that would live beside the cell lives in a
//! `Mutex<Vec<_>>` here, addressed by a small integer the JavaScript object
//! carries. Nothing in these tables is a JavaScript VALUE, so the collector has
//! nothing to be told — which is what keeps this out of `side_tables`.
//!
//! # Why nothing is ever removed
//!
//! A `Store` is dropped only with the process. An exported function is reached
//! through an index into [`INSTANCES`], and an index that could be reused would
//! let a callable outlive its instance and then call a DIFFERENT one — a wrong
//! answer rather than an error. Freeing needs `entry::on_death` on the JavaScript
//! object, which is the next lot's work; until then a program that instantiates in
//! a loop grows, and that is a stated cost rather than an unnoticed one.

use std::sync::Mutex;
use wasmi::{Engine, Instance, Module, Store};

/// The one engine. `wasmi::Engine` is `Send + Sync` and compiling against two of
/// them would make a `Module` from one unusable in the other's `Store`.
static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

/// Every `new WebAssembly.Module`, by the index its JavaScript object carries.
static MODULES: Mutex<Vec<Module>> = Mutex::new(Vec::new());

/// Each module's export names in DECLARATION order, parallel to [`MODULES`].
/// `order.rs` says why the engine cannot be asked for it.
static ORDERS: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());

/// Every live instance: its store, and the handle into it.
///
/// `Option` because a call TAKES the instance out for the duration of the call
/// and puts it back after. That is what makes the surface re-entrant, and
/// re-entrancy is not a refinement here: a wasm body that calls a JavaScript
/// import can call another export from inside it (`wasm-bindgen` does, through
/// its finalizers), and the previous shape held this lock across the call. A row
/// that is `None` is one whose instance is currently running.
static INSTANCES: Mutex<Vec<Option<Live>>> = Mutex::new(Vec::new());

/// Every exported function a program has been handed, by the index its callable
/// carries in its environment.
static EXPORTS: Mutex<Vec<Export>> = Mutex::new(Vec::new());

/// An instantiated module and the store its state lives in.
struct Live {
    store: Store<HostState>,
    instance: Instance,
}

/// What a host function reached from inside a call needs to know.
///
/// Only the row, because everything else is reachable from the `Caller` the
/// callback is handed — and a row is what identifies which JavaScript `Memory`
/// object this instance's linear memory is mirrored into.
pub(super) struct HostState {
    pub(super) row: usize,
}

/// Which function of which instance a callable stands for.
#[derive(Clone)]
pub(super) struct Export {
    pub(super) instance: usize,
    /// The parameter kinds and the result count, recorded at registration.
    ///
    /// Cached rather than read from the instance on every call, and that is a
    /// correctness point and not a saving: reading it needs the instance, and the
    /// instance is TAKEN out of the table while it runs — so a call reached from
    /// inside another call found nothing and reported that the export had
    /// disappeared.
    signature: (Vec<wasmi::core::ValueType>, usize),
    /// Resolved by NAME at call time rather than held as a `wasmi::Func`,
    /// because a `Func` borrows nothing but reading it back costs one lookup
    /// against a store this already has to lock.
    pub(super) name_at: usize,
}

/// The export names, parallel to [`EXPORTS`] — a `String` in a `static Mutex`
/// initialiser is what a `const fn` cannot build, so the name is held apart
/// rather than inside `Export`.
static EXPORT_NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The engine, created on first use.
fn engine() -> Engine {
    let mut held = ENGINE.lock().expect("the wasm engine lock");
    held.get_or_insert_with(Engine::default).clone()
}

/// Compiles bytes, answering the index of the module or why it would not compile.
pub(super) fn compile(bytes: &[u8]) -> Result<usize, String> {
    let engine = engine();
    let module = Module::new(&engine, bytes).map_err(|error| error.to_string())?;
    let order = super::order::exported_names(bytes);
    let mut held = MODULES.lock().expect("the module table lock");
    held.push(module);
    let at = held.len() - 1;
    drop(held);
    let mut orders = ORDERS.lock().expect("the export order lock");
    debug_assert_eq!(orders.len(), at, "the orders must stay parallel to the modules");
    orders.push(order);
    Ok(at)
}

/// Whether bytes are a module this engine accepts — `WebAssembly.validate`.
pub(super) fn validates(bytes: &[u8]) -> bool {
    Module::new(&engine(), bytes).is_ok()
}

/// Runs `body` over a compiled module.
pub(super) fn with_module<T>(at: usize, body: impl FnOnce(&Module) -> T) -> Option<T> {
    let held = MODULES.lock().expect("the module table lock");
    held.get(at).map(body)
}

/// Instantiates a compiled module, answering the index of the live instance.
///
/// `imports` is the JavaScript import object, or `undefined` for none.
/// `wasmi::Module` is not `Clone`, so the work happens with the module table
/// locked rather than over a copy — and nothing inside reaches back for a module.
///
/// The ROW is decided before the instance exists, because a host function needs
/// it to find the JavaScript `Memory` this instance's linear memory mirrors, and
/// `start` may call one. It is the length the table will have, which is sound
/// while the push below is the only thing that grows it.
pub(super) fn instantiate(module_at: usize, imports: u64) -> Result<usize, String> {
    let engine = engine();
    let row = {
        let held = INSTANCES.lock().expect("the instance table lock");
        held.len()
    };
    let built = with_module(module_at, |module| {
        let mut store = Store::new(&engine, HostState { row });
        let mut linker = wasmi::Linker::<HostState>::new(&engine);
        super::imports::define(&mut linker, &mut store, module, imports)?;
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|error| error.to_string())?
            .start(&mut store)
            .map_err(|error| error.to_string())?;
        Ok::<Live, String>(Live { store, instance })
    })
    .ok_or("no such module")??;
    let mut held = INSTANCES.lock().expect("the instance table lock");
    debug_assert_eq!(held.len(), row, "the row must be the one the host state carries");
    held.push(Some(built));
    Ok(held.len() - 1)
}

/// Records an exported function, answering the index a callable carries.
pub(super) fn remember_export(instance: usize, name: &str) -> usize {
    let signature = signature_of(instance, name).unwrap_or_default();
    let mut names = EXPORT_NAMES.lock().expect("the export name lock");
    names.push(name.to_owned());
    let name_at = names.len() - 1;
    drop(names);
    let mut held = EXPORTS.lock().expect("the export table lock");
    held.push(Export { instance, name_at, signature });
    held.len() - 1
}

/// Reads an export's signature off the live instance, for [`remember_export`].
fn signature_of(instance: usize, name: &str) -> Option<(Vec<wasmi::core::ValueType>, usize)> {
    let held = INSTANCES.lock().expect("the instance table lock");
    let live = held.get(instance)?.as_ref()?;
    let func = live.instance.get_export(&live.store, name)?.into_func()?;
    let ty = func.ty(&live.store);
    Some((ty.params().to_vec(), ty.results().len()))
}

/// Which live instance a recorded export belongs to.
pub(super) fn instance_of(at: usize) -> Option<usize> {
    EXPORTS.lock().expect("the export table lock").get(at).map(|export| export.instance)
}

/// The arity and parameter kinds of a recorded export.
pub(super) fn signature(at: usize) -> Option<(Vec<wasmi::core::ValueType>, usize)> {
    EXPORTS.lock().expect("the export table lock").get(at).map(|export| export.signature.clone())
}

/// Calls a recorded export.
///
/// The instance is TAKEN out of the table for the duration, so a JavaScript
/// import called from inside the wasm body can reach this function again for
/// another export of the same instance — which `wasm-bindgen` does. Holding the
/// lock across the call, as the first version did, deadlocks there.
///
/// A row left `None` by a panic would be permanently unusable, which is the cost
/// of this shape; `extern "C"` frames here cannot unwind anyway, so a panic ends
/// the process rather than leaving the table in that state.
pub(super) fn call(at: usize, arguments: &[wasmi::Value]) -> Result<Vec<wasmi::Value>, String> {
    let export = EXPORTS
        .lock()
        .expect("the export table lock")
        .get(at)
        .cloned()
        .ok_or("no such export")?;
    let name = EXPORT_NAMES
        .lock()
        .expect("the export name lock")
        .get(export.name_at)
        .cloned()
        .ok_or("no such export name")?;
    let mut live = take(export.instance).ok_or(REENTRANT)?;
    let outcome = run(&mut live, &name, arguments);
    give_back(export.instance, live);
    outcome
}

/// One call against an instance held OUTSIDE the table.
fn run(live: &mut Live, name: &str, arguments: &[wasmi::Value]) -> Result<Vec<wasmi::Value>, String> {
    let func = live
        .instance
        .get_export(&live.store, name)
        .and_then(wasmi::Extern::into_func)
        .ok_or_else(|| format!("instance exports no `{name}`"))?;
    let count = func.ty(&live.store).results().len();
    let mut results = vec![wasmi::Value::I32(0); count];
    func.call(&mut live.store, arguments, &mut results)
        .map_err(|error| error.to_string())?;
    Ok(results)
}

/// What a call into an instance that is already running answers.
///
/// A JavaScript import CAN call an export — of another instance, and anything
/// else JavaScript does — but not of the one whose body is currently suspended on
/// it. `wasmi` allows that only through the `Caller` it hands the host function,
/// and reaching it from where the JavaScript call lands would mean keeping a
/// `Caller` alive in a thread-local across a borrow the compiler is right to
/// refuse. Named rather than deadlocked or silently wrong, and measured: node
/// answers 6 and 8 for the fixture's case, and this answers a `RuntimeError`.
///
/// What made this acceptable for now was checking the claim this module first
/// made without checking it: `wasm-bindgen`'s finalizers do NOT need it. They run
/// from a `FinalizationRegistry`, which is a microtask — after the call, not
/// inside it.
pub(super) const REENTRANT: &str =
    "a reentrant call into the instance that is already running is not supported";

/// Takes an instance out of the table, leaving the row running.
fn take(at: usize) -> Option<Live> {
    INSTANCES.lock().expect("the instance table lock").get_mut(at)?.take()
}

/// Puts one back.
fn give_back(at: usize, live: Live) {
    if let Some(row) = INSTANCES.lock().expect("the instance table lock").get_mut(at) {
        *row = Some(live);
    }
}

/// The names and kinds a COMPILED module exports, before anything is
/// instantiated — which is the question `WebAssembly.Module.exports` asks and a
/// different one from [`exports_of`], whose answer needs a store.
pub(super) fn module_exports(at: usize) -> Vec<(String, Kind)> {
    let rows: Vec<(String, Kind)> = with_module(at, |module| {
        module
            .exports()
            .map(|export| (export.name().to_owned(), kind_of_type(&export.ty().clone())))
            .collect()
    })
    .unwrap_or_default();
    super::order::applied(&order_of(at), rows, |row| row.0.as_str())
}

/// A module's declaration order, empty when it has none recorded.
fn order_of(at: usize) -> Vec<String> {
    ORDERS.lock().expect("the export order lock").get(at).cloned().unwrap_or_default()
}

/// What a compiled module IMPORTS: the module it asks, the name, and the kind.
pub(super) fn module_imports(at: usize) -> Vec<(String, String, Kind)> {
    with_module(at, |module| {
        module
            .imports()
            .map(|import| {
                (
                    import.module().to_owned(),
                    import.name().to_owned(),
                    kind_of_type(&import.ty().clone()),
                )
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The names and kinds a live instance exports.
pub(super) fn exports_of(instance_at: usize, module_at: usize) -> Vec<(String, Kind)> {
    let held = INSTANCES.lock().expect("the instance table lock");
    let Some(Some(live)) = held.get(instance_at) else { return Vec::new() };
    let rows: Vec<(String, Kind)> = live
        .instance
        .exports(&live.store)
        .map(|export| (export.name().to_owned(), kind_of(&export.into_extern())))
        .collect();
    drop(held);
    // Declaration order here too: `Object.keys(instance.exports)` is observable,
    // and the properties are written in the order this answers.
    super::order::applied(&order_of(module_at), rows, |row| row.0.as_str())
}

/// Which of the four things an export is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Function,
    Memory,
    Table,
    Global,
}

impl Kind {
    /// The `kind` string of the JS-API's export descriptor.
    pub(super) fn text(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Memory => "memory",
            Kind::Table => "table",
            Kind::Global => "global",
        }
    }
}

fn kind_of(external: &wasmi::Extern) -> Kind {
    match external {
        wasmi::Extern::Func(_) => Kind::Function,
        wasmi::Extern::Memory(_) => Kind::Memory,
        wasmi::Extern::Table(_) => Kind::Table,
        wasmi::Extern::Global(_) => Kind::Global,
    }
}

/// The same question over an `ExternType`, which is what a MODULE answers with
/// before anything is instantiated.
pub(super) fn kind_of_type(ty: &wasmi::ExternType) -> Kind {
    match ty {
        wasmi::ExternType::Func(_) => Kind::Function,
        wasmi::ExternType::Memory(_) => Kind::Memory,
        wasmi::ExternType::Table(_) => Kind::Table,
        wasmi::ExternType::Global(_) => Kind::Global,
    }
}

// ----------------------------------------------------------- linear memory --

/// The bytes of one of an instance's memories.
pub(super) fn memory_bytes(instance_at: usize, memory: wasmi::Memory) -> Vec<u8> {
    let held = INSTANCES.lock().expect("the instance table lock");
    let Some(Some(live)) = held.get(instance_at) else { return Vec::new() };
    memory.data(&live.store).to_vec()
}

/// Writes bytes into one of an instance's memories, up to its length.
pub(super) fn write_memory(instance_at: usize, memory: wasmi::Memory, source: &[u8]) {
    let mut held = INSTANCES.lock().expect("the instance table lock");
    let Some(Some(live)) = held.get_mut(instance_at) else { return };
    let window = memory.data_mut(&mut live.store);
    let count = source.len().min(window.len());
    window[..count].copy_from_slice(&source[..count]);
}

/// Grows one of an instance's memories, answering the page count it had.
pub(super) fn grow_memory(
    instance_at: usize,
    memory: wasmi::Memory,
    pages: u32,
) -> Result<u32, String> {
    let mut held = INSTANCES.lock().expect("the instance table lock");
    let live = held.get_mut(instance_at).and_then(Option::as_mut).ok_or("no such instance")?;
    let delta = wasmi::core::Pages::new(pages).ok_or("a page count wasm cannot represent")?;
    memory
        .grow(&mut live.store, delta)
        .map(|before| u32::from(before))
        .map_err(|error| error.to_string())
}

/// The memories a live instance exports, by name.
pub(super) fn exported_memories(instance_at: usize) -> Vec<(String, wasmi::Memory)> {
    let held = INSTANCES.lock().expect("the instance table lock");
    let Some(Some(live)) = held.get(instance_at) else { return Vec::new() };
    live.instance
        .exports(&live.store)
        .filter_map(|export| {
            let name = export.name().to_owned();
            export.into_extern().into_memory().map(|memory| (name, memory))
        })
        .collect()
}

/// A memory with no module behind it — `new WebAssembly.Memory({ initial })`.
///
/// It still needs a `Store` to live in, so one is made and registered as an
/// instance with no instance in it. `Live::instance` cannot be `Option` without
/// every caller asking, so the store is paired with a handle from an empty
/// module — which is what `MEMORY_ONLY` is: a module that declares nothing, so
/// instantiating it cannot fail and cannot run anything.
pub(super) fn standalone_memory(pages: u32) -> Result<(usize, wasmi::Memory), String> {
    let engine = engine();
    let module = Module::new(&engine, MEMORY_ONLY).map_err(|error| error.to_string())?;
    let mut store = Store::new(&engine, HostState { row: usize::MAX });
    let instance = wasmi::Linker::<HostState>::new(&engine)
        .instantiate(&mut store, &module)
        .map_err(|error| error.to_string())?
        .start(&mut store)
        .map_err(|error| error.to_string())?;
    let kind = wasmi::MemoryType::new(pages, None).map_err(|error| error.to_string())?;
    let memory = wasmi::Memory::new(&mut store, kind).map_err(|error| error.to_string())?;
    let mut held = INSTANCES.lock().expect("the instance table lock");
    held.push(Some(Live { store, instance }));
    Ok((held.len() - 1, memory))
}

/// An empty module: the eight-byte header and nothing else. See
/// [`standalone_memory`].
const MEMORY_ONLY: &[u8] = &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
