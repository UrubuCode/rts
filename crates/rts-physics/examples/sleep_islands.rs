//! Reproducible cost of settled islands versus rebuilding their contact graph.
//! Run: cargo run --release -p rts-physics --example sleep_islands
use rts_physics::solver::Solver;
use std::time::Instant;

fn main() {
    let n = 2000;
    let mut pos = vec![0.0; n * 4];
    let mut vel = vec![0.0; n * 4];
    let mut ext = vec![0.5; n * 4];
    for i in 0..n {
        pos[i * 4] = (i % 40) as f32 * 1.3 - 26.0;
        pos[i * 4 + 1] = 0.47 + (i / 1000) as f32 * 0.97;
        pos[i * 4 + 2] = ((i / 40) % 25) as f32 * 1.3 - 16.0;
        pos[i * 4 + 3] = 10.0;
        vel[i * 4 + 3] = 1.0;
        ext[i * 4 + 3] = 1.0;
    }
    // Full layout-v1 world, including the materials/masks/quaternion tail used
    // by cpurigid.ts. The idle measurement includes comparison of all buffers.
    let materials_at = 8 + 256 * 8;
    let bodies_at = materials_at + 256 * 4;
    let orientations_at = bodies_at + n * 8;
    let mut world = vec![0.0; orientations_at + (256 + n) * 4];
    world[..16].copy_from_slice(&[
        1.0 / 60.0,
        1.0,
        1.733,
        1.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        -0.5,
        0.0,
        0.0,
        100.0,
        0.5,
        100.0,
        0.0,
    ]);
    for i in 0..256 {
        world[materials_at + i * 4..materials_at + (i + 1) * 4].copy_from_slice(&[
            0.0,
            0.35,
            f32::from_bits(1),
            f32::from_bits(u32::MAX),
        ]);
    }
    for i in 0..n {
        world[bodies_at + i * 8..bodies_at + (i + 1) * 8].copy_from_slice(&[
            9.8,
            0.0,
            0.0,
            0.35,
            -1.0e30,
            3.0,
            f32::from_bits(1),
            f32::from_bits(u32::MAX),
        ]);
    }
    for i in 0..256 + n {
        world[orientations_at + i * 4 + 3] = 1.0;
    }
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &world, 1);
    assert_eq!(solver.grid_overflows(), 0);
    assert!(pos.chunks_exact(4).all(|p| p[3] >= 10.0));
    let rounds = 2000;
    let start = Instant::now();
    for _ in 0..rounds {
        solver.step(&mut pos, &mut vel, &ext, &world, 1);
    }
    let sleeping_ms = start.elapsed().as_secs_f64() * 1000.0 / rounds as f64;
    let (settled_pos, settled_vel) = (pos.clone(), vel.clone());
    let rounds = 200;
    let start = Instant::now();
    for _ in 0..rounds {
        pos.copy_from_slice(&settled_pos);
        vel.copy_from_slice(&settled_vel);
        // An explicit wake command invalidates the idle cache and rebuilds the
        // whole graph. Includes the reset copies, stated here for reproducibility.
        pos[3] = 0.0;
        solver.step(&mut pos, &mut vel, &ext, &world, 1);
    }
    let rebuilding_ms = start.elapsed().as_secs_f64() * 1000.0 / rounds as f64;
    println!("bodies={n} threads={} overflow={} sleeping_ms={sleeping_ms:.6} graph_and_step_ms={rebuilding_ms:.6}",
        rayon::current_num_threads(), solver.grid_overflows());
}
