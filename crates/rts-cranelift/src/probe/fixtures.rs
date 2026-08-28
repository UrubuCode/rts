//! The programs the probe measures.
//!
//! Each is a primitive this layer claims to make cheap, written in the
//! representation and compiled through the ordinary pipeline. What is absent is
//! as deliberate as what is here: nothing that reaches a runtime entry point is
//! measured, because what a stand-in costs says nothing about what a real one
//! will, and a number that measured a stand-in would be worse than no number.
//!
//! # Why every fixture is a LOOP, and was one operation
//!
//! Because the instrument could not see. Each fixture used to be a single
//! primitive behind an `extern "C" fn(i64) -> i64` pointer, and `harness.rs`
//! measures that pointer at **1.27 ns** — against primitives that cost a
//! fraction of one. The table it produced on 2026-08-28, release, was:
//!
//! ```text
//!   arithmetic     2.18 ns  1.0x
//!   field_read     2.08 ns  1.0x
//!   widen_narrow   2.14 ns  1.0x
//!   type_guard     2.85 ns  1.3x
//! ```
//!
//! `field_read` reads BELOW `arithmetic` there, and it cannot be cheaper: it is
//! an addition's worth of address arithmetic plus a load. The ordering is noise,
//! which is the instrument saying it has no resolution left — its smallest
//! division is larger than everything it was built to measure.
//!
//! The module doc for [`super`] already said the call dominates and that the
//! distance from the floor is the number. Subtracting a floor removes an
//! OFFSET; it cannot recover RESOLUTION. Only amortising it can, so the
//! primitive is now repeated inside the compiled program and the call is paid
//! once per measurement instead of once per operation.
//!
//! # What the loop costs, and why it is a fixture rather than a correction
//!
//! The loop is not free — a compare, a branch and an increment per iteration —
//! so [`build_loop_floor`] measures exactly that with no body, and every other
//! row is read against it. A correction subtracted in the harness would be a
//! number nobody could check; a row in the same table is measured the same way
//! as everything it is subtracted from.
//!
//! # Why the trip count comes from the argument
//!
//! So that nothing can be folded. A constant trip count invites the code
//! generator to unroll or to evaluate the whole loop while compiling, and then
//! the number is about the optimizer rather than about the machine. The count
//! arrives in a register, so the loop must run.

use cranelift_jit::JITBuilder;
use cranelift_module::Linkage;

use crate::ir::{
    BlockId, CmpOp, ConstDecl, FuncBuilder, FuncId, FuncRegistry, Function, NumOp, ScalarBits,
    Signature, ValueId,
};
use crate::mem::{HeaderLayout, ObjectLayout, RegionBase, RegionBases};
use crate::repr::{RefKind, Repr};
use crate::symbols::RtEntry;
use crate::target::{MachineModule, host_isa};
use crate::types::{TypeId, TypeRegistry};

/// What a fixture's `build` produces.
///
/// A struct rather than a tuple because it grew a fourth member — the callee a
/// call fixture needs declared — and a four-place tuple at three call sites is
/// where the third and fourth get swapped.
pub struct Built {
    /// The program to measure.
    pub func: Function,
    /// Its signature: one trip count in, one accumulator out.
    pub signature: Signature,
    /// The heap it reads, when it reads one.
    pub heap: Option<RegionBases>,
    /// A function it calls, which has to be defined beside it.
    pub callee: Option<(FuncId, Function)>,
}

/// A program to measure, and what it needs to run.
pub struct Fixture {
    /// What it measures.
    pub name: &'static str,
    /// What it is measuring, in one line.
    pub about: &'static str,
    build: fn(&mut TypeRegistry, &mut FuncRegistry) -> Built,
}

/// Something compiled and ready to be called.
pub struct Compiled {
    entry: extern "C" fn(i64) -> i64,
}

impl Compiled {
    /// Runs it once, for the given number of inner iterations.
    pub fn call(&self, trips: i64) -> i64 {
        (self.entry)(trips)
    }
}

