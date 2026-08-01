//! Kernel STDLIB SHAPE — the integration test for "symbolize the body".
//!
//! Every other kernel measures ONE axis. This one measures both at once, on the
//! same operation, because the claim under test is that moving a stdlib body
//! from a `.ts` prelude to a native symbol wins on BOTH — and a claim about two
//! axes has to be checked on two axes.
//!
//! The operation is a document scan (count `{` and `:`), which is what
//! `JSON.parse`'s tokenizer does per character.
//!
//! * **P0, the `.ts` prelude shape.** The scanner is TypeScript, so it lowers to
//!   Cranelift IR that calls a trampoline per character: `s.length`, then per
//!   character `s[i]` (which allocates a fresh one-character string and a fresh
//!   handle — `json.ts:238` + `abi_adapter.rs:62-67`) and four `===`
//!   comparisons. That IR must be COMPILED at every startup.
//! * **P1, the native-symbol shape.** One call. The body is LLVM-compiled Rust
//!   already in the binary, so the compile pipeline emits two instructions and
//!   the symbol itself costs nothing to compile.
//!
//! Both must return the same count.

use std::hint::black_box;
use std::time::Instant;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{Context, control::ControlPlane};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use crate::emit::{Compiled, call1, compile};
use crate::harness::{Check, Row, report};
use crate::slab::{self, Entry};

const ROWS: usize = 2_000;
const RUNS: usize = 5;

const STR_LEN: (&str, usize) = ("probe_str_len", 1);
const CHAR_STR: (&str, usize) = ("probe_char_str", 2);
const STR_EQ: (&str, usize) = ("probe_str_eq_lit", 2);
const SCAN: (&str, usize) = ("probe_scan_native", 1);

/// Emit the `.ts`-prelude scanner: a per-character loop of trampoline calls.
/// Factored out so the SAME body feeds both the runtime kernel and the
/// compile-time measurement — otherwise the two axes would be measuring
/// different code.
fn emit_prelude_scan(fb: &mut FunctionBuilder, im: &crate::emit::Imports, recv: cranelift_codegen::ir::Value) {
    let n = call1(fb, im[STR_LEN.0], &[recv]);

    let header = fb.create_block();
    fb.append_block_param(header, types::I64);
    fb.append_block_param(header, types::I64);
    let body = fb.create_block();
    let exit = fb.create_block();
    fb.append_block_param(exit, types::I64);

    let z = fb.ins().iconst(types::I64, 0);
    let a0 = fb.ins().iconst(types::I64, 0);
    fb.ins().jump(header, &[z.into(), a0.into()]);

    fb.switch_to_block(header);
    let i = fb.block_params(header)[0];
    let acc = fb.block_params(header)[1];
    let go = fb.ins().icmp(IntCC::SignedLessThan, i, n);
    fb.ins().brif(go, body, &[], exit, &[acc.into()]);

    fb.switch_to_block(body);
    fb.seal_block(body);
    // `const c = s[i]` — a fresh one-character string, allocated.
    let c = call1(fb, im[CHAR_STR.0], &[recv, i]);
    // `c === "{" || c === ":"`, plus the two whitespace tests `skipWs` performs.
    let mut sum = fb.ins().iconst(types::I64, 0);
    for lit in [b'{' as i64, b':' as i64, b' ' as i64, b'\n' as i64] {
        let l = fb.ins().iconst(types::I64, lit);
        let eq = call1(fb, im[STR_EQ.0], &[c, l]);
        // Only `{` and `:` count; the whitespace tests are performed and
        // discarded, exactly as the real scanner does.
        if lit == b'{' as i64 || lit == b':' as i64 {
            sum = fb.ins().iadd(sum, eq);
        } else {
            black_box_val(fb, eq);
        }
    }
    let acc_next = fb.ins().iadd(acc, sum);
    let i_next = fb.ins().iadd_imm(i, 1);
    fb.ins().jump(header, &[i_next.into(), acc_next.into()]);
    fb.seal_block(header);

    fb.switch_to_block(exit);
    fb.seal_block(exit);
    let out = fb.block_params(exit)[0];
    fb.ins().return_(&[out]);
}

/// Keep a value live without contributing arithmetic: `v & 0` folds to zero but
/// the call producing `v` is still emitted and executed.
fn black_box_val(fb: &mut FunctionBuilder, v: cranelift_codegen::ir::Value) {
    let _ = fb.ins().band_imm(v, 0);
}

