//! Kernel COMPILE-TIME — the ONE axis on which a hand-written backend could beat
//! Cranelift.
//!
//! Code QUALITY is already settled by `bench/symarch.rs`: Cranelift's inline
//! emission ties the ceiling of a precompiled native symbol (0.67 vs 0.66
//! ns/element). A replacement backend cannot emit a better `fadd`. So the only
//! remaining question is COMPILE SPEED, and the shape that wins it is
//! copy-and-patch: pre-compile a template per operation, then generate code by
//! `memcpy`-ing templates and patching immediates.
//!
//! That approach is not speculative — it is the Xu & Kjolstad OOPSLA 2021 paper
//! that Cranelift's own README cites for its performance figure, and the paper
//! reports code quality COMPARABLE to Cranelift ("2.6% slower on Coremark, 4.6%
//! faster on PolyBenchC") at a fraction of the compile cost.
//!
//! This kernel measures both ends on this machine: what Cranelift charges to
//! compile a function, and what stitching the same number of bytes costs.

use std::hint::black_box;
use std::time::Instant;

use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{Context, control::ControlPlane};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

/// Operations per generated function. 200 is around a mid-sized lowered JS
/// function; the per-op cost is what matters, not the absolute.
const OPS: usize = 200;
/// Functions compiled per timing round.
const FUNCS: usize = 200;
const RUNS: usize = 5;

/// Function sizes for the slope measurement. Doubling sizes give the marginal
/// compile cost of one more IR instruction, which is the number that decides
/// whether emitting an inline fast path is affordable at compile time.
const SIZES: [usize; 5] = [25, 50, 100, 200, 400];

pub fn kernel_compile_time() {
    compile_slope();
    compile_vs_stitch();
}

/// How much does Cranelift charge PER IR INSTRUCTION?
///
/// This is the exchange rate between the two halves of the "replace emitted IR
/// with symbol calls" thesis. Emitting an inline fast path costs compile time
/// ONCE, per site. Calling a symbol instead costs run time EVERY execution. The
/// slope here plus the runtime delta from `bench/irladder.rs` gives the
/// break-even execution count.
fn compile_slope() {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    flags.set("enable_verifier", "false").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa")
        .finish(settings::Flags::new(flags))
        .expect("finish isa");
    let call_conv = isa.default_call_conv();

    println!("KERNEL COMPILE-TIME / SLOPE — what Cranelift charges per IR instruction");
    println!("{}", "-".repeat(70));
    println!(
        "  {:>6}  {:>7}  {:>11}  {:>13}  {:>11}",
        "ops", "instrs", "compile us", "us/instruction", "bytes/fn"
    );

    let mut prev: Option<(f64, f64)> = None;
    for &ops in SIZES.iter() {
        let n = 100usize;
        let mut times = Vec::new();
        let mut bytes = 0usize;
        for _ in 0..RUNS {
            let mut ctxs = Vec::with_capacity(n);
            for _ in 0..n {
                let mut sig = Signature::new(call_conv);
                sig.params.push(AbiParam::new(types::F64));
                sig.returns.push(AbiParam::new(types::F64));
                let mut ctx = Context::new();
                ctx.func.signature = sig;
                {
                    let mut fbc = FunctionBuilderContext::new();
                    let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fbc);
                    let b = fb.create_block();
                    fb.append_block_params_for_function_params(b);
                    fb.switch_to_block(b);
                    fb.seal_block(b);
                    let mut v = fb.block_params(b)[0];
                    for i in 0..ops {
                        let k = fb.ins().f64const(1.0 + (i as f64) * 0.5);
                        let m = fb.ins().fmul(v, k);
                        v = fb.ins().fadd(m, k);
                    }
                    fb.ins().return_(&[v]);
                    fb.finalize();
                }
                ctxs.push(ctx);
            }
            let t = Instant::now();
            let mut total = 0usize;
            for ctx in ctxs.iter_mut() {
                let mut cp = ControlPlane::default();
                let code = ctx.compile(&*isa, &mut cp).expect("compile");
                total += code.code_buffer().len();
            }
            times.push(t.elapsed().as_secs_f64() * 1e6 / n as f64);
            bytes = total / n;
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let us = times[RUNS / 2];
        // 3 instructions emitted per op (f64const, fmul, fadd).
        let instrs = (ops * 3) as f64;
        // The MARGINAL cost: slope between this size and the previous one, which
        // excludes the fixed per-function overhead (prologue, epilogue, setup).
        let marginal = match prev {
            Some((pi, pu)) => (us - pu) / (instrs - pi),
            None => us / instrs,
        };
        println!(
            "  {ops:>6}  {:>7.0}  {us:>11.1}  {marginal:>13.3}  {bytes:>11}",
            instrs
        );
        prev = Some((instrs, us));
    }
    println!();
}