impl Fixture {
    /// Compiles this fixture into memory.
    ///
    /// Everything it allocates is leaked: the code and the heap both have to
    /// outlive the pointer that calls them, and a probe that tidied up while
    /// measuring would be measuring the tidying.
    pub fn compile(&self) -> Compiled {
        let mut types = TypeRegistry::new();
        let mut funcs = FuncRegistry::new();
        // Built BEFORE the entry function is declared, because a fixture that
        // calls something has to declare the callee first — `builder::call`
        // resolves the signature out of the registry, so a callee that is not
        // in it yet is `UnknownCallee` rather than a forward reference.
        let built = (self.build)(&mut types, &mut funcs);

        let shape = funcs.declare_signature(built.signature.clone());
        let id = funcs.declare_function(shape);

        let isa = host_isa().expect("this machine can host its own code");
        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        // Declared so the module links, never called: no fixture allocates.
        builder.symbol(RtEntry::Alloc.symbol(), unreached_alloc as *const u8);
        builder.symbol(
            RtEntry::WriteBarrier.symbol(),
            unreached_barrier as *const u8,
        );
        let mut jit = cranelift_jit::JITModule::new(builder);

        let machine_id = {
            let mut module = MachineModule::new(&mut jit);
            if let Some(heap) = built.heap {
                module = module.with_heap(heap);
            }
            if let Some((callee_id, callee)) = &built.callee {
                module
                    .declare(*callee_id, "probe_callee", Linkage::Local, &funcs)
                    .expect("callee declared");
                module
                    .define(*callee_id, callee, &funcs, &types)
                    .expect("callee defined");
            }
            module
                .declare(id, self.name, Linkage::Export, &funcs)
                .expect("declared");
            module
                .define(id, &built.func, &funcs, &types)
                .expect("defined");
            module.declarations().machine_id(id).expect("declared")
        };
        jit.finalize_definitions().expect("finalized");
        let address = jit.get_finalized_function(machine_id);
        std::mem::forget(jit);

        Compiled {
            entry: unsafe { std::mem::transmute::<*const u8, extern "C" fn(i64) -> i64>(address) },
        }
    }
}

extern "C" fn unreached_alloc(_size: i64, _ty: i64) -> i64 {
    unreachable!("no fixture allocates; a number that measured a stand-in would be a lie")
}

extern "C" fn unreached_barrier(_object: i64, _value: i64) {
    unreachable!("no fixture stores a reference")
}

/// Every fixture, in the order a reader should meet them.
///
/// The floor is first because every other row is read against it, and the call
/// is last because it is the only one that is expected to be expensive — the
/// layer above pays it on every operation it cannot emit inline, and knowing
/// what it costs HERE is what separates "the machine's call is slow" from "the
/// runtime does too much once the call arrives".
pub fn all() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "loop_floor",
            about: "the loop itself: a compare, a branch and an increment",
            build: build_loop_floor,
        },
        Fixture {
            name: "arithmetic",
            about: "one proven addition per iteration",
            build: build_arithmetic,
        },
        Fixture {
            name: "field_read",
            about: "a reference becoming an address, and a load",
            build: build_field_read,
        },
        Fixture {
            name: "widen_narrow",
            about: "a value made generic and proven back, as instructions",
            build: build_widen_narrow,
        },
        Fixture {
            name: "type_guard",
            about: "reading what an object says it is, and narrowing on it",
            build: build_type_guard,
        },
        Fixture {
            name: "call_direct",
            about: "a direct call to a known function, and its return",
            build: build_call_direct,
        },
    ]
}

/// The signature every fixture has: a trip count in, an accumulator out.
fn counted() -> Signature {
    Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    }
}

/// The blocks a counted loop needs, and the values it carries.
struct Loop {
    /// Where the body starts. The builder is positioned here on return.
    body: BlockId,
    /// Where the body must jump back to, with the next accumulator.
    head: BlockId,
    /// The accumulator, as the body sees it.
    accumulator: ValueId,
    /// The iteration number, as the body sees it.
    step: ValueId,
}

