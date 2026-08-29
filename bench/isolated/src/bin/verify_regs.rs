//! Register pressure across an `extern "C"` call, on its own.
//!
//! Split from `crossing_price.rs` so the number can be read without the rest:
//! a call clobbers every caller-saved register, so the allocator spills and
//! reloads whatever the caller was holding. A and B hold one and six live
//! accumulators across the SAME call; C and D hold the same across no call.
//! (B-A) - (D-C) is what the call costs in spills alone.
//!
//! RESULT, 2026-08-29: **inconclusive, and that is the finding.** This bin puts
//! the call's own share at (B-A) - (D-C) = **-0.24 ns**, i.e. NEGATIVE, while
//! `crossing_price.rs` row 6 minus row 5 puts six live values across the call at
//! +1.09. Two instruments disagreeing in sign means the honest range is 0 to
//! 0.6 and the number is not usable. `docs/codegen/entry-tax.md` part two
//! records it as unsettled rather than quoting the first instrument, which is
//! what this bin exists to have made impossible.

use std::hint::black_box;
use std::time::Instant;
#[inline(never)]
extern "C" fn nothing(v: u64) -> u64 { black_box(v) }
const N: u64 = 200_000_000;
fn time(name: &str, f: impl Fn() -> u64) {
    let mut best = f64::MAX;
    for _ in 0..5 { let t = Instant::now(); black_box(f()); let ns = t.elapsed().as_nanos() as f64 / N as f64; if ns < best { best = ns; } }
    println!("{name:<52}{best:>8.3}");
}
fn main() {
    let p: extern "C" fn(u64) -> u64 = black_box(nothing);
    time("A. call, 1 live acc", || { let mut a=0u64; for i in 0..N { let r=p(black_box(i)); a=a.wrapping_add(r);} a });
    time("B. call, 6 live accs (chain)", || { let (mut a,mut b,mut c,mut d,mut e,mut f)=(0u64,1u64,2u64,3u64,4u64,5u64); for i in 0..N { let r=p(black_box(i)); a=a.wrapping_add(r); b=b.wrapping_add(a^r); c=c.wrapping_add(b); d=d.wrapping_add(c^r); e=e.wrapping_add(d); f=f.wrapping_add(e^r);} a^b^c^d^e^f });
    time("C. NO call, 1 live acc", || { let mut a=0u64; for i in 0..N { let r=black_box(i); a=a.wrapping_add(r);} a });
    time("D. NO call, 6 live accs (same chain)", || { let (mut a,mut b,mut c,mut d,mut e,mut f)=(0u64,1u64,2u64,3u64,4u64,5u64); for i in 0..N { let r=black_box(i); a=a.wrapping_add(r); b=b.wrapping_add(a^r); c=c.wrapping_add(b); d=d.wrapping_add(c^r); e=e.wrapping_add(d); f=f.wrapping_add(e^r);} a^b^c^d^e^f });
}
