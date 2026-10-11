//! As fases de contato de um sub-passo: estáticos e pares.
//!
//! Saíram de `mod.rs` quando ele passou de 500 linhas (regra 6 do README), e o
//! corte é este e não outro porque estas três são o que um sub-passo FAZ — o que
//! fica lá é o que ele É: o estado, as constantes e a ordem em que elas rodam.
//!
//! Um módulo filho enxerga os campos privados do pai, então `Scene` continua
//! fechada para fora do `solver` e aberta aqui.

use super::*;

impl Scene<'_> {
    /// The immovable geometry of the scene, which does not give: the whole
    /// correction is the moving body's, and only the velocity component entering
    /// the static is removed.
    /// Answers whether any static touched the body.
    /// `FILTRO` é `any_mask`, escolhido uma vez por sub-passo.
    pub(super) fn against_statics<const FILTRO: bool>(
        &self,
        p: &mut V3,
        v: &mut V3,
        h: V3,
        shape: f32,
        orientation: [f32; 4],
        inverse_mass: f32,
        mine: Body,
    ) -> bool {
        let mut touched = false;
        for k in 0..self.statics {
            if FILTRO {
                let (other_layer, other_mask) = self.materials.fixed_layer_mask(k);
                if (mine.mask & other_layer) == 0 || (other_mask & mine.layer) == 0 {
                    continue;
                }
            }
            let centre = triple(self.world, 2 + k * 2);
            let half = triple(self.world, 3 + k * 2);
            // Roundness, not shape: see the layout note in the module doc.
            let fixed_shape = match self.world[(2 + k * 2) * 4 + 3] > 0.5 {
                true => 0.0,
                false => 1.0,
            };
            let Some((normal, depth)) = narrow(
                *p,
                h,
                shape,
                orientation,
                centre,
                half,
                fixed_shape,
                self.materials.fixed_orientation(k),
            ) else {
                continue;
            };
            let made_of = self.materials.fixed(k);
            if !self.softness_enabled {
                *p = add(
                    *p,
                    scale(normal, (depth - SLOP).max(0.0) * STATIC_RELAXATION),
                );
            }
            let approach = dot(*v, normal);
            if self.softness_enabled {
                let mut target = depth * self.softness_bias_rate;
                if approach < -1.0 {
                    target = target
                        .max(-approach * bounce(approach, mine.restitution, made_of.restitution));
                }
                let lambda =
                    (target - approach).max(0.0) / (inverse_mass + self.softness_gamma).max(0.0001);
                *v = add(*v, scale(normal, lambda * inverse_mass));
            } else if approach < 0.0 {
                let back = 1.0 + bounce(approach, mine.restitution, made_of.restitution);
                *v = sub(*v, scale(normal, approach * back));
            }
            if normal[1] > VERTICAL {
                let kept =
                    1.0 - friction_loss(GROUND_FRICTION_LOSS, mine.friction, made_of.friction);
                v[0] *= kept;
                v[2] *= kept;
            }
            touched = true;
        }
        touched
    }

    /// The dynamic pairs: this body applies only its own half of each.
    ///
    /// The cell is recomputed from the position AFTER integration and statics,
    /// not from the one the grid was built with — a pair the step itself created
    /// would otherwise be missed until the next one.
    ///
    /// `FILTRO` é `any_mask`. Aqui é onde mais pesa, porque o teste roda
    /// por CANDIDATO e a cena densa tem dezenas por corpo.
    pub(super) fn against_bodies<const FILTRO: bool>(
        &self,
        body: usize,
        p: &mut V3,
        v: &mut V3,
        sleep: &mut f32,
        h: V3,
        shape: f32,
        inverse_mass: f32,
        mine: Body,
    ) -> bool {
        let mut position = *p;
        let mut velocity = *v;
        let pair_velocity_base = velocity;
        let mut split_velocity_delta = [0.0; 3];
        let mut counter = *sleep;
        let mut touched = false;
        self.near(*p, |other| {
            if other == body {
                return;
            }
            if FILTRO {
                let (other_layer, other_mask) = self.materials.layer_mask(other);
                if (mine.mask & other_layer) == 0 || (other_mask & mine.layer) == 0 {
                    return;
                }
            }
            let Some((normal, depth)) = narrow(
                position,
                h,
                shape,
                self.materials.body_orientation(body),
                self.position(other),
                self.extent(other),
                self.shape(other),
                self.materials.body_orientation(other),
            ) else {
                return;
            };
            let (other_restitution, other_friction) = self.materials.restitution_friction(other);
            let other_inverse_mass = self.extents[other * 4 + 3];
            let share = inverse_mass / (inverse_mass + other_inverse_mass).max(0.0001);
            let theirs = self.velocity(other);
            let split_pair = self.mass_splitting
                && shape > 0.5
                && self.shape(other) > 0.5
                && other_inverse_mass > 0.0;
            let response_share = match split_pair {
                true => {
                    inverse_mass
                        / (inverse_mass * self.degrees[body]
                            + other_inverse_mass * self.degrees[other])
                            .max(0.0001)
                }
                false => share,
            };
            // Relative velocity along the normal. Negative is approaching.
            let velocity_for_pair = match split_pair {
                true => pair_velocity_base,
                false => velocity,
            };
            let approach = dot(sub(velocity_for_pair, theirs), normal);
            let back = 1.0 + bounce(approach, mine.restitution, other_restitution);
            if self.softness_enabled {
                let effective_mass = match split_pair {
                    true => {
                        inverse_mass * self.degrees[body] + other_inverse_mass * self.degrees[other]
                    }
                    false => inverse_mass + other_inverse_mass,
                };
                let mut target = depth * self.softness_bias_rate;
                if approach < -1.0 {
                    target = target
                        .max(-approach * bounce(approach, mine.restitution, other_restitution));
                }
                let impulse = (target - approach).max(0.0)
                    / (effective_mass + self.softness_gamma).max(0.0001);
                let delta = scale(normal, impulse * inverse_mass);
                if split_pair {
                    split_velocity_delta = add(split_velocity_delta, delta);
                } else {
                    velocity = add(velocity, delta);
                }
                if approach < -1.0 {
                    counter = 0.0;
                }
                if !split_pair
                    && depth * self.softness_bias_rate < 0.01
                    && normal[1] > VERTICAL
                    && approach < 0.5
                {
                    velocity[1] = theirs[1];
                    let grip = friction_loss(STACK_FRICTION, mine.friction, other_friction);
                    velocity[0] += (theirs[0] - velocity[0]) * grip;
                    velocity[2] += (theirs[2] - velocity[2]) * grip;
                }
            } else {
                if normal[1] > VERTICAL || normal[1] < -VERTICAL {
                    if approach < -1.0 {
                        // A real impact: a normal impulse. Stone does not bounce,
                        // and with the default material neither does anything else.
                        let delta = scale(normal, approach * response_share * back);
                        if split_pair {
                            split_velocity_delta = sub(split_velocity_delta, delta);
                        } else {
                            velocity = sub(velocity, delta);
                        }
                        counter = 0.0;
                    } else if approach < 0.5 && normal[1] > VERTICAL {
                        // SUPPORT INHERITANCE: resting on something and descending
                        // slowly, so the support's vertical velocity is taken rather
                        // than an impulse applied — an impulse here is the limit
                        // cycle that makes a column vibrate forever.
                        velocity[1] = theirs[1];
                        let grip = friction_loss(STACK_FRICTION, mine.friction, other_friction);
                        velocity[0] += (theirs[0] - velocity[0]) * grip;
                        velocity[2] += (theirs[2] - velocity[2]) * grip;
                    }
                } else if approach < 0.0 {
                    let delta = scale(normal, approach * response_share * back);
                    if split_pair {
                        split_velocity_delta = sub(split_velocity_delta, delta);
                    } else {
                        velocity = sub(velocity, delta);
                    }
                    if approach < -1.0 {
                        counter = 0.0;
                    }
                }
            }
            if !self.softness_enabled {
                position = add(
                    position,
                    scale(normal, (depth - SLOP).max(0.0) * PAIR_RELAXATION * share),
                );
            }
            touched = true;
        });
        velocity = add(velocity, split_velocity_delta);
        *p = position;
        *v = velocity;
        *sleep = counter;
        touched
    }
}
