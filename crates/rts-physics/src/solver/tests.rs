//! What the solver must do, stated as behaviour rather than as calls.

use super::*;

/// A world record: fixed step, `statics` static boxes, cells of `size`.
///
/// It stops where it always did, at the end of the static block, which makes
/// every test written before materials existed a test of their ABSENCE too —
/// the legacy defaults are what all of these assert against.
fn world(statics: &[([f32; 3], [f32; 3])], size: f32) -> Vec<f32> {
    let mut world = vec![
        1.0 / 60.0,
        statics.len() as f32,
        size,
        1.0,
        material::PHYSICS_LAYOUT_VERSION,
        0.0,
        0.0,
        0.0,
    ];
    for (centre, half) in statics {
        world.extend_from_slice(&[centre[0], centre[1], centre[2], 0.0]);
        world.extend_from_slice(&[half[0], half[1], half[2], 0.0]);
    }
    world
}

/// The same, with room for a material region, and every record at the legacy
/// value — so a test changes the ONE number it is about and nothing else.
fn world_with_materials(statics: &[([f32; 3], [f32; 3])], size: f32, bodies: usize) -> Vec<f32> {
    let mut world = world(statics, size);
    world.resize(material::MATERIALS_AT, 0.0);
    let def_layer = f32::from_bits(1);
    let def_mask = f32::from_bits(0xFFFF_FFFF);
    for _ in 0..256 {
        world.extend_from_slice(&[0.0, 0.35, def_layer, def_mask]);
    }
    for _ in 0..bodies {
        world.extend_from_slice(&[9.8, 0.0, 0.0, 0.35, -1.0e30, material::BODY_DYNAMIC, def_layer, def_mask]);
    }
    world
}

/// The five numbers of body `i`'s material, to overwrite in place.
fn body_material(world: &mut [f32], i: usize) -> &mut [f32] {
    let at = material::MATERIALS_AT + 256 * 4 + i * 8;
    &mut world[at..at + 5]
}

/// The two numbers of static `k`'s material.
fn static_material(world: &mut [f32], k: usize) -> &mut [f32] {
    let at = material::MATERIALS_AT + k * 4;
    &mut world[at..at + 2]
}

