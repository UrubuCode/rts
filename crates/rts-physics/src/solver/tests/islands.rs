use super::*;

// Include type and filters, beyond the older five material coefficients.
fn body_material(world: &mut [f32], body: usize) -> &mut [f32] {
    let at = material::MATERIALS_AT + 256 * 4 + body * 8;
    &mut world[at..at + 8]
}

fn floor() -> Vec<f32> {
    world(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 2.0)
}

fn sleeping_boxes(points: &[[f32; 3]]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let bodies: Vec<_> = points
        .iter()
        .map(|&p| body(p, [0.5; 3], BOX, 1.0))
        .collect();
    let (mut pos, vel, ext) = scene(&bodies);
    for record in pos.chunks_exact_mut(4) {
        record[3] = SLEEP_STEPS;
    }
    (pos, vel, ext)
}

#[test]
fn touching_bodies_accumulate_quiet_time_and_sleep_together() {
    let (mut pos, mut vel, ext) = sleeping_boxes(&[[0.0, 0.47, 0.0], [0.98, 0.47, 0.0]]);
    pos[3] = SLEEP_STEPS - 1.0;
    pos[7] = 0.0;
    let mut solver = Solver::new();
    for step in 1..=10 {
        solver.step(&mut pos, &mut vel, &ext, &floor(), 1);
        assert_eq!(pos[3], pos[7], "island split on step {step}");
        assert_eq!(
            pos[3] >= SLEEP_STEPS,
            step == 10,
            "sleep deadline at {step}"
        );
    }
    assert_eq!(&vel[0..3], &[0.0; 3]);
    assert_eq!(&vel[4..7], &[0.0; 3]);
}

#[test]
fn a_slow_nudge_reaches_the_whole_stack_and_not_a_distant_stack() {
    let (mut pos, mut vel, ext) = sleeping_boxes(&[
        [0.0, 0.47, 0.0],
        [0.0, 1.45, 0.0],
        [0.0, 2.43, 0.0],
        [5.0, 0.47, 0.0],
        [5.0, 1.45, 0.0],
    ]);
    // Even if the caller forgets to clear the sleep counter, the impulse wakes.
    vel[0] = 0.1;
    Solver::new().step(&mut pos, &mut vel, &ext, &floor(), 1);
    for i in 0..3 {
        assert!(pos[i * 4 + 3] < SLEEP_STEPS, "body {i} stayed asleep");
    }
    for i in 3..5 {
        assert!(
            pos[i * 4 + 3] >= SLEEP_STEPS,
            "the floor joined separate islands"
        );
    }
}

#[test]
fn removing_the_floor_wakes_the_entire_supported_stack() {
    let (mut pos, mut vel, ext) =
        sleeping_boxes(&[[0.0, 0.47, 0.0], [0.0, 1.45, 0.0], [0.0, 2.43, 0.0]]);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &floor(), 1);
    assert!(pos.chunks_exact(4).all(|p| p[3] >= SLEEP_STEPS));
    let before = pos.clone();
    solver.step(&mut pos, &mut vel, &ext, &world(&[], 2.0), 1);
    for i in 0..3 {
        assert!(
            pos[i * 4 + 3] < SLEEP_STEPS,
            "unsupported body {i} stayed asleep"
        );
        assert!(pos[i * 4 + 1] < before[i * 4 + 1], "body {i} did not fall");
    }
}

#[test]
fn an_unanchored_falling_cluster_cannot_sleep_on_its_own_contacts() {
    let (mut pos, mut vel, ext) = sleeping_boxes(&[[0.0, 5.0, 0.0], [0.0, 5.98, 0.0]]);
    let before = pos.clone();
    Solver::new().step(&mut pos, &mut vel, &ext, &world(&[], 2.0), 10);
    for i in 0..2 {
        assert_eq!(pos[i * 4 + 3], 0.0);
        assert!(pos[i * 4 + 1] < before[i * 4 + 1]);
    }
}

