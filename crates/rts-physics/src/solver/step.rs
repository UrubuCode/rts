//! As FASES de um sub-passo, uma por função: acordar, estáticos, pares.
//!
//! Saíram de `mod.rs` quando ele passou de 500 linhas (regra 6 do README), e o
//! corte é este e não outro porque estas três são o que um sub-passo FAZ — o que
//! fica lá é o que ele É: o estado, as constantes e a ordem em que elas rodam.
//!
//! Um módulo filho enxerga os campos privados do pai, então `Scene` continua
//! fechada para fora do `solver` e aberta aqui.

use super::*;

impl Scene<'_> {
    /// Whether a sleeping body has a FAST neighbour touching it.
    ///
    /// Through the grid, like everything else. Scanning every body instead would
    /// make a scene at rest — the case sleeping exists to make cheap — the one
    /// that pays a full n² every step.
    pub(super) fn disturbed(&self, body: usize, p: V3, h: V3, shape: f32) -> bool {
        let (my_layer, my_mask) = self.materials.layer_mask(body);
        let mut woken = false;
        self.near(p, |other| {
            if woken || other == body {
                return;
            }
            let (other_layer, other_mask) = self.materials.layer_mask(other);
            if (my_mask & other_layer) == 0 || (other_mask & my_layer) == 0 {
                return;
            }
            let vj = self.velocity(other);
            if dot(vj, vj) <= WAKE_SPEED_SQUARED {
                return;
            }
            let hit = narrow(
                p,
                h,
                shape,
                self.position(other),
                self.extent(other),
                self.shape(other),
            );
            woken = hit.is_some();
        });
        woken
    }

    /// The immovable geometry of the scene, which does not give: the whole
    /// correction is the moving body's, and only the velocity component entering
    /// the static is removed.
    /// Answers whether any static touched the body.
    pub(super) fn against_statics(&self, p: &mut V3, v: &mut V3, h: V3, shape: f32, mine: Body) -> bool {
        let mut touched = false;
        for k in 0..self.statics {
            let (other_layer, other_mask) = self.materials.fixed_layer_mask(k);
            if (mine.mask & other_layer) == 0 || (other_mask & mine.layer) == 0 {
                continue;
            }
            let centre = triple(self.world, 2 + k * 2);
            let half = triple(self.world, 3 + k * 2);
            // Roundness, not shape: see the layout note in the module doc.
            let fixed_shape = match self.world[(2 + k * 2) * 4 + 3] > 0.5 {
                true => 0.0,
                false => 1.0,
            };
            let Some((normal, depth)) = narrow(*p, h, shape, centre, half, fixed_shape) else {
                continue;
            };
            let made_of = self.materials.fixed(k);
            *p = add(*p, scale(normal, (depth - SLOP).max(0.0) * STATIC_RELAXATION));
            let approach = dot(*v, normal);
            if approach < 0.0 {
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
    pub(super) fn against_bodies(
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
        let mut counter = *sleep;
        let mut touched = false;
        self.near(*p, |other| {
            if other == body {
                return;
            }
            let (other_layer, other_mask) = self.materials.layer_mask(other);
            if (mine.mask & other_layer) == 0 || (other_mask & mine.layer) == 0 {
                return;
            }
            let Some((normal, depth)) = narrow(
                position,
                h,
                shape,
                self.position(other),
                self.extent(other),
                self.shape(other),
            ) else {
                return;
            };
            let made_of = self.materials.body(other);
            let other_inverse_mass = self.extents[other * 4 + 3];
            let share = inverse_mass / (inverse_mass + other_inverse_mass).max(0.0001);
            let theirs = self.velocity(other);
            // Relative velocity along the normal. Negative is approaching.
            let approach = dot(sub(velocity, theirs), normal);
            let back = 1.0 + bounce(approach, mine.restitution, made_of.restitution);

            if normal[1] > VERTICAL || normal[1] < -VERTICAL {
                if approach < -1.0 {
                    // A real impact: a normal impulse. Stone does not bounce,
                    // and with the default material neither does anything else.
                    velocity = sub(velocity, scale(normal, approach * share * back));
                    counter = 0.0;
                } else if approach < 0.5 && normal[1] > VERTICAL {
                    // SUPPORT INHERITANCE: resting on something and descending
                    // slowly, so the support's vertical velocity is taken rather
                    // than an impulse applied — an impulse here is the limit
                    // cycle that makes a column vibrate forever.
                    velocity[1] = theirs[1];
                    let grip = friction_loss(STACK_FRICTION, mine.friction, made_of.friction);
                    velocity[0] += (theirs[0] - velocity[0]) * grip;
                    velocity[2] += (theirs[2] - velocity[2]) * grip;
                }
            } else if approach < 0.0 {
                velocity = sub(velocity, scale(normal, approach * share * back));
                if approach < -1.0 {
                    counter = 0.0;
                }
            }
            position = add(
                position,
                scale(normal, (depth - SLOP).max(0.0) * PAIR_RELAXATION * share),
            );
            touched = true;
        });
        *p = position;
        *v = velocity;
        *sleep = counter;
        touched
    }
}
