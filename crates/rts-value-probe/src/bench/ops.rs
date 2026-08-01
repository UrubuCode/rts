//! Driver for kernel OPS — every JS operator whose Tagged path is a call today.

use crate::emit::kernel_ops::{Op, x0_today, x1_guarded, x2_proven};
use crate::harness::{Check, Row, report};
use crate::poly;

use super::{ITERS_OPS, MASK, N_OBJS};

/// Operand A: `0..255`. Operand B: `1..7` — never zero (so `%` is defined) and
/// small (so a shift count is in range), while still making `===` hit sometimes
/// and `<` miss most of the time.
fn operands() -> (Vec<i64>, Vec<i64>, Vec<f64>, Vec<f64>) {
    let mut a = Vec::with_capacity(N_OBJS);
    let mut b = Vec::with_capacity(N_OBJS);
    let mut af = Vec::with_capacity(N_OBJS);
    let mut bf = Vec::with_capacity(N_OBJS);
    for k in 0..N_OBJS {
        let x = (k & 255) as f64;
        let y = ((k % 7) + 1) as f64;
        a.push(poly::from_f64(x) as i64);
        b.push(poly::from_f64(y) as i64);
        af.push(x);
        bf.push(y);
    }
    (a, b, af, bf)
}

/// The same computation in Rust, for the checksum.
fn expected(op: Op, af: &[f64], bf: &[f64]) -> f64 {
    let mut s = 0.0f64;
    for i in 0..ITERS_OPS {
        let j = (i & MASK) as usize;
        let (x, y) = (af[j], bf[j]);
        let (xi, yi) = (x as i64 as i32, y as i64 as i32);
        let sh = y as u32 & 31;
        s += match op {
            Op::StrictEq | Op::LooseEq => f64::from(x == y),
            Op::StrictNe | Op::LooseNe => f64::from(x != y),
            Op::Lt => f64::from(x < y),
            Op::Le => f64::from(x <= y),
            Op::Gt => f64::from(x > y),
            Op::Ge => f64::from(x >= y),
            Op::Add => x + y,
            Op::Sub => x - y,
            Op::Mul => x * y,
            Op::Div => x / y,
            Op::Mod => x % y,
            Op::Exp => x.powf(y),
            Op::BitAnd => f64::from(xi & yi),
            Op::BitOr => f64::from(xi | yi),
            Op::BitXor => f64::from(xi ^ yi),
            Op::Shl => f64::from(xi.wrapping_shl(sh)),
            Op::Shr => f64::from(xi.wrapping_shr(sh)),
            Op::UShr => f64::from((xi as u32).wrapping_shr(sh)),
        };
    }
    s
}

pub fn kernel_ops() {
    let (a, b, af, bf) = operands();
    let hdr = [a.as_ptr() as i64, b.as_ptr() as i64];
    let p = hdr.as_ptr() as i64;

    for op in crate::emit::kernel_ops::ALL_OPS {
        let x0 = x0_today(op);
        let x1 = x1_guarded(op);
        let x2 = x2_proven(op);
        // A predicate accumulates a plain count; an arithmetic op accumulates
        // `f64` bits, which `to_number` reads back.
        let check = if op.is_predicate() {
            Check::Int
        } else {
            Check::Poly
        };
        let mut rows = vec![
            Row::new("X0 today", "box, box, call __rtsadp_*", move || {
                (x0.f)(ITERS_OPS, p, MASK)
            }),
            Row::new(
                "X1 +inline guard",
                "inline double test + native op, call on miss",
                move || (x1.f)(ITERS_OPS, p, MASK),
            ),
            Row::new("X2 proven Repr", "operands already native", move || {
                (x2.f)(ITERS_OPS, p, MASK)
            }),
        ];
        if op == Op::Mod {
            // The engine has no runtime-guarded integer `%`; only a
            // known-non-zero-constant divisor takes `srem` today. The guard hits
            // on 100% of this kernel's operands (all integral, divisor 1..7).
            let x3 = crate::emit::kernel_ops::x3_mod_int_srem();
            rows.push(Row::new(
                "X3 guarded int srem",
                "runtime int/non-zero guard -> srem, fmod on miss (100% hit)",
                move || (x3.f)(ITERS_OPS, p, MASK),
            ));
        }
        if op == Op::Exp {
            // `x ** 2` -> `fmul`. NOTE the guard hits on only 1/7 of this
            // kernel's operands (`b` cycles 1..7), so this row is a LOWER bound
            // on what `**`-with-literal-2 code would see — and it still shows
            // whether a missed guard costs anything.
            let x3 = crate::emit::kernel_ops::x3_exp_square();
            rows.push(Row::new(
                "X3 guarded square",
                "runtime b==2 guard -> fmul, pow on miss (1/7 hit)",
                move || (x3.f)(ITERS_OPS, p, MASK),
            ));
        }
        report(
            &format!(
                "KERNEL OPS `{}` — Tagged operands, {}M iterations",
                op.label(),
                ITERS_OPS / 1_000_000
            ),
            ITERS_OPS,
            expected(op, &af, &bf),
            check,
            rows,
        );
    }
    // `x ** 2` on its own operand set: the guard hits 100% here, which is the
    // case real code writes. Reported separately because the checksum has to
    // match a different expectation.
    exp_square();
    unary(&a, &af);
    drop((a, b, af, bf));
}