/// Builds `acc = 0; for step in 0..trips { acc = <body> }; return acc`.
///
/// The body is written by the caller, which finishes it by calling
/// [`close_loop`]. Split that way because the body needs its own
/// `FuncBuilder` — one cannot be held across the block switches this performs —
/// and because every fixture would otherwise repeat the same six blocks.
///
/// `carry` is what the accumulator is, and it is a parameter because one
/// fixture cannot use the obvious answer: **an `I64` cannot be widened.** The
/// generic form is a NaN box with a 48-bit payload, so a full machine integer
/// does not fit in it and `lower::value` refuses by name. `widen_narrow`
/// therefore carries an `I32`, which is the representation the question is
/// about anyway. The trip count and the step stay `I64` regardless — they are
/// the instrument, not the subject.
fn open_loop(func: &mut Function, types: &TypeRegistry, trips: ValueId, carry: Repr) -> Loop {
    let entry = func.entry;
    let head = {
        let mut b = FuncBuilder::new(func, types, entry);
        let head = b.create_block();
        b.add_block_param(head, carry);
        b.add_block_param(head, Repr::I64);
        let seed = constant(&mut b, carry, 0);
        let zero = constant(&mut b, Repr::I64, 0);
        b.jump(head, &[seed, zero]).expect("well formed");
        head
    };

    let (body, exit, accumulator, step) = {
        let params = func.block(head).expect("exists").params.clone();
        let (accumulator, step) = (params[0], params[1]);
        let mut b = FuncBuilder::new(func, types, head);
        let body = b.create_block();
        let exit = b.create_block();
        b.add_block_param(exit, carry);
        let more = b.compare(CmpOp::Lt, step, trips).expect("both proven");
        b.branch(more, (body, &[]), (exit, &[accumulator]))
            .expect("well formed");
        (body, exit, accumulator, step)
    };

    {
        let answer = func.block(exit).expect("exists").params[0];
        let mut b = FuncBuilder::new(func, types, exit);
        b.ret(&[answer]);
    }

    Loop {
        body,
        head,
        accumulator,
        step,
    }
}

/// Closes a loop body by carrying `next` back to the head.
fn close_loop(b: &mut FuncBuilder, shape: &Loop, next: ValueId) {
    let one = constant(b, Repr::I64, 1);
    let stepped = b.arith(NumOp::Add, shape.step, one).expect("both proven");
    b.jump(shape.head, &[next, stepped]).expect("well formed");
}

/// The loop with nothing in it. Every other row is this one subtracted.
fn build_loop_floor(_types: &mut TypeRegistry, _funcs: &mut FuncRegistry) -> Built {
    let types = TypeRegistry::new();
    let signature = counted();
    let mut func = Function::new(signature.clone());
    let trips = entry_param(&func, 0);
    let shape = open_loop(&mut func, &types, trips, Repr::I64);
    let mut b = FuncBuilder::new(&mut func, &types, shape.body);
    let carried = shape.accumulator;
    close_loop(&mut b, &shape, carried);
    Built {
        func,
        signature,
        heap: None,
        callee: None,
    }
}

/// One addition per iteration.
fn build_arithmetic(_types: &mut TypeRegistry, _funcs: &mut FuncRegistry) -> Built {
    let types = TypeRegistry::new();
    let signature = counted();
    let mut func = Function::new(signature.clone());
    let trips = entry_param(&func, 0);
    let shape = open_loop(&mut func, &types, trips, Repr::I64);
    let mut b = FuncBuilder::new(&mut func, &types, shape.body);
    // Against the STEP rather than a constant, so the addend changes every
    // iteration and nothing can strength-reduce the chain into a multiply.
    let next = b
        .arith(NumOp::Add, shape.accumulator, shape.step)
        .expect("both proven");
    close_loop(&mut b, &shape, next);
    Built {
        func,
        signature,
        heap: None,
        callee: None,
    }
}

