//! What the dense scene ACTUALLY is at each size — and where the broad phase
//! stops describing the same physics.
//!
//! ```text
//! cargo build --release -p rts-physics --example audit_denso
//! target/release/examples/audit_denso.exe
//! ```
//!
//! # Why this is a separate program from the timing bench
//!
//! Because a timing table that grows `n` is answering "how does the cost scale"
//! only while the scene stays the same KIND of scene. Two things can quietly
//! stop being true as the lattice grows, and both change what is being simulated
//! rather than what it costs:
//!
//! - **Bucket overflow.** A cell records 32 bodies and drops the rest, exactly as
//!   the GPU kernel does. Past that point real pairs are lost, both backends lose
//!   different ones, and the number on that row is not a measurement of the same
//!   physics as the row above it.
//! - **Hash collision.** `cell_hash` folds the 3D cell coordinate into 8192
//!   buckets. A big enough lattice has more distinct cells than buckets, so
//!   unrelated regions share a bucket — which costs candidate tests without
//!   losing pairs, and inflates occupancy toward the overflow above.
//!
//! Both are measured here through the REAL `Grid`, not through a copy of it: a
//! second implementation of the hash would answer about itself.

use rts_physics::solver::contact::contact;
use rts_physics::solver::grid::{CELLS, Grid, SLOTS};

/// The dense lattice the bench uses: spacing 0.6 over half-extent 0.5, cubic.
const SPACING: f32 = 0.6;
const HALF: f32 = 0.5;
/// Cell size is twice the largest half-extent, which is what makes the 27-cell
/// scan exact. Same value the bench writes into `world[2]`.
const CELL: f32 = 2.0 * HALF;

fn scene(n: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let side = (n as f64).cbrt().ceil() as usize;
    let mut pos = Vec::with_capacity(n * 4);
    let mut vel = Vec::with_capacity(n * 4);
    let mut ext = Vec::with_capacity(n * 4);
    for body in 0..n {
        let x = (body % side) as f32 * SPACING;
        let z = ((body / side) % side) as f32 * SPACING;
        let y = 2.0 + (body / (side * side)) as f32 * SPACING;
        pos.extend_from_slice(&[x, y, z, 0.0]);
        vel.extend_from_slice(&[0.0, 0.0, 0.0, 1.0]);
        ext.extend_from_slice(&[HALF, HALF, HALF, 1.0]);
    }
    (pos, vel, ext)
}

fn main() {
    println!("[audit] a cena densa em cada n — pelo Grid REAL, nao por uma copia");
    println!("  espacamento {SPACING}, meia-extensao {HALF}, celula {CELL}");
    println!("  o bucket guarda {SLOTS} corpos e DESCARTA o resto, como o kernel");
    println!();
    println!("   n     | celulas | buckets | pior  | candidatos | contatos | DESCARTE");
    println!("         | usadas  | usados  |bucket | por corpo  | por corpo|");
    println!("---------+---------+---------+-------+------------+----------+---------");

    for n in [250usize, 1000, 2000, 4000, 8000, 16000, 32000, 64000, 128000] {
        let (pos, vel, ext) = scene(n);
        let mut grid = Grid::new();
        grid.build(&pos, n, CELL);

        // Occupancy through the real grid. `bucket` answers what a body would
        // actually see, so overflow is visible as a bucket pinned at SLOTS.
        let mut used = 0usize;
        let mut worst = 0usize;
        let mut held = 0usize;
        for cell in 0..CELLS {
            let count = grid.bucket(cell).len();
            if count > 0 {
                used += 1;
            }
            held += count;
            worst = worst.max(count);
        }
        // What the grid DROPPED: every body is recorded once or not at all, so
        // the bodies it holds against the bodies there are is the whole story.
        let dropped = n - held;

        // How many distinct cells the lattice occupies, independent of hashing.
        // The gap between this and `used` is collision, which costs candidates
        // without losing pairs — the two are different failures and are counted
        // apart for that reason.
        let side = (n as f64).cbrt().ceil() as usize;
        let span = ((side as f32 - 1.0) * SPACING / CELL).floor() as usize + 1;
        let distinct = span * span * span;

        let sample = n.min(400);
        let (mut near, mut touching) = (0usize, 0usize);
        for body in 0..sample {
            let p = [pos[body * 4], pos[body * 4 + 1], pos[body * 4 + 2]];
            let h = [ext[body * 4], ext[body * 4 + 1], ext[body * 4 + 2]];
            grid.near(p, CELL, |other| {
                if other == body {
                    return;
                }
                near += 1;
                let q = [pos[other * 4], pos[other * 4 + 1], pos[other * 4 + 2]];
                let hq = [ext[other * 4], ext[other * 4 + 1], ext[other * 4 + 2]];
                if contact(p, h, vel[body * 4 + 3], q, hq, vel[other * 4 + 3]).is_some() {
                    touching += 1;
                }
            });
        }
        let mark = match dropped {
            0 => "-".to_string(),
            _ => format!("{dropped} corpos"),
        };
        println!(
            "  {:<7}|{:>8} |{:>8} |{:>6} |{:>11.1} |{:>9.1} | {}",
            n,
            distinct,
            used,
            worst,
            near as f64 / sample as f64,
            touching as f64 / sample as f64,
            mark
        );
    }

    println!();
    println!("  'celulas usadas' > 'buckets usados' = COLISAO de hash: regioes");
    println!("  distantes dividem bucket. Custa candidato, nao perde par.");
    println!("  'DESCARTE' != 0 = o bucket estourou as {SLOTS} vagas e PERDEU par.");
    println!("  A partir dai a linha nao mede a mesma fisica da linha acima.");
}