/// Marks static `k` ROUND: `w` of its centre. See the layout note in the
/// module doc for why 1 is the sphere and 0 the box.
fn round(world: &mut [f32], k: usize) {
    world[(2 + k * 2) * 4 + 3] = 1.0;
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

// ── materials: what a body and a static are MADE of ────────────────────────

#[test]
fn a_world_that_stops_at_the_statics_simulates_exactly_as_it_did_before_materials() {
    // The rule that makes the region safe to add: an older caller's buffer must
    // produce the SAME trajectory, bit for bit, or every number measured against
    // the GPU backend stops meaning anything.
    let start = body([0.0, 6.0, 0.0], [0.5; 3], BOX, 1.0);
    let floor = ([0.0, 0.0, 0.0], [10.0, 0.5, 10.0]);

    let (mut short_pos, mut short_vel, ext) = scene(&[start]);
    Solver::new().step(&mut short_pos, &mut short_vel, &ext, &world(&[floor], 2.0), 120);

    let (mut long_pos, mut long_vel, ext) = scene(&[start]);
    let long = world_with_materials(&[floor], 2.0, 1);
    Solver::new().step(&mut long_pos, &mut long_vel, &ext, &long, 120);

    assert_eq!(short_pos, long_pos, "the same scene landed somewhere else");
    assert_eq!(short_vel, long_vel);
}

#[test]
fn a_body_whose_material_says_no_gravity_hangs_where_it_was_put() {
    // The case with no workaround before this: a floating platform, a scripted
    // mover, anything the game moves itself. Its only alternative was `stationary`,
    // which also stops it colliding with what rests on it.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 6.0, 0.0], [0.5; 3], BOX, 1.0)]);
    let mut world = world_with_materials(&[], 1.0, 1);
    body_material(&mut world, 0)[0] = 0.0;
    Solver::new().step(&mut pos, &mut vel, &ext, &world, 120);
    assert_eq!(pos[1], 6.0, "it moved: y = {}", pos[1]);
}

#[test]
fn a_bouncy_body_leaves_the_floor_again_and_a_dead_one_does_not() {
    // Restitution reaches the solver at all, and reaches it from the BODY: the
    // two runs differ in one number and nowhere else.
    let start = body([0.0, 6.0, 0.0], [0.5; 3], SPHERE, 1.0);
    let floor = ([0.0, 0.0, 0.0], [10.0, 0.5, 10.0]);
    let mut peak = [0.0f32; 2];
    for (which, restitution) in [(0usize, 0.0f32), (1, 0.9)] {
        let (mut pos, mut vel, ext) = scene(&[start]);
        let mut world = world_with_materials(&[floor], 2.0, 1);
        body_material(&mut world, 0)[1] = restitution;
        let mut solver = Solver::new();
        // One step at a time, and the peak counted only AFTER the first landing:
        // the body starts at 6, so a peak measured from the beginning is the
        // drop itself on both runs and the test compares nothing.
        let mut landed = false;
        for _ in 0..240 {
            solver.step(&mut pos, &mut vel, &ext, &world, 1);
            landed = landed || pos[1] < 1.0;
            if landed {
                peak[which] = peak[which].max(pos[1]);
            }
        }
    }
    assert!(peak[1] > peak[0] + 0.5, "bounce {} vs dead {}", peak[1], peak[0]);
}

#[test]
fn drag_takes_the_speed_of_a_body_that_nothing_else_slows() {
    // Sideways, where gravity does not reach: drag on its own.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 500.0, 0.0], [0.5; 3], SPHERE, 1.0)]);
    vel[0] = 10.0;
    let mut world = world_with_materials(&[], 1.0, 1);
    body_material(&mut world, 0)[0] = 0.0;
    body_material(&mut world, 0)[2] = 4.0;
    Solver::new().step(&mut pos, &mut vel, &ext, &world, 120);
    // Under the sleep threshold rather than under zero, and the difference is
    // the point: drag carries the body down to a crawl, and SLEEP is what ends
    // the decay there — below 0.45 u/s for ten steps and nothing integrates it
    // any more. A test demanding 0.0 would be demanding that sleeping not work.
    assert!(vel[0] < 0.45, "drag did not bite: vx = {}", vel[0]);
    assert!(vel[0] >= 0.0, "drag reversed the body: vx = {}", vel[0]);
}

#[test]
fn a_body_with_its_own_floor_rests_on_it_with_no_static_in_the_scene() {
    // The Rigidbody's implicit ground, which the game side integrated itself
    // until this backend took the body over.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 6.0, 0.0], [0.5; 3], BOX, 1.0)]);
    let mut world = world_with_materials(&[], 1.0, 1);
    body_material(&mut world, 0)[4] = 2.0;
    Solver::new().step(&mut pos, &mut vel, &ext, &world, 300);
    assert!((pos[1] - 2.0).abs() < 1e-3, "it should rest at 2.0: y = {}", pos[1]);
}

#[test]
fn ice_lets_a_body_slide_where_rubber_stops_it() {
    // Friction reaches the solver from BOTH sides of the contact: the two runs
    // differ only in the floor's number, and the body's is the reference one.
    let floor = ([0.0, 0.0, 0.0], [40.0, 0.5, 40.0]);
    let mut travelled = [0.0f32; 2];
    for (which, friction) in [(0usize, 0.02f32), (1, 1.0)] {
        let (mut pos, mut vel, ext) = scene(&[body([0.0, 1.0, 0.0], [0.5; 3], BOX, 1.0)]);
        vel[0] = 12.0;
        let mut world = world_with_materials(&[floor], 2.0, 1);
        static_material(&mut world, 0)[1] = friction;
        Solver::new().step(&mut pos, &mut vel, &ext, &world, 180);
        travelled[which] = pos[0];
    }
    assert!(
        travelled[0] > travelled[1] * 2.0,
        "ice {} should carry much further than rubber {}",
        travelled[0],
        travelled[1]
    );
}

#[test]
fn a_round_static_is_round_and_a_body_lands_on_top_of_it_rather_than_inside() {
    // A spherical static was silently a box: a ball dropped on a dome landed on
    // a flat lid at the dome's full height. The contact is what moves, so the
    // test is the RESTING HEIGHT — a box of half-extent 2 holds the body at 2.5,
    // a sphere of radius 2 at the same spot holds it at 2.5 too but only over
    // the pole, and rejects it sideways. Dropping off-centre is what tells them
    // apart: on the box the body rests flat, off the sphere it slides away.
    let dome = ([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let mut drift = [0.0f32; 2];
    for which in 0..2 {
        let (mut pos, mut vel, ext) = scene(&[body([1.2, 5.0, 0.0], [0.5; 3], SPHERE, 1.0)]);
        let mut world = world_with_materials(&[dome], 4.0, 1);
        if which == 1 {
            round(&mut world, 0);
        }
        Solver::new().step(&mut pos, &mut vel, &ext, &world, 180);
        drift[which] = pos[0];
    }
    assert!((drift[0] - 1.2).abs() < 0.05, "on a box it should sit still: x = {}", drift[0]);
    assert!(drift[1] > 1.6, "off a sphere it should slide: x = {}", drift[1]);
}

#[test]
fn a_bouncing_body_does_not_fall_asleep_in_mid_air() {
    // Sleeping asked only about SPEED, and at the top of a bounce a body is slow
    // for as many steps as the arc is shallow. Ten of those and it hung there,
    // awake to nothing — a sleeping body is only woken by a fast neighbour, and
    // empty space is not one. A dropped box with bounce 0.5 stopped at y = 1.135
    // and stayed for as long as anyone watched.
    //
    // Unreachable while restitution was a constant zero, which is why it arrived
    // with the material region rather than with the sleep rule.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 6.0, 0.0], [0.5; 3], BOX, 1.0)]);
    let floor = ([0.0, 0.0, 0.0], [10.0, 0.5, 10.0]);
    let mut world = world_with_materials(&[floor], 2.0, 1);
    body_material(&mut world, 0)[1] = 0.5;
    Solver::new().step(&mut pos, &mut vel, &ext, &world, 900);
    // On the floor (half-extents 0.5 + 0.5), not hanging above it.
    assert!(pos[1] < 1.1, "it fell asleep in the air at y = {}", pos[1]);
    assert!(pos[1] > 0.8, "it sank into the floor: y = {}", pos[1]);
}

#[test]
fn a_kinematic_body_moves_by_velocity_with_no_gravity_or_drag_and_never_sleeps() {
    // Kinematic (body_type = 2.0): moves strictly by p += v * dt.
    // Velocity must be preserved exactly, position must advance linearly,
    // gravity and drag must not affect it.
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 10.0, 0.0], [1.0; 3], BOX, 0.0)]);
    vel[0] = 5.0; // vx = 5.0
    vel[1] = 0.0; // vy = 0.0
    let mut world = world_with_materials(&[], 4.0, 1);
    // Set body_type = 2.0 (kinematic)
    body_material(&mut world, 0)[0] = 9.8; // gravity declared
    body_material(&mut world, 0)[2] = 0.5; // drag declared
    let at = material::MATERIALS_AT + 256 * 4;
    world[at + 5] = material::BODY_KINEMATIC; // 2.0!

    let dt = world[0];
    let steps = 60;
    Solver::new().step(&mut pos, &mut vel, &ext, &world, steps);

    let expected_x = 5.0 * (steps as f32) * dt;
    assert!((pos[0] - expected_x).abs() < 1e-3, "x was {}, expected {}", pos[0], expected_x);
    assert!((pos[1] - 10.0).abs() < 1e-3, "y was {} (should stay 10.0, no gravity)", pos[1]);
    assert!((vel[0] - 5.0).abs() < 1e-3, "vx was {} (should stay 5.0, no drag)", vel[0]);
    assert_eq!(vel[1], 0.0, "vy was {} (should stay 0.0)", vel[1]);
    assert_eq!(pos[3], 0.0, "kinematic bodies must not sleep");
}