/// Turning a reference into an address and reading a field.
///
/// The measured lever: doing this with arithmetic rather than a call was worth
/// more than any change to the value representation.
///
/// The reference is a CONSTANT here, and that is worth stating because it
/// bounds what the row says: the load is loop-invariant, so a code generator
/// that hoists it would make this row read as the floor. It does not today —
/// the row sits above the floor — and if it ever collapses onto it, that is the
/// first thing to check rather than a win.
fn build_field_read(types: &mut TypeRegistry, _funcs: &mut FuncRegistry) -> Built {
    let cell = types.declare(&[Repr::I64]);
    let heap = heap_for(cell, types, 1);
    let signature = counted();
    let mut func = Function::new(signature.clone());
    let trips = entry_param(&func, 0);
    let shape = open_loop(&mut func, types, trips, Repr::I64);

    let mut b = FuncBuilder::new(&mut func, types, shape.body);
    let object = constant(&mut b, Repr::Ref(RefKind::Aggregate(cell)), 0);
    let value = b.field_load(object, cell, 0).expect("field exists");
    let next = b
        .arith(NumOp::Add, shape.accumulator, value)
        .expect("both proven");
    close_loop(&mut b, &shape, next);

    Built {
        func,
        signature,
        heap: Some(heap),
        callee: None,
    }
}

/// Widening and narrowing back, which should cost almost nothing.
///
/// Both are bit operations rather than calls, so an optimizer can see through
/// the pair. If this number ever approaches the call row, that property broke.
fn build_widen_narrow(_types: &mut TypeRegistry, _funcs: &mut FuncRegistry) -> Built {
    let types = TypeRegistry::new();
    // An `I32` in and out: see [`open_loop`] for why this one fixture cannot
    // carry the machine integer every other one does.
    let signature = Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I32],
        ..Signature::default()
    };
    let mut func = Function::new(signature.clone());
    let trips = entry_param(&func, 0);
    let shape = open_loop(&mut func, &types, trips, Repr::I32);

    let (ok, fail) = {
        let mut b = FuncBuilder::new(&mut func, &types, shape.body);
        let ok = b.create_block();
        let fail = b.create_block();
        b.add_block_param(ok, Repr::I32);
        // The ACCUMULATOR is what is widened, so the pair sits on the loop's
        // dependency chain: the next iteration's widen cannot begin until this
        // one's narrow has produced a value, so neither can be hoisted out.
        let widened = b.widen(shape.accumulator);
        b.guard(widened, Repr::I32, (ok, &[]), (fail, &[]))
            .expect("well formed");
        (ok, fail)
    };

    {
        let narrowed = func.block(ok).expect("exists").params[0];
        let mut b = FuncBuilder::new(&mut func, &types, ok);
        let one = constant(&mut b, Repr::I32, 1);
        let next = b.arith(NumOp::Add, narrowed, one).expect("both proven");
        close_loop(&mut b, &shape, next);
    }
    {
        let mut b = FuncBuilder::new(&mut func, &types, fail);
        let zero = constant(&mut b, Repr::I32, 0);
        b.ret(&[zero]);
    }

    Built {
        func,
        signature,
        heap: Some(no_heap()),
        callee: None,
    }
}

/// Reading what an object says it is, then reading a field through that.
fn build_type_guard(types: &mut TypeRegistry, _funcs: &mut FuncRegistry) -> Built {
    let cell = types.declare(&[Repr::I64]);
    let heap = heap_for(cell, types, 1);
    let signature = counted();
    let mut func = Function::new(signature.clone());
    let trips = entry_param(&func, 0);
    let shape = open_loop(&mut func, types, trips, Repr::I64);

    let (matched, mismatched) = {
        let mut b = FuncBuilder::new(&mut func, types, shape.body);
        let matched = b.create_block();
        let mismatched = b.create_block();
        b.add_block_param(matched, Repr::Ref(RefKind::Aggregate(cell)));
        let object = constant(&mut b, Repr::Ref(RefKind::Opaque), 0);
        b.guard_type(object, cell, (matched, &[]), (mismatched, &[]))
            .expect("well formed");
        (matched, mismatched)
    };

    {
        let narrowed = func.block(matched).expect("exists").params[0];
        let mut b = FuncBuilder::new(&mut func, types, matched);
        let value = b.field_load(narrowed, cell, 0).expect("field exists");
        let next = b
            .arith(NumOp::Add, shape.accumulator, value)
            .expect("both proven");
        close_loop(&mut b, &shape, next);
    }
    {
        let mut b = FuncBuilder::new(&mut func, types, mismatched);
        let zero = constant(&mut b, Repr::I64, 0);
        b.ret(&[zero]);
    }

    Built {
        func,
        signature,
        heap: Some(heap),
        callee: None,
    }
}

