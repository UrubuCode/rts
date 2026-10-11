//! Deterministic contact components and their shared sleep state.
//!
//! Static/kinematic bodies can support several islands but never join them.
//! Scratch is rebuilt from the caller's buffers, so alternating worlds through
//! the same Solver cannot retain another world's membership or sleep state.

use super::*;

const UNASSIGNED: usize = usize::MAX;
/// Touching faces belong to the same island even without penetration.
pub(super) const CONTACT_MARGIN: f32 = 0.0001;
const MOVING_SPEED_SQUARED: f32 = 1.0e-12;

#[derive(Default)]
pub(super) struct Islands {
    ids: Vec<usize>,
    members: Vec<usize>,
    stack: Vec<usize>,
    groups: Vec<Group>,
    pub counters: Vec<f32>,
}

struct Group {
    start: usize,
    end: usize,
    can_sleep: bool,
}

impl Islands {
    pub fn prepare(&mut self, scene: &Scene<'_>) {
        self.ids.clear();
        self.ids.resize(scene.count, UNASSIGNED);
        self.counters.clear();
        self.counters
            .extend((0..scene.count).map(|i| scene.positions[i * 4 + 3]));
        self.members.clear();
        self.groups.clear();
        self.stack.clear();

        for root in 0..scene.count {
            if self.ids[root] != UNASSIGNED || !dynamic(scene, root) {
                continue;
            }
            let id = self.groups.len();
            let start = self.members.len();
            self.ids[root] = id;
            self.stack.push(root);
            let mut quiet = SLEEP_STEPS;
            let mut moving_anchor = false;
            let mut changed_velocity = false;
            let mut anchored = false;
            let mut needs_support = false;
            while let Some(body) = self.stack.pop() {
                self.members.push(body);
                let mine = scene.materials.body(body);
                let counter = self.counters[body];
                quiet = quiet.min(if counter.is_finite() {
                    counter.max(0.0)
                } else {
                    0.0
                });
                // A caller may apply an impulse without clearing pos.w.
                changed_velocity |= counter >= SLEEP_STEPS
                    && dot(scene.velocity(body), scene.velocity(body)) > MOVING_SPEED_SQUARED;
                needs_support |= mine.gravity.abs() > 0.000001;
                let p = scene.position(body);
                anchored |= p[1] <= FLOOR + CONTACT_MARGIN
                    || (mine.floor > -1.0e8 && p[1] <= mine.floor + CONTACT_MARGIN);
                if !anchored {
                    for fixed in 0..scene.statics {
                        if scene.materials.any_mask() {
                            let (layer, mask) = scene.materials.fixed_layer_mask(fixed);
                            if mine.mask & layer == 0 || mask & mine.layer == 0 {
                                continue;
                            }
                        }
                        let shape = if scene.world[(2 + fixed * 2) * 4 + 3] > 0.5 {
                            0.0
                        } else {
                            1.0
                        };
                        if let Some((normal, _)) = narrow(
                            p,
                            padded(scene.extent(body)),
                            scene.shape(body),
                            scene.materials.body_orientation(body),
                            triple(scene.world, 2 + fixed * 2),
                            triple(scene.world, 3 + fixed * 2),
                            shape,
                            scene.materials.fixed_orientation(fixed),
                        ) {
                            anchored |= supports(normal, mine.gravity);
                        }
                    }
                }
                scene.near(p, |other| {
                    // Already-discovered dynamics need no second narrow phase.
                    // Static/kinematic anchors always keep UNASSIGNED.
                    if other == body || self.ids[other] == id {
                        return;
                    }
                    let (layer, mask) = scene.materials.layer_mask(other);
                    if scene.materials.any_mask()
                        && (mine.mask & layer == 0 || mask & mine.layer == 0)
                    {
                        return;
                    }
                    let Some((normal, _)) = narrow(
                        p,
                        padded(scene.extent(body)),
                        scene.shape(body),
                        scene.materials.body_orientation(body),
                        scene.position(other),
                        scene.extent(other),
                        scene.shape(other),
                        scene.materials.body_orientation(other),
                    ) else {
                        return;
                    };
                    if dynamic(scene, other) {
                        if self.ids[other] == UNASSIGNED {
                            self.ids[other] = id;
                            self.stack.push(other);
                        }
                    } else {
                        anchored |= supports(normal, mine.gravity);
                        // A moving kinematic support wakes even below the old
                        // impact threshold. Static velocity is never a command.
                        moving_anchor |= scene.materials.body(other).body_type
                            != material::BODY_STATIC
                            && dot(scene.velocity(other), scene.velocity(other))
                                > MOVING_SPEED_SQUARED;
                    }
                });
            }
            let end = self.members.len();
            self.members[start..end].sort_unstable();
            let can_sleep = !moving_anchor && (!needs_support || anchored);
            if !can_sleep || changed_velocity {
                quiet = 0.0;
            }
            for &body in &self.members[start..end] {
                self.counters[body] = quiet;
            }
            self.groups.push(Group {
                start,
                end,
                can_sleep,
            });
        }
    }