fn prelude_kernel() -> Compiled {
    compile("std_prelude_scan", &[STR_LEN, CHAR_STR, STR_EQ], |fb, im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let recv = fb.block_params(entry)[2];
        emit_prelude_scan(fb, im, recv);
    })
}

fn symbol_kernel() -> Compiled {
    compile("std_symbol_scan", &[SCAN], |fb, im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let recv = fb.block_params(entry)[2];
        let r = call1(fb, im[SCAN.0], &[recv]);
        fb.ins().return_(&[r]);
    })
}

pub fn kernel_stdlib_shape() {
    let mut doc = String::from("[");
    for i in 0..ROWS {
        if i > 0 {
            doc.push_str(", ");
        }
        doc.push_str(&format!("{{\"id\": {i}, \"name\": \"n{i}\", \"ok\": true}}"));
    }
    doc.push(']');
    let bytes = doc.clone().into_bytes();
    let chars = bytes.len() as i64;
    let expect = bytes
        .iter()
        .filter(|b| **b == b'{' || **b == b':')
        .count() as f64;

    slab::sharded::reset();
    let h = slab::sharded::alloc(Entry::String(bytes)) as i64;

    let kp = prelude_kernel();
    let ks = symbol_kernel();
    let (fp, fs) = (kp.f, ks.f);

    report(
        "KERNEL STDLIB SHAPE / THROUGHPUT — scanning a JSON document, per character",
        chars,
        expect,
        Check::Int,
        vec![
            Row::new(
                "P0 `.ts` prelude shape — a trampoline per character",
                "s.length, then per char: `s[i]` (allocates!) + four `===`",
                move || fp(0, 0, black_box(h)),
            ),
            Row::new(
                "P1 native symbol — ONE call for the whole document",
                "the body is LLVM-compiled Rust already in the binary",
                move || fs(0, 0, black_box(h)),
            ),
        ],
    );
    drop((kp, ks));

    compile_cost();
}

/// The startup axis: what each shape costs the compile pipeline.
///
/// This is the half that is invisible in a throughput benchmark and that the
/// `RTS_TIMING` breakdown says is 82% of a hello-world's compile time.
fn compile_cost() {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    flags.set("enable_verifier", "false").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa")
        .finish(settings::Flags::new(flags))
        .expect("finish isa");

    // Both bodies need their imports declared in a module, so reuse the real
    // kernel-building path and time only `Context::compile`.
    let measure = |label: &'static str,
                   needed: &[(&'static str, usize)],
                   build: fn(&mut FunctionBuilder, &crate::emit::Imports)| {
        let mut times = Vec::new();
        let mut size = 0usize;
        for _ in 0..RUNS {
            let (mut ctx, _keep) = crate::emit::build_context(needed, build, &*isa);
            let t = Instant::now();
            let mut cp = ControlPlane::default();
            let code = ctx.compile(&*isa, &mut cp).expect("compile");
            times.push(t.elapsed().as_secs_f64() * 1e6);
            size = code.code_buffer().len();
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (label, times[RUNS / 2], size)
    };

    let a = measure("P0 `.ts` prelude shape", &[STR_LEN, CHAR_STR, STR_EQ], |fb, im| {
        let entry = fb.create_block();
        let mut sig = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let _ = sig;
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let recv = fb.block_params(entry)[2];
        emit_prelude_scan(fb, im, recv);
    });
    let b = measure("P1 native symbol", &[SCAN], |fb, im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let recv = fb.block_params(entry)[2];
        let r = call1(fb, im[SCAN.0], &[recv]);
        fb.ins().return_(&[r]);
    });

    println!("KERNEL STDLIB SHAPE / STARTUP — what each shape costs the compile pipeline");
    println!("{}", "-".repeat(74));
    println!("  {:<30} {:>13} {:>13}", "shape", "compile us", "code bytes");
    for (label, us, size) in [a, b] {
        println!("  {label:<30} {us:>13.2} {size:>13}");
    }
    println!(
        "\n  A native symbol needs NO compilation of its body — the {:.1}x here is only\n  the call site. In the engine the prelude body is compiled at EVERY startup\n  (`RTS_TIMING`: 425 functions, 17.6 ms, for a one-line program).\n",
        a.1 / b.1.max(1e-9)
    );
}
