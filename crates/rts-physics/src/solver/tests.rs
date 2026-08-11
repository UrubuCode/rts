//! What the solver must do, stated as behaviour rather than as calls.

use super::*;

/// A world record: fixed step, `statics` static boxes, cells of `size`.
fn world(statics: &[([f32; 3], [f32; 3])], size: f32) -> Vec<f32> {
    let mut world = vec![1.0 / 60.0, statics.len() as f32, size, 0.0];
    for (centre, half) in statics {
        world.extend_from_slice(&[centre[0], centre[1], centre[2], 0.0]);
        world.extend_from_slice(&[half[0], half[1], half[2], 0.0]);
    }
    world
}

/// A body: position, half-extent, shape, mass. `mass = 0` is immovable.
fn body(p: [f32; 3], h: [f32; 3], shape: f32, mass: f32) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let inverse = match mass > 0.0 {
        true => 1.0 / mass,
        false => 0.0,
    };
    (
        [p[0], p[1], p[2], 0.0],
        [0.0, 0.0, 0.0, shape],
        [h[0], h[1], h[2], inverse],
    )
}

/// The three buffers, from a list of bodies.
fn scene(bodies: &[([f32; 4], [f32; 4], [f32; 4])]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut pos = Vec::new();
    let mut vel = Vec::new();
    let mut ext = Vec::new();
    for (p, v, e) in bodies {
        pos.extend_from_slice(p);
        vel.extend_from_slice(v);
        ext.extend_from_slice(e);
    }
    (pos, vel, ext)
}

const BOX: f32 = 1.0;
const SPHERE: f32 = 0.0;

#[test]
fn a_body_over_nothing_accelerates_downwards_and_never_past_the_terminal_speed() {
    // High enough that ten seconds of falling never reaches the floor of the
    // world, which parks a body and would make this test measure that instead.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 500.0, 0.0], [0.5; 3], BOX, 1.0)]);
    let world = world(&[], 1.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &world, 600);
    assert!(vel[1] < -1.0, "it should be falling, vy = {}", vel[1]);
    // The anti-tunnelling ceiling: ten seconds of free fall would reach 98 u/s
    // without it, which is a body that passes through a floor in one step.
    assert!(vel[1] >= -SPEED_CAP - 1e-3, "past the cap: {}", vel[1]);
}

#[test]
fn a_body_dropped_on_a_static_floor_comes_to_rest_on_it_and_falls_asleep() {
    // Three things at once, and they are one behaviour: the solver has to stop
    // the body, hold it at the surface rather than let it sink, and stop
    // spending work on it.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 3.0, 0.0], [0.5; 3], BOX, 1.0)]);
    let floor = world(&[([0.0, -0.5, 0.0], [20.0, 0.5, 20.0])], 40.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &floor, 240);

    // Resting height is the floor's top plus the body's half-extent, less the
    // slop the solver deliberately leaves uncorrected.
    assert!(
        (pos[1] - 0.5).abs() < SLOP + 1e-2,
        "resting height was {}",
        pos[1]
    );
    assert!(dot([vel[0], vel[1], vel[2]], [vel[0], vel[1], vel[2]]) < SLEEP_SPEED_SQUARED);
    assert!(pos[3] >= SLEEP_STEPS, "sleep counter was {}", pos[3]);
}

#[test]
fn a_sphere_rests_on_a_floor_at_its_own_radius_and_not_at_its_largest_extent() {
    // The sphere-box case, and the rule that a sphere's radius is the SMALLEST
    // half-extent: a flattened extent must rest lower, not at its widest side.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 3.0, 0.0], [2.0, 0.25, 2.0], SPHERE, 1.0)]);
    let floor = world(&[([0.0, -0.5, 0.0], [20.0, 0.5, 20.0])], 40.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &floor, 240);
    assert!(
        (pos[1] - 0.25).abs() < SLOP + 1e-2,
        "resting height was {}",
        pos[1]
    );
}

#[test]
fn two_overlapping_bodies_push_each_other_apart_by_half_each() {
    // The gather half-correction: neither body is authoritative, so a symmetric
    // pair must move symmetrically. A solver that applied the whole correction
    // on one side would separate them just as well and be wrong.
    let (mut pos, mut vel, ext) = scene(&[
        body([-0.2, 0.0, 0.0], [0.5; 3], SPHERE, 1.0),
        body([0.2, 0.0, 0.0], [0.5; 3], SPHERE, 1.0),
    ]);
    let empty = world(&[], 2.0);
    let mut solver = Solver::new();
    let before = pos[4] - pos[0];
    solver.step(&mut pos, &mut vel, &ext, &empty, 1);
    let after = pos[4] - pos[0];
    assert!(after > before, "they did not separate: {before} -> {after}");
    // Symmetric: each moved the same distance from where it started.
    let left = (pos[0] - -0.2).abs();
    let right = (pos[4] - 0.2).abs();
    assert!((left - right).abs() < 1e-5, "{left} vs {right}");
}

#[test]
fn an_immovable_body_is_not_pushed_by_the_bodies_piling_on_it() {
    // Inverse mass 0 gives it a share of 0 of every correction, which is what
    // lets a scene mix a moving pile with an anchor without a second code path.
    let (mut pos, mut vel, ext) = scene(&[
        body([0.0, 0.0, 0.0], [0.5; 3], BOX, 0.0),
        body([0.1, 0.6, 0.0], [0.5; 3], BOX, 1.0),
    ]);
    let empty = world(&[], 2.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &empty, 30);
    assert!(pos[0].abs() < 1e-4, "the anchor drifted to x = {}", pos[0]);
    assert!(pos[2].abs() < 1e-4, "the anchor drifted to z = {}", pos[2]);
}

