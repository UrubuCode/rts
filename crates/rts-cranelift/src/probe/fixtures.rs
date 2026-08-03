//! The programs the probe measures.
//!
//! Each is a primitive this layer claims to make cheap, written in the
//! representation and compiled through the ordinary pipeline. What is absent is
//! as deliberate as what is here: nothing that reaches a runtime entry point is
//! measured, because what a stand-in costs says nothing about what a real one
//! will, and a number that measured a stand-in would be worse than no number.

use cranelift_jit::JITBuilder;
use cranelift_module::Linkage;

use crate::ir::{
    ConstDecl, FuncBuilder, FuncRegistry, Function, NumOp, ScalarBits, Signature, ValueId,
};
use crate::mem::{HeaderLayout, ObjectLayout, RegionBase, RegionBases};
use crate::repr::{RefKind, Repr};
use crate::symbols::RtEntry;
use crate::target::{MachineModule, host_isa};
use crate::types::{TypeId, TypeRegistry};

/// A program to measure, and what it needs to run.
pub struct Fixture {
    /// What it measures.
    pub name: &'static str,
    /// What it is measuring, in one line.
    pub about: &'static str,
    build: fn(&mut TypeRegistry) -> (Function, Signature, Option<RegionBases>),
    /// What the iteration number means to this fixture.
    ///
    /// Each one decides for itself, because a step is a different thing to each:
    /// a number to add, or a reference into a heap with one object in it. The
    /// harness passed the step to everything at first, which walked a reference
    /// off the end of the heap on the second iteration. A shared meaning for a
    /// number that means different things is how that happens.
    argument: fn(u64) -> i64,
}

/// Something compiled and ready to be called.
pub struct Compiled {
    entry: extern "C" fn(i64) -> i64,
}

impl Compiled {
    /// Runs it once.
    pub fn call(&self, argument: i64) -> i64 {
        (self.entry)(argument)
    }
}

impl Fixture {
    /// What this fixture should be given on a particular iteration.
    pub fn argument_for(&self, step: u64) -> i64 {
        (self.argument)(step)
    }

    /// Compiles this fixture into memory.
    ///
    /// Everything it allocates is leaked: the code and the heap both have to
    /// outlive the pointer that calls them, and a probe that tidied up while
    /// measuring would be measuring the tidying.
    pub fn compile(&self) -> Compiled {
        let mut types = TypeRegistry::new();
        let (func, signature, heap) = (self.build)(&mut types);

        let mut funcs = FuncRegistry::new();
        let shape = funcs.declare_signature(signature);
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
            if let Some(heap) = heap {
                module = module.with_heap(heap);
            }
            module
                .declare(id, self.name, Linkage::Export, &funcs)
                .expect("declared");
            module.define(id, &func, &funcs, &types).expect("defined");
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
pub fn all() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "arithmetic",
            about: "the floor: one proven addition and a return",
            build: build_arithmetic,
            argument: |step| step as i64,
        },
        Fixture {
            name: "field_read",
            about: "a reference becoming an address, and a load",
            build: build_field_read,
            argument: |_| 0,
        },
        Fixture {
            name: "widen_narrow",
            about: "a value made generic and proven back, as instructions",
            build: build_widen_narrow,
            argument: |step| step as i64,
        },
        Fixture {
            name: "type_guard",
            about: "reading what an object says it is, and narrowing on it",
            build: build_type_guard,
            argument: |_| 0,
        },
    ]
}

/// One addition. The number every other number is read against.
fn build_arithmetic(_types: &mut TypeRegistry) -> (Function, Signature, Option<RegionBases>) {
    let types = TypeRegistry::new();
    let signature = Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    };
    let mut func = Function::new(signature.clone());
    let x = entry_param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let doubled = b.arith(NumOp::Add, x, x).expect("proven");
    b.ret(&[doubled]);
    (func, signature, None)
}

/// Turning a reference into an address and reading a field.
///
/// The measured lever: doing this with arithmetic rather than a call was worth
/// more than any change to the value representation.
fn build_field_read(types: &mut TypeRegistry) -> (Function, Signature, Option<RegionBases>) {
    let cell = types.declare(&[Repr::I64]);
    let heap = heap_for(cell, types, 1);

    let signature = Signature {
        params: vec![Repr::Ref(RefKind::Aggregate(cell))],
        returns: vec![Repr::I64],
        ..Signature::default()
    };
    let mut func = Function::new(signature.clone());
    let object = entry_param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, types, entry);
    let value = b.field_load(object, cell, 0).expect("field exists");
    b.ret(&[value]);
    (func, signature, Some(heap))
}

/// Widening and narrowing back, which should cost almost nothing.
///
/// Both are bit operations rather than calls, so an optimizer can see through
/// the pair. If this number ever approaches a call, that property broke.
fn build_widen_narrow(_types: &mut TypeRegistry) -> (Function, Signature, Option<RegionBases>) {
    let types = TypeRegistry::new();
    let signature = Signature {
        params: vec![Repr::I32],
        returns: vec![Repr::I32],
        ..Signature::default()
    };
    let mut func = Function::new(signature.clone());
    let x = entry_param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::I32);
    let widened = b.widen(x);
    b.guard(widened, Repr::I32, (ok, &[]), (fail, &[]))
        .expect("well formed");

    let narrowed = func.block(ok).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, ok);
    b.ret(&[narrowed]);

    let mut b = FuncBuilder::new(&mut func, &types, fail);
    let zero = constant(&mut b, Repr::I32, 0);
    b.ret(&[zero]);

    (func, signature, Some(no_heap()))
}

/// Reading what an object says it is, then reading a field through that.
fn build_type_guard(types: &mut TypeRegistry) -> (Function, Signature, Option<RegionBases>) {
    let cell = types.declare(&[Repr::I64]);
    let heap = heap_for(cell, types, 1);

    let signature = Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        returns: vec![Repr::I64],
        ..Signature::default()
    };
    let mut func = Function::new(signature.clone());
    let object = entry_param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, types, entry);

    let matched = b.create_block();
    let mismatched = b.create_block();
    b.add_block_param(matched, Repr::Ref(RefKind::Aggregate(cell)));
    b.guard_type(object, cell, (matched, &[]), (mismatched, &[]))
        .expect("well formed");

    let narrowed = func.block(matched).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, types, matched);
    let value = b.field_load(narrowed, cell, 0).expect("field exists");
    b.ret(&[value]);

    let mut b = FuncBuilder::new(&mut func, types, mismatched);
    let zero = constant(&mut b, Repr::I64, 0);
    b.ret(&[zero]);

    (func, signature, Some(heap))
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