#[test]
fn a_dynamic_body_resting_on_a_moving_kinematic_platform_is_carried_along() {
    // Platform (body 0, kinematic): y = 0.0, h = [5.0, 0.5, 5.0], vx = 3.0
    // Dynamic body (body 1): dropped at y = 1.0, h = [0.5, 0.5, 0.5], mass = 1.0
    let (mut pos, mut vel, ext) = scene(&[
        body([0.0, 0.0, 0.0], [5.0, 0.5, 5.0], BOX, 0.0), // Kinematic platform
        body([0.0, 1.0, 0.0], [0.5, 0.5, 0.5], BOX, 1.0), // Dynamic body resting on it
    ]);
    vel[0] = 3.0; // platform moves at vx = 3.0

    let mut world = world_with_materials(&[], 10.0, 2);
    // Body 0 is kinematic (tipo = 2.0)
    let at0 = material::MATERIALS_AT + 256 * 4;
    world[at0 + 5] = material::BODY_KINEMATIC;
    // Body 1 is dynamic (tipo = 3.0)
    let at1 = material::MATERIALS_AT + 256 * 4 + 8;
    world[at1 + 5] = material::BODY_DYNAMIC;

    let mut solver = Solver::new();
    // Step 120 steps (~2 seconds)
    solver.step(&mut pos, &mut vel, &ext, &world, 120);

    // Platform moved to x = 3.0 * (120/60) = 6.0
    assert!((pos[0] - 6.0).abs() < 0.05, "platform x = {}, expected 6.0", pos[0]);
    assert!((vel[0] - 3.0).abs() < 1e-3, "platform vx = {}", vel[0]);

    // Dynamic body on top (body 1) must stay around y = 1.0 (top of platform 0.5 + half 0.5)
    assert!((pos[5] - 1.0).abs() < 0.2, "dynamic body y = {}, expected ~1.0", pos[5]);
    // Dynamic body must have been carried horizontally along with platform
    assert!(pos[4] > 4.0, "dynamic body x = {} was not carried by platform", pos[4]);
}

