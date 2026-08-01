//! Timing harness. `setup` runs OUTSIDE the timer — an early version reset the
//! slab inside the timed closure, which charged every allocation variant for a
//! teardown the real engine does not do per loop.

use std::time::Instant;

pub const RUNS: usize = 7;

pub struct Row {
    pub name: &'static str,
    pub detail: &'static str,
    /// Restores whatever the kernel mutates. Not timed.
    pub setup: Box<dyn Fn()>,
    pub run: Box<dyn Fn() -> i64>,
}

impl Row {
    pub fn new(
        name: &'static str,
        detail: &'static str,
        run: impl Fn() -> i64 + 'static,
    ) -> Self {
        Row {
            name,
            detail,
            setup: Box::new(|| {}),
            run: Box::new(run),
        }
    }

    pub fn with_setup(mut self, setup: impl Fn() + 'static) -> Self {
        self.setup = Box::new(setup);
        self
    }
}

/// How the returned word is turned back into the number to check.
#[derive(Clone, Copy, PartialEq)]
pub enum Check {
    /// The kernel returns a PolyValue word (or `f64` bits).
    Poly,
    /// The kernel returns a plain integer count.
    Int,
    /// No checksum is meaningful for this kernel.
    None,
}

pub fn report(title: &str, iters: i64, expect: f64, check: Check, rows: Vec<Row>) {
    println!("{title}");
    println!("{}", "-".repeat(title.len().min(90)));
    println!(
        "{:<26} {:>10} {:>10} {:>9}  {}",
        "variant", "ms (med)", "ns/iter", "vs floor", "what it does"
    );

    let mut results = Vec::new();
    for row in &rows {
        for _ in 0..2 {
            (row.setup)();
            (row.run)();
        }
        let mut times = Vec::with_capacity(RUNS);
        let mut checksum = 0i64;
        for _ in 0..RUNS {
            (row.setup)();
            let t0 = Instant::now();
            let c = (row.run)();
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
            checksum = c;
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = times[RUNS / 2];
        let got = match check {
            Check::Poly => crate::poly::to_number(checksum as u64),
            Check::Int => checksum as f64,
            Check::None => expect,
        };
        let ok = check == Check::None || (got - expect).abs() <= expect.abs() * 1e-9;
        results.push((row.name, row.detail, med, ok, got));
    }

    let floor = results
        .iter()
        .map(|r| r.2)
        .fold(f64::INFINITY, f64::min)
        .max(1e-9);

    for (name, detail, med, ok, got) in &results {
        let ns = med * 1e6 / iters as f64;
        let mark = if *ok { "" } else { "  <-- CHECKSUM MISMATCH" };
        println!(
            "{:<26} {:>10.2} {:>10.2} {:>8.2}x  {}{}",
            name,
            med,
            ns,
            med / floor,
            detail,
            mark
        );
        if !*ok {
            println!("      expected {expect:.9e}, got {got:.9e}");
        }
    }
    println!();
}
