//! The Rust solver on the SAME two scenes the TypeScript number was taken from.
//!
//! It reproduces `tools/claude-bench-onde-custa.ts` in the `rts-game` project,
//! body for body, because a benchmark that builds its own scene is not comparable
//! to the one it is quoted against — and the whole point of this number is the
//! comparison.
//!
//! - **spread** — spacing 8 between bodies of half-extent 0.5. Nothing touches,
//!   so it is the broad phase and the bookkeeping with zero pair arithmetic.
//! - **dense** — spacing 0.6. Every neighbour is a real pair.
//!
//! One sub-step per frame, which is what `resolveCollisions()` is on the other
//! side. Five warm-up frames, then sixty timed, exactly as there.
//!
//! ```text
//! cargo build --release -p rts-physics --example onde_custa
//! target/release/examples/onde_custa.exe
//! ```
//!
//! Release only. A debug number is not a number.

use rts_physics::solver::{BroadPhase, Solver};

/// The floor: 400 × 1 × 400 at y = -1, as a static box in the world record.
fn world(substeps: f32) -> Vec<f32> {
    vec![
        1.0 / 60.0,
        1.0,
        // Cell size: twice the largest DYNAMIC half-extent, which is what makes
        // the 27-cell scan exact. The floor is not counted, exactly as the GPU
        // backend's `rbMaxHalf` does not count it.
        1.0,
        substeps,
        0.0,
        -1.0,
        0.0,
        0.0,
        200.0,
        0.5,
        200.0,
        0.0,
    ]
}

/// `n` unit bodies on a square lattice of the given spacing, at y = 2.
fn scene(n: usize, spacing: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let side = (n as f32).sqrt().ceil() as usize;
    let mut pos = Vec::with_capacity(n * 4);
    let mut vel = Vec::with_capacity(n * 4);
    let mut ext = Vec::with_capacity(n * 4);
    for body in 0..n {
        let x = (body % side) as f32 * spacing;
        let z = (body / side) as f32 * spacing;
        pos.extend_from_slice(&[x, 2.0, z, 0.0]);
        // Shape 1 is a box, which is what `setMesh(1, …)` gives every body there.
        vel.extend_from_slice(&[0.0, 0.0, 0.0, 1.0]);
        ext.extend_from_slice(&[0.5, 0.5, 0.5, 1.0]);
    }
    (pos, vel, ext)
}

/// Milliseconds per frame, over sixty frames after five of warm-up.
///
/// `restore` puts the lattice back before each frame, and the first run of this
/// bench without it measured the wrong thing twice over.
///
/// A dense scene does not stay dense: the solver pushes the bodies apart, and
/// within the warm-up they have separated and gone to sleep. So sixty frames of
/// "dense" were mostly sixty frames of a settled scene taking the sleeping
/// branch — a 27-cell scan and then nothing. Restoring makes every frame do the
/// same work on the same configuration, which is the variable this bench
/// isolates and is what the TypeScript bench gets for free by never integrating
/// at all.
///
/// Both are reported. Restored is the number comparable to the TypeScript one;
/// unrestored is what a live scene actually costs once it settles, which is a
/// real saving and a different question.
fn time(n: usize, spacing: f32, restore: bool, substeps: usize, broad: BroadPhase) -> f64 {
    let (mut pos, mut vel, ext) = scene(n, spacing);
    let (base_pos, base_vel, _) = scene(n, spacing);
    let world = world(1.0);
    let mut solver = Solver::new().with_broad_phase(broad);
    for _ in 0..5 {
        solver.step(&mut pos, &mut vel, &ext, &world, substeps);
    }
    let started = std::time::Instant::now();
    for _ in 0..60 {
        if restore {
            // Two 32 KB copies at 2000 bodies. Inside the timed region rather
            // than outside it, because taking it out would report a number the
            // loop did not spend — and it is small against everything else here.
            pos.copy_from_slice(&base_pos);
            vel.copy_from_slice(&base_vel);
        }
        solver.step(&mut pos, &mut vel, &ext, &world, substeps);
    }
    started.elapsed().as_secs_f64() * 1000.0 / 60.0
}