#[test]
fn unassigned_body_type_0_defaults_to_dynamic() {
    let (mut pos, mut vel, ext) = scene(&[body([0.0, 10.0, 0.0], [0.5; 3], BOX, 1.0)]);
    let mut world = world_with_materials(&[], 4.0, 1);
    let at = material::MATERIALS_AT + 256 * 4;
    world[at + 5] = 0.0; // unassigned
    body_material(&mut world, 0)[0] = 9.8; // gravity

    Solver::new().step(&mut pos, &mut vel, &ext, &world, 60);
    // Body should fall under gravity because 0.0 defaults to dynamic
    assert!(pos[1] < 9.0, "unassigned body type 0.0 did not fall under gravity: y = {}", pos[1]);
}

#[test]
fn layer_and_mask_filters_collision_pairs() {
    // Two overlapping bodies: body 0 at x=0, body 1 at x=0.5 (both h=0.5)
    let (mut pos, mut vel, ext) = scene(&[
        body([0.0, 0.0, 0.0], [0.5; 3], BOX, 1.0),
        body([0.5, 0.0, 0.0], [0.5; 3], BOX, 1.0),
    ]);
    let mut world = world_with_materials(&[], 4.0, 2);
    let at0 = material::MATERIALS_AT + 256 * 4;
    let at1 = material::MATERIALS_AT + 256 * 4 + 8;

    // Set body 0: layer = 1, mask = 2 (only collides with layer 2)
    world[at0 + 6] = f32::from_bits(1);
    world[at0 + 7] = f32::from_bits(2);

    // Set body 1: layer = 4, mask = 1 (only collides with layer 1, but body 0 is layer 1 and body 1 is layer 4 != mask 2)
    world[at1 + 6] = f32::from_bits(4);
    world[at1 + 7] = f32::from_bits(1);

    // 1. With any_mask = 0.0, the solver skips the mask filter (recurso desligado custa zero):
    world[material::WORLD_PARAM_ANY_MASK] = 0.0;
    let initial_x0 = pos[0];
    let initial_x1 = pos[4];
    Solver::new().step(&mut pos, &mut vel, &ext, &world, 10);
    assert!(pos[0] < initial_x0, "with any_mask=0 bodies should separate regardless of masks");
    assert!(pos[4] > initial_x1, "with any_mask=0 bodies should separate regardless of masks");

    // Reset positions
    pos[0] = initial_x0;
    pos[4] = initial_x1;

    // 2. With any_mask = 1.0, masks ARE checked: (2 & 4) == 0 -> no collision!
    world[material::WORLD_PARAM_ANY_MASK] = 1.0;
    Solver::new().step(&mut pos, &mut vel, &ext, &world, 10);
    assert_eq!(pos[0], initial_x0, "body 0 was moved despite mask mismatch");
    assert_eq!(pos[4], initial_x1, "body 1 was moved despite mask mismatch");

    // Now change body 1 layer to 2 (so body 0 mask 2 matches body 1 layer 2, AND body 1 mask 1 matches body 0 layer 1)
    world[at1 + 6] = f32::from_bits(2);
    Solver::new().step(&mut pos, &mut vel, &ext, &world, 10);
    assert!(pos[0] < initial_x0, "body 0 did not separate when masks matched");
    assert!(pos[4] > initial_x1, "body 1 did not separate when masks matched");
}