/// A direct call to a known function, once per iteration.
///
/// # Why this fixture is the one worth having
///
/// Everything the layer above cannot emit inline becomes a call, and
/// `docs/codegen/native-call-floor.md` measured a built-in at ~35 ns against a
/// static call at 2.8 — so the question "how much of that is the machine's
/// call" had no instrument to answer it. This is that instrument, and it
/// deliberately measures the CHEAPEST possible call: a known callee, one
/// proven argument, one proven return, no safepoint, no barrier, nothing to
/// resolve. Whatever it costs is the floor under every call in every program
/// this engine compiles, and the difference between it and 35 ns belongs to
/// somebody else.
fn build_call_direct(_types: &mut TypeRegistry, funcs: &mut FuncRegistry) -> Built {
    let types = TypeRegistry::new();
    let callee_signature = counted();
    let callee_shape = funcs.declare_signature(callee_signature.clone());
    let callee_id = funcs.declare_function(callee_shape);

    // The callee: one addition and a return. Small on purpose — a call's cost
    // is the convention, not the callee, and a large body would measure the
    // body.
    let callee = {
        let mut func = Function::new(callee_signature);
        let x = entry_param(&func, 0);
        let entry = func.entry;
        let mut b = FuncBuilder::new(&mut func, &types, entry);
        let one = constant(&mut b, Repr::I64, 1);
        let answer = b.arith(NumOp::Add, x, one).expect("both proven");
        b.ret(&[answer]);
        func
    };

    let signature = counted();
    let mut func = Function::new(signature.clone());
    let trips = entry_param(&func, 0);
    let shape = open_loop(&mut func, &types, trips, Repr::I64);
    let mut b = FuncBuilder::new(&mut func, &types, shape.body);
    // The accumulator goes in and comes back, so the calls form a chain: one
    // cannot start before the last finished, and none can be hoisted.
    let answered = b
        .call(funcs, callee_id, &[shape.accumulator])
        .expect("declared above");
    close_loop(&mut b, &shape, answered[0]);

    Built {
        func,
        signature,
        heap: None,
        callee: Some((callee_id, callee)),
    }
}

/// The entry block's parameter at an index.
fn entry_param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// Materializes a constant.
fn constant(b: &mut FuncBuilder, repr: Repr, bits: u64) -> ValueId {
    let decl = b.declare_const(ConstDecl::Scalar {
        repr,
        bits: ScalarBits(bits),
    });
    b.use_const(decl)
}

/// A heap holding `count` objects of one type, with slot zero already filled.
///
/// The fixtures that read memory always read slot zero: the measurement is of
/// reading, not of choosing where to read.
fn heap_for(ty: TypeId, types: &TypeRegistry, count: usize) -> RegionBases {
    let layout = ObjectLayout::of(ty, types);
    let bytes = vec![0u8; layout.size as usize * count];
    let leaked = Box::leak(bytes.into_boxed_slice());
    let base = leaked.as_ptr() as u64;

    unsafe {
        let object = base as *mut u8;
        (object.offset(HeaderLayout::TYPE_OFFSET as isize) as *mut i64).write(ty.index() as i64);
        let field = layout.field_offset(0).expect("one field");
        (object.offset(field as isize) as *mut i64).write(1);
    }

    RegionBases::single(RegionBase::Immediate(base), layout.size)
}

/// A heap for a fixture that describes one but never reads it.
fn no_heap() -> RegionBases {
    RegionBases::single(RegionBase::Immediate(0), 8)
}