#[test]
fn a_static_body_between_two_islands_does_not_join_them() {
    let (mut pos, mut vel, ext) =
        sleeping_boxes(&[[0.0, 0.47, 0.0], [0.98, 0.47, 0.0], [1.96, 0.47, 0.0]]);
    let mut w = world_with_materials(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 2.0, 3);
    body_material(&mut w, 1)[5] = material::BODY_STATIC;
    vel[4] = 20.0; // ignored on a static, including by wake-up
    pos[3] = 0.0;
    vel[0] = 0.1;
    Solver::new().step(&mut pos, &mut vel, &ext, &w, 1);
    assert!(pos[3] < SLEEP_STEPS);
    assert!(
        pos[11] >= SLEEP_STEPS,
        "a static bridged the two dynamic islands"
    );
}

#[test]
fn a_slow_kinematic_support_wakes_every_body_resting_on_it() {
    let (mut pos, mut vel, mut ext) =
        sleeping_boxes(&[[0.0, 0.0, 0.0], [0.0, 0.98, 0.0], [0.0, 1.96, 0.0]]);
    let mut w = world_with_materials(&[], 2.0, 3);
    body_material(&mut w, 0)[5] = material::BODY_KINEMATIC;
    ext[3] = 0.0;
    vel[0] = 0.1;
    Solver::new().step(&mut pos, &mut vel, &ext, &w, 1);
    assert!(pos[0] > 0.0);
    assert!(pos[7] < SLEEP_STEPS && pos[11] < SLEEP_STEPS);
}

#[test]
fn collision_masks_cut_wake_propagation_inside_a_chain() {
    let (mut pos, mut vel, ext) =
        sleeping_boxes(&[[0.0, 0.47, 0.0], [0.98, 0.47, 0.0], [1.96, 0.47, 0.0]]);
    let mut w = world_with_materials(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 2.0, 3);
    w[material::WORLD_PARAM_ANY_MASK] = 1.0;
    body_material(&mut w, 0)[6] = f32::from_bits(2);
    body_material(&mut w, 0)[7] = f32::from_bits(1 | 2);
    body_material(&mut w, 1)[6] = f32::from_bits(2);
    body_material(&mut w, 1)[7] = f32::from_bits(1 | 2);
    body_material(&mut w, 2)[6] = f32::from_bits(4);
    body_material(&mut w, 2)[7] = f32::from_bits(1 | 4);
    pos[3] = 0.0;
    vel[0] = 0.1;
    Solver::new().step(&mut pos, &mut vel, &ext, &w, 1);
    assert!(pos[3] < SLEEP_STEPS && pos[7] < SLEEP_STEPS);
    assert!(
        pos[11] >= SLEEP_STEPS,
        "masked edge still transmitted wake-up"
    );
}

#[test]
fn exact_face_contact_belongs_to_the_same_island() {
    let (mut pos, mut vel, ext) = sleeping_boxes(&[[0.0, 0.47, 0.0], [1.0, 0.47, 0.0]]);
    pos[3] = 0.0;
    Solver::new().step(&mut pos, &mut vel, &ext, &floor(), 1);
    assert!(
        pos[7] < SLEEP_STEPS,
        "touching faces were treated as disjoint"
    );
}

#[test]
fn an_overflowing_grid_uses_complete_pairs_for_solver_and_islands() {
    // 33 inert bodies fill one bucket; the dynamic bridge is past its capacity.
    let points: Vec<_> = (0..35)
        .map(|i| [if i < 33 { 0.0 } else { (i - 32) as f32 * 0.98 }, 0.47, 0.0])
        .collect();
    let (mut pos, mut vel, mut ext) = sleeping_boxes(&points);
    let mut w = world_with_materials(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 100.0, 35);
    for i in 0..32 {
        body_material(&mut w, i)[5] = material::BODY_STATIC;
        ext[i * 4 + 3] = 0.0;
    }
    pos[32 * 4 + 3] = 0.0;
    vel[32 * 4] = 0.1;
    let (mut reference_pos, mut reference_vel) = (pos.clone(), vel.clone());
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &w, 1);
    Solver::new().with_broad_phase(BroadPhase::Everything).step(
        &mut reference_pos,
        &mut reference_vel,
        &ext,
        &w,
        1,
    );
    assert!(solver.grid_overflows() > 0);
    assert!(pos[34 * 4 + 3] < SLEEP_STEPS, "overflow dropped the bridge");
    assert_eq!(pos, reference_pos);
    assert_eq!(vel, reference_vel);
}