#[test]
fn a_pile_settles_instead_of_exploding() {
    // The property the Jacobi relaxation and the per-step correction ceiling
    // exist for: a body deep in a pile sums the corrections of every neighbour
    // at once, and with a sequential solver's relaxation the pile scatters.
    let mut bodies = Vec::new();
    for level in 0..8 {
        for column in 0..8 {
            bodies.push(body(
                [column as f32 * 0.9, 0.6 + level as f32 * 0.9, 0.0],
                [0.5; 3],
                BOX,
                1.0,
            ));
        }
    }
    let (mut pos, mut vel, ext) = scene(&bodies);
    let floor = world(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 2.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &floor, 600);
    for body in 0..bodies.len() {
        let x = pos[body * 4];
        let y = pos[body * 4 + 1];
        assert!(x.is_finite() && y.is_finite(), "body {body} went to NaN");
        assert!(y > FLOOR + 0.5, "body {body} fell out of the world at {y}");
        assert!(x.abs() < 40.0, "body {body} was flung to x = {x}");
    }
}

#[test]
fn the_answer_does_not_depend_on_how_many_threads_ran_it() {
    // This is the gather model's whole claim, stated as a test: a body writes
    // only itself and reads a snapshot, so the partition into threads cannot be
    // observable. Bit-for-bit, not within a tolerance — a tolerance here would
    // pass for a solver that had a race and got lucky.
    let mut bodies = Vec::new();
    for i in 0..300 {
        let f = i as f32;
        bodies.push(body(
            [(f * 0.37).sin() * 6.0, 2.0 + f * 0.31, (f * 0.71).cos() * 6.0],
            [0.5; 3],
            match i % 2 {
                0 => BOX,
                _ => SPHERE,
            },
            1.0,
        ));
    }
    let (pos, vel, ext) = scene(&bodies);
    let floor = world(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 2.0);

    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("a private pool");
        let (mut pos, mut vel) = (pos.clone(), vel.clone());
        pool.install(|| Solver::new().step(&mut pos, &mut vel, &ext, &floor, 120));
        (pos, vel)
    };

    let (serial_pos, serial_vel) = run(1);
    let (parallel_pos, parallel_vel) = run(8);
    assert_eq!(serial_pos, parallel_pos);
    assert_eq!(serial_vel, parallel_vel);
}

#[test]
fn a_body_that_leaves_the_world_is_parked_rather_than_falling_forever() {
    let (mut pos, mut vel, ext) = scene(&[body([0.0, -17.0, 0.0], [0.5; 3], SPHERE, 1.0)]);
    let empty = world(&[], 1.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &empty, 120);
    assert_eq!(pos[1], FLOOR);
    assert_eq!(vel[1], 0.0);
}

#[test]
fn a_body_arriving_with_nan_is_quarantined_instead_of_infecting_its_neighbours() {
    // NaN never sleeps — every comparison against it is false — and one
    // contaminated body reaches every neighbour it touches in the next gather.
    let (mut pos, mut vel, ext) = scene(&[
        body([f32::NAN, 1.0, 0.0], [0.5; 3], SPHERE, 1.0),
        body([0.0, 1.0, 0.0], [0.5; 3], SPHERE, 1.0),
    ]);
    let empty = world(&[], 2.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &empty, 4);
    assert!(pos.iter().all(|x| x.is_finite()), "NaN survived: {pos:?}");
    assert!(vel.iter().all(|x| x.is_finite()), "NaN survived: {vel:?}");
}

#[test]
fn the_shape_and_the_inverse_mass_survive_a_step() {
    // They live in the `w` of `vel` and `ext`, which the solver writes back and
    // reads respectively — the kernel had to be told explicitly to preserve the
    // shape, having written a constant 0 there before.
    let (mut pos, mut vel, ext) = scene(&[
        body([0.0, 5.0, 0.0], [0.5; 3], SPHERE, 2.0),
        body([4.0, 5.0, 0.0], [0.5; 3], BOX, 0.0),
    ]);
    let empty = world(&[], 1.0);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &empty, 10);
    assert_eq!(vel[3], SPHERE);
    assert_eq!(vel[7], BOX);
    assert_eq!(ext[3], 0.5);
    assert_eq!(ext[7], 0.0);
}

#[test]
fn a_sleeping_body_is_woken_by_a_fast_one_touching_it_and_not_by_a_slow_one() {
    // Sleeping is what makes a scene at rest cheap, and waking is what stops it
    // being a lie. Both halves are one behaviour and are pinned together.
    let asleep = |x: f32| ([x, 0.5, 0.0, SLEEP_STEPS], [0.0, 0.0, 0.0, BOX], [0.5, 0.5, 0.5, 1.0]);
    let mover = |x: f32, speed: f32| {
        (
            [x, 0.5, 0.0, 0.0],
            [-speed, 0.0, 0.0, BOX],
            [0.5, 0.5, 0.5, 1.0],
        )
    };
    let floor = world(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 2.0);

    let (mut pos, mut vel, ext) = scene(&[asleep(0.0), mover(0.9, 4.0)]);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &floor, 1);
    assert!(pos[3] < SLEEP_STEPS, "a struck body stayed asleep");

    // The same geometry with a slow neighbour leaves it asleep: the wake test is
    // about the neighbour's speed, not merely about being touched.
    let (mut pos, mut vel, ext) = scene(&[asleep(0.0), mover(0.9, 0.1)]);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &floor, 1);
    assert!(pos[3] >= SLEEP_STEPS, "a nudge woke it");
}