/// What the two snapshot copies alone cost, per frame.
///
/// The solver takes a copy of `pos` and `vel` at the top of every sub-step — the
/// deliberate divergence from the GPU kernel, which reads whatever a racing
/// thread left. This times exactly those two copies and nothing else, because
/// "one copy per sub-step" is a design claim and the only honest way to state
/// what it costs is to measure it apart from the solver it is inside.
fn time_copies(n: usize) -> f64 {
    let (mut pos, mut vel, _) = scene(n, 0.6);
    let (base_pos, base_vel, _) = scene(n, 0.6);
    let started = std::time::Instant::now();
    for _ in 0..60 {
        pos.copy_from_slice(&base_pos);
        vel.copy_from_slice(&base_vel);
        std::hint::black_box((&pos, &vel));
    }
    started.elapsed().as_secs_f64() * 1000.0 / 60.0
}

/// How many neighbours each body considers, and how many of those really touch.
///
/// Printed because the table above is a comparison, and a comparison is only
/// worth as much as the claim that both sides do the same work. "Dense" is a
/// spacing, not a pair count; this is the pair count, so a reader can check that
/// the scene labelled dense is dense rather than take the label's word for it.
fn candidates(n: usize, spacing: f32) -> (f64, f64) {
    let (pos, vel, ext) = scene(n, spacing);
    let mut grid = rts_physics::solver::grid::Grid::new();
    grid.build(&pos, n, 1.0);
    let (mut near, mut touching) = (0usize, 0usize);
    for body in 0..n {
        let p = [pos[body * 4], pos[body * 4 + 1], pos[body * 4 + 2]];
        let h = [ext[body * 4], ext[body * 4 + 1], ext[body * 4 + 2]];
        grid.near(p, 1.0, |other| {
            if other == body {
                return;
            }
            near += 1;
            let q = [pos[other * 4], pos[other * 4 + 1], pos[other * 4 + 2]];
            let hq = [ext[other * 4], ext[other * 4 + 1], ext[other * 4 + 2]];
            if rts_physics::solver::contact::contact(
                p,
                h,
                vel[body * 4 + 3],
                q,
                hq,
                vel[other * 4 + 3],
            )
            .is_some()
            {
                touching += 1;
            }
        });
    }
    (near as f64 / n as f64, touching as f64 / n as f64)
}

fn main() {
    let threads = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or_else(rayon::current_num_threads);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("a pool");

    println!("[rts:rigid] ms/frame, release, one sub-step, {threads} threads");
    println!();
    println!("  n    | spread | dense | 2 sub-steps | dense, NO grid | settled");
    println!("-------+--------+-------+-------------+----------------+--------");
    for n in [500usize, 1000, 2000] {
        let (spread, dense, twice, brute, settled) = pool.install(|| {
            (
                time(n, 8.0, true, 1, BroadPhase::Grid),
                time(n, 0.6, true, 1, BroadPhase::Grid),
                // Two, because the TypeScript `resolveCollisions` this is quoted
                // against runs `resolveInto` TWICE per call. One sub-step against
                // two iterations is half the work compared with all of it.
                time(n, 0.6, true, 2, BroadPhase::Grid),
                // The reference arm: the same solver with every body considering
                // every other, which is what separates the algorithm from the
                // language and the threads.
                time(n, 0.6, true, 1, BroadPhase::Everything),
                time(n, 0.6, false, 1, BroadPhase::Grid),
            )
        });
        println!(
            "  {:<5}|{:>7.2} |{:>6.2} |{:>12.2} |{:>15.2} |{:>7.2}",
            n, spread, dense, twice, brute, settled
        );
    }
    println!();
    println!("  the two snapshot copies alone, ms/frame:");
    for n in [500usize, 1000, 2000] {
        println!("  {:<5}| {:.4}", n, time_copies(n));
    }
    println!();
    println!("  per body, in the scene as built: candidates / actually touching");
    for n in [500usize, 1000, 2000] {
        let (near, touching) = candidates(n, 0.6);
        let (near_spread, _) = candidates(n, 8.0);
        println!("  {n:<5}| dense {near:.1} / {touching:.1}   spread {near_spread:.1} / 0");
    }
}