/// `x ** 2` — the form the engine has no special case for.
fn exp_square() {
    let mut a = Vec::with_capacity(N_OBJS);
    let mut b = Vec::with_capacity(N_OBJS);
    let mut af = Vec::with_capacity(N_OBJS);
    for k in 0..N_OBJS {
        let x = (k & 255) as f64;
        a.push(poly::from_f64(x) as i64);
        b.push(poly::from_f64(2.0) as i64);
        af.push(x);
    }
    let hdr = [a.as_ptr() as i64, b.as_ptr() as i64];
    let p = hdr.as_ptr() as i64;
    let expect: f64 = (0..ITERS_OPS)
        .map(|i| {
            let x = af[(i & MASK) as usize];
            x * x
        })
        .sum();

    let x0 = x0_today(Op::Exp);
    let x2 = x2_proven(Op::Exp);
    let x3 = crate::emit::kernel_ops::x3_exp_square();
    report(
        "KERNEL OPS `x ** 2` — the common form, guard hits 100%, 10M iterations",
        ITERS_OPS,
        expect,
        Check::Poly,
        vec![
            Row::new("X0 today", "box, box, call __rtsadp_pow", move || {
                (x0.f)(ITERS_OPS, p, MASK)
            }),
            Row::new("X2 proven Repr", "raw f64, still calls Math.pow", move || {
                (x2.f)(ITERS_OPS, p, MASK)
            }),
            Row::new("X3 guarded square", "b==2 -> fmul", move || {
                (x3.f)(ITERS_OPS, p, MASK)
            }),
        ],
    );
    drop((a, b, af));
}

/// `typeof`, `!`, unary `-`, `??`.
fn unary(a: &[i64], af: &[f64]) {
    use crate::emit::kernel_un::{ALL_UN, Un, u0_today, u1_guarded};
    let hdr = [a.as_ptr() as i64, 0i64];
    let p = hdr.as_ptr() as i64;

    for un in ALL_UN {
        let expect: f64 = match un {
            // Every operand is a number, so `typeof` yields "number" every time.
            Un::TypeOf => ITERS_OPS as f64,
            // `!x` counts the falsy operands: only the element holding 0.
            Un::Not => (0..ITERS_OPS)
                .filter(|i| af[(i & MASK) as usize] == 0.0)
                .count() as f64,
            Un::Neg => (0..ITERS_OPS).map(|i| -af[(i & MASK) as usize]).sum(),
            // No operand is null/undefined, so `??` never takes the right side.
            Un::Nullish => 0.0,
        };
        let u0 = u0_today(un);
        let u1 = u1_guarded(un);
        let detail0 = if un == Un::Nullish {
            "already pure IR — same row as U1"
        } else {
            "call __rtsadp_*"
        };
        report(
            &format!(
                "KERNEL UN `{}` — Tagged operand, {}M iterations",
                un.label(),
                ITERS_OPS / 1_000_000
            ),
            ITERS_OPS,
            expect,
            Check::Int,
            vec![
                Row::new("U0 today", detail0, move || (u0.f)(ITERS_OPS, p, MASK)),
                Row::new("U1 inline", "tag test in IR, call on miss", move || {
                    (u1.f)(ITERS_OPS, p, MASK)
                }),
            ],
        );
    }
}