fn compile_vs_stitch() {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    flags.set("enable_verifier", "false").unwrap();
    let isa = cranelift_native::builder()
        .expect("host isa")
        .finish(settings::Flags::new(flags))
        .expect("finish isa");
    let call_conv = isa.default_call_conv();

    // --- Cranelift: build IR, then compile it ------------------------------
    let mut build_ms = Vec::new();
    let mut compile_ms = Vec::new();
    let mut code_len = 0usize;

    for _ in 0..RUNS {
        let mut ctxs = Vec::with_capacity(FUNCS);
        let t_build = Instant::now();
        for _ in 0..FUNCS {
            let mut sig = Signature::new(call_conv);
            sig.params.push(AbiParam::new(types::F64));
            sig.returns.push(AbiParam::new(types::F64));
            let mut ctx = Context::new();
            ctx.func.signature = sig;
            {
                let mut fbc = FunctionBuilderContext::new();
                let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fbc);
                let b = fb.create_block();
                fb.append_block_params_for_function_params(b);
                fb.switch_to_block(b);
                fb.seal_block(b);
                let mut v = fb.block_params(b)[0];
                // A dependent chain so nothing folds away and the register
                // allocator has real work.
                for i in 0..OPS {
                    let k = fb.ins().f64const(1.0 + (i as f64) * 0.5);
                    let m = fb.ins().fmul(v, k);
                    v = fb.ins().fadd(m, k);
                }
                fb.ins().return_(&[v]);
                fb.finalize();
            }
            ctxs.push(ctx);
        }
        build_ms.push(t_build.elapsed().as_secs_f64() * 1000.0);

        let t_comp = Instant::now();
        let mut total = 0usize;
        for ctx in ctxs.iter_mut() {
            let mut cp = ControlPlane::default();
            let code = ctx.compile(&*isa, &mut cp).expect("compile");
            total += code.code_buffer().len();
        }
        compile_ms.push(t_comp.elapsed().as_secs_f64() * 1000.0);
        code_len = total / FUNCS;
    }

    build_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    compile_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let build = build_ms[RUNS / 2];
    let comp = compile_ms[RUNS / 2];

    // --- Copy-and-patch: memcpy the templates, patch the immediates --------
    // The generous model: one `memcpy` of the whole function's worth of bytes
    // plus one 8-byte immediate patch per operation. A real implementation also
    // selects templates and fixes relocations, so this is a LOWER BOUND on what
    // stitching costs — i.e. an UPPER BOUND on how much it could win.
    let template = vec![0x90u8; code_len];
    let mut out = vec![0u8; code_len];
    let mut stitch_ms = Vec::new();
    for _ in 0..RUNS {
        let t = Instant::now();
        for _ in 0..FUNCS {
            out.copy_from_slice(black_box(&template));
            for i in 0..OPS {
                let at = (i * code_len / OPS).min(code_len.saturating_sub(8));
                out[at..at + 8].copy_from_slice(&(i as u64).to_le_bytes());
            }
            black_box(&out);
        }
        stitch_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    stitch_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let stitch = stitch_ms[RUNS / 2];

    println!("KERNEL COMPILE-TIME — what a hand-written backend could actually save");
    println!("{}", "-".repeat(70));
    println!("  {FUNCS} functions x {OPS} ops, median of {RUNS}; emitted code {code_len} bytes/fn");
    println!();
    println!(
        "  {:<44} {:>9.2} ms   {:>8.1} us/fn",
        "IR construction (RTS's front-end would keep this)",
        build,
        build * 1000.0 / FUNCS as f64
    );
    println!(
        "  {:<44} {:>9.2} ms   {:>8.1} us/fn",
        "Cranelift compile (opt + isel + regalloc + emit)",
        comp,
        comp * 1000.0 / FUNCS as f64
    );
    println!(
        "  {:<44} {:>9.2} ms   {:>8.1} us/fn",
        "copy-and-patch LOWER BOUND (memcpy + patch)",
        stitch,
        stitch * 1000.0 / FUNCS as f64
    );
    println!();
    println!(
        "  Cranelift compile is {:.1}x the stitch floor, and {:.2}x the IR construction\n  that a replacement backend would NOT remove.",
        comp / stitch.max(1e-9),
        comp / build.max(1e-9)
    );
    println!();
}
