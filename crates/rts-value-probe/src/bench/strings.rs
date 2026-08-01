//! String driver. Unlike the other kernels this one is RUST-SIDE, not JITted:
//! every variant emits the identical `call __rtsadp_add`, so the emitted code is
//! not what differs — the runtime implementation behind it is. Driving it
//! through the JIT would add a constant call overhead to all rows and measure
//! nothing extra.

use crate::harness::{Check, Row, report};
use crate::rt::strings;
use crate::slab;

/// `s = s + "x"` repeated. The classic accumulator loop — and the one where an
/// O(len) copy per append turns into O(n²).
const APPENDS: i64 = 10_000;
/// Equality is measured on strings long enough that a memcmp is not free but
/// short enough to be a realistic key/identifier compare.
const EQ_ROUNDS: i64 = 2_000_000;
const EQ_LEN: usize = 24;

pub fn kernel_string_append() {
    let expect = (APPENDS as f64) * (APPENDS as f64 + 1.0) / 2.0; // total bytes seen

    report(
        "KERNEL STR-APPEND — s = s + \"x\", 10k appends (the quadratic loop)",
        APPENDS,
        expect,
        Check::None,
        vec![
            Row::new(
                "D0 today",
                "STRING_CONCAT via the snapshot layer (copies each side twice)",
                || {
                    slab::sharded::reset();
                    let x = strings::new_string(b"x");
                    let mut s = strings::new_string(b"");
                    for _ in 0..APPENDS {
                        s = strings::concat_today(s, x);
                    }
                    strings::len_of(s) as i64
                },
            ),
            Row::new(
                "D1 concat, no snapshot",
                "same immutable result, each side copied once",
                || {
                    slab::sharded::reset();
                    let x = strings::new_string(b"x");
                    let mut s = strings::new_string(b"");
                    for _ in 0..APPENDS {
                        s = strings::concat_direct(s, x);
                    }
                    strings::len_of(s) as i64
                },
            ),
            Row::new(
                "D2 append in place",
                "mutable accumulator (needs a liveness proof)",
                || {
                    slab::sharded::reset();
                    let x = strings::new_string(b"x");
                    let s = strings::new_string(b"");
                    for _ in 0..APPENDS {
                        strings::append_in_place(s, x);
                    }
                    strings::len_of(s) as i64
                },
            ),
            Row::new("D3 Rust String floor", "push_str into an owned String", || {
                let mut s = String::new();
                for _ in 0..APPENDS {
                    s.push_str("x");
                }
                s.len() as i64
            }),
        ],
    );
}

pub fn kernel_string_eq() {
    let a: Vec<u8> = vec![b'k'; EQ_LEN];
    let mut b = a.clone();
    b[EQ_LEN - 1] = b'z'; // differs only at the END — the worst case for memcmp

    slab::sharded::reset();
    let ha = strings::new_string(&a);
    let hb = strings::new_string(&b);
    let a2 = a.clone();
    let b2 = b.clone();

    report(
        "KERNEL STR-EQ — a === b on two 24-byte strings differing in the last byte",
        EQ_ROUNDS,
        0.0,
        Check::Int,
        vec![
            // `black_box` on the operands: without it E1/E2 fold to a constant
            // and time 0.00 ms, which is not a measurement of anything.
            Row::new(
                "E0 today",
                "handle check, then content compare under lock",
                move || {
                    let mut n = 0i64;
                    for _ in 0..EQ_ROUNDS {
                        n += i64::from(strings::eq_today(
                            std::hint::black_box(ha),
                            std::hint::black_box(hb),
                        ));
                    }
                    n
                },
            ),
            Row::new(
                "E1 interned",
                "equal content implies equal handle: one integer compare",
                move || {
                    let mut n = 0i64;
                    for _ in 0..EQ_ROUNDS {
                        n += i64::from(strings::eq_interned(
                            std::hint::black_box(ha),
                            std::hint::black_box(hb),
                        ));
                    }
                    n
                },
            ),
            Row::new("E2 raw memcmp floor", "bytes already in hand", move || {
                let mut n = 0i64;
                for _ in 0..EQ_ROUNDS {
                    n += i64::from(strings::eq_raw(
                        std::hint::black_box(&a2),
                        std::hint::black_box(&b2),
                    ));
                }
                n
            }),
        ],
    );
}