    /// Decide sleep only after every worker has produced its candidate counter.
    /// No body in a component can stop while another still needs simulation.
    pub fn finish(&self, pos: &mut [f32], vel: &mut [f32]) {
        for group in &self.groups {
            let members = &self.members[group.start..group.end];
            let mut quiet = if group.can_sleep { SLEEP_STEPS } else { 0.0 };
            for &body in members {
                quiet = quiet.min(pos[body * 4 + 3]);
            }
            for &body in members {
                pos[body * 4 + 3] = quiet;
                if quiet >= SLEEP_STEPS {
                    vel[body * 4..body * 4 + 3].fill(0.0);
                }
            }
        }
    }
}

fn dynamic(scene: &Scene<'_>, body: usize) -> bool {
    scene.materials.body(body).body_type == material::BODY_DYNAMIC
        && scene.extents[body * 4 + 3] > 0.0
}

fn padded(h: V3) -> V3 {
    add(h, [CONTACT_MARGIN; 3])
}

fn supports(normal: V3, gravity: f32) -> bool {
    if gravity < 0.0 {
        normal[1] < -VERTICAL
    } else {
        normal[1] > VERTICAL
    }
}

/// Exact input cache for a world whose entire mutable state is at rest.
/// Every input, including filters, shapes and static poses, participates. It is
/// safe across interleaved worlds and needs no JS buffer ownership or identity.
#[derive(Default)]
pub(super) struct SleepCache {
    valid: bool,
    pos: Vec<f32>,
    vel: Vec<f32>,
    ext: Vec<f32>,
    world: Vec<f32>,
}

impl SleepCache {
    pub fn matches(&self, pos: &[f32], vel: &[f32], ext: &[f32], world: &[f32]) -> bool {
        self.valid
            && same_bits(pos, &self.pos)
            && same_bits(vel, &self.vel)
            && same_bits(ext, &self.ext)
            && same_bits(world, &self.world)
    }

    pub fn capture_if_asleep(
        &mut self,
        pos: &[f32],
        vel: &[f32],
        ext: &[f32],
        world: &[f32],
        materials: Materials<'_>,
        count: usize,
    ) -> bool {
        self.valid = false;
        for body in 0..count {
            let kind = materials.body(body).body_type;
            if kind == material::BODY_DYNAMIC && pos[body * 4 + 3] < SLEEP_STEPS {
                return false;
            }
            if vel[body * 4..body * 4 + 3].iter().any(|&v| v != 0.0) {
                return false;
            }
        }
        for (saved, input) in [
            (&mut self.pos, pos),
            (&mut self.vel, vel),
            (&mut self.ext, ext),
            (&mut self.world, world),
        ] {
            saved.clear();
            saved.extend_from_slice(input);
        }
        self.valid = true;
        true
    }
}

fn same_bits(a: &[f32], b: &[f32]) -> bool {
    // Layer/mask bit patterns may be NaNs: floating equality would miss forever.
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}