#[test]
fn reusing_a_solver_for_another_world_does_not_keep_old_islands() {
    let (mut pos, mut vel, ext) = sleeping_boxes(&[[0.0, 0.47, 0.0], [0.98, 0.47, 0.0]]);
    pos[3] = 0.0;
    let mut shared = Solver::new();
    shared.step(&mut pos, &mut vel, &ext, &floor(), 1);
    let (mut other_pos, mut other_vel, other_ext) =
        sleeping_boxes(&[[0.0, 0.47, 0.0], [5.0, 0.47, 0.0]]);
    let (mut fresh_pos, mut fresh_vel) = (other_pos.clone(), other_vel.clone());
    shared.step(&mut other_pos, &mut other_vel, &other_ext, &floor(), 1);
    Solver::new().step(&mut fresh_pos, &mut fresh_vel, &other_ext, &floor(), 1);
    assert_eq!(other_pos, fresh_pos);
    assert_eq!(other_vel, fresh_vel);
}

#[test]
fn sphere_contacts_propagate_a_slow_wake_through_the_chain() {
    let (mut pos, mut vel, ext) =
        sleeping_boxes(&[[0.0, 0.47, 0.0], [0.98, 0.47, 0.0], [1.96, 0.47, 0.0]]);
    for record in vel.chunks_exact_mut(4) {
        record[3] = SPHERE;
    }
    vel[0] = 0.1;
    Solver::new().step(&mut pos, &mut vel, &ext, &floor(), 1);
    assert!(pos.chunks_exact(4).all(|p| p[3] < SLEEP_STEPS));
}

#[test]
fn rotated_boxes_with_only_aabb_overlap_remain_separate_islands() {
    let (mut pos, mut vel, mut ext) =
        sleeping_boxes(&[[0.0, 0.18, 0.0], [0.3535534, 0.18, 0.3535534]]);
    for record in ext.chunks_exact_mut(4) {
        record[..3].copy_from_slice(&[2.0, 0.2, 0.2]);
    }
    let mut w = world_with_materials(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 5.0, 2);
    let orientations_at = w.len();
    w.resize(orientations_at + (256 + 2) * 4, 0.0);
    for q in w[orientations_at..].chunks_exact_mut(4) {
        q[3] = 1.0;
    }
    let half_angle = std::f32::consts::FRAC_PI_4 * 0.5;
    for q in w[orientations_at + 256 * 4..].chunks_exact_mut(4) {
        q[1] = half_angle.sin();
        q[3] = half_angle.cos();
    }
    pos[3] = 0.0;
    Solver::new().step(&mut pos, &mut vel, &ext, &w, 1);
    assert!(pos[3] < SLEEP_STEPS);
    assert!(
        pos[7] >= SLEEP_STEPS,
        "AABB overlap falsely joined oriented islands"
    );
}

#[test]
fn changing_support_filters_invalidates_the_fully_sleeping_world() {
    let (mut pos, mut vel, ext) = sleeping_boxes(&[[0.0, 0.47, 0.0]]);
    let mut w = world_with_materials(&[([0.0, -0.5, 0.0], [40.0, 0.5, 40.0])], 2.0, 1);
    let mut solver = Solver::new();
    solver.step(&mut pos, &mut vel, &ext, &w, 20);
    let before = pos.clone();
    solver.step(&mut pos, &mut vel, &ext, &w, 20);
    assert_eq!(pos, before, "an unchanged sleeping world drifted");
    w[material::WORLD_PARAM_ANY_MASK] = 1.0;
    w[material::MATERIALS_AT + 3] = f32::from_bits(0);
    solver.step(&mut pos, &mut vel, &ext, &w, 1);
    assert!(pos[3] < SLEEP_STEPS && pos[1] < before[1]);
}
