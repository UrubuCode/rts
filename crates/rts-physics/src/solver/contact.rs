//! The narrow phase: one contact function, three shape cases.
//!
//! # This is a translation, and the original is named
//!
//! `contato()` in `engine/rigid/gpurigid.ts` (the WGSL kernel of the `rts-game`
//! project), which is itself the translation of that project's `solvePair`. The
//! order of the cases, the constants, and the convention that the normal points
//! from B **towards A** are all taken verbatim, because the two backends are
//! compared against each other by final position: a divergence here does not
//! read as "the CPU is slightly different", it reads as the two solvers ending
//! the simulation in different places.
//!
//! # Why the radius of a sphere is `min(half_extent)`
//!
//! The same rule `raio()` in the kernel and `radiusOf` on the game's CPU side
//! already state: half of the SMALLEST extent, so the sphere fits inside the box
//! its extents describe and there is never a phantom push. Derived here from
//! `ext` rather than stored, so no second place can hold a different radius.

/// A point or a direction. Three floats, because that is what the buffers hold.
pub type V3 = [f32; 3];

#[inline]
pub(crate) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub(crate) fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

#[inline]
pub(crate) fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub(crate) fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}

/// The sphere radius an extent describes: half of the smallest side.
#[inline]
pub fn radius(h: V3) -> f32 {
    h[0].min(h[1]).min(h[2])
}

/// WGSL's `select(-1.0, 1.0, x >= 0.0)`, which is not `f32::signum`: signum
/// answers `-1` for `-0.0` and `NaN` for `NaN`, where the kernel answers `+1`
/// for `-0.0`. A body exactly on an axis is common enough — a cube dropped
/// square onto a floor — that the difference is reachable rather than academic.
#[inline]
fn axis_sign(x: f32) -> f32 {
    match x >= 0.0 {
        true => 1.0,
        false => -1.0,
    }
}

/// A contact between two shapes: the normal pointing from B towards A, and the
/// penetration depth.
///
/// `None` is "not touching". The caller is always A, because the gather model
/// has each body apply only its own half of the correction.
///
/// `shape` is `0.0` for a sphere and anything above `0.5` for a box — the same
/// encoding the kernel reads out of `vel.w`, so a buffer written for one backend
/// is readable by the other with no conversion step to get wrong.
pub fn contact(pa: V3, ha: V3, sa: f32, pb: V3, hb: V3, sb: f32) -> Option<(V3, f32)> {
    contact_oriented(
        pa,
        ha,
        sa,
        [0.0, 0.0, 0.0, 1.0],
        pb,
        hb,
        sb,
        [0.0, 0.0, 0.0, 1.0],
    )
}

/// Quaternion-aware narrow phase. Quaternions are `(x, y, z, w)` and use the
/// same local-to-world convention as `Transform` and the WGSL backend.
pub fn contact_oriented(
    pa: V3,
    ha: V3,
    sa: f32,
    qa: [f32; 4],
    pb: V3,
    hb: V3,
    sb: f32,
    qb: [f32; 4],
) -> Option<(V3, f32)> {
    let d = sub(pa, pb);

    if sa > 0.5 && sb > 0.5 {
        return obb_box_box(d, ha, axes(qa), hb, axes(qb));
    }
    if sa < 0.5 && sb < 0.5 {
        return sphere_sphere(d, ha, hb);
    }
    sphere_obb(pa, ha, axes(qa), pb, hb, axes(qb), sb)
}

fn axes(q: [f32; 4]) -> [V3; 3] {
    let norm = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let (x, y, z, w) = if norm > 1.0e-8 {
        (q[0] / norm, q[1] / norm, q[2] / norm, q[3] / norm)
    } else {
        (0.0, 0.0, 0.0, 1.0)
    };
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
        ],
        [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
        ],
        [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn obb_box_box(d: V3, ha: V3, aa: [V3; 3], hb: V3, ab: [V3; 3]) -> Option<(V3, f32)> {
    let mut best = f32::INFINITY;
    let mut best_axis = [0.0, 1.0, 0.0];
    for index in 0..15 {
        let raw = if index < 3 {
            aa[index]
        } else if index < 6 {
            ab[index - 3]
        } else {
            cross(aa[(index - 6) / 3], ab[(index - 6) % 3])
        };
        let len2 = dot(raw, raw);
        if index >= 6 && len2 < 1.0e-8 {
            continue;
        }
        let axis = if index < 6 {
            raw
        } else {
            scale(raw, 1.0 / len2.sqrt())
        };
        let ra = ha[0] * dot(axis, aa[0]).abs()
            + ha[1] * dot(axis, aa[1]).abs()
            + ha[2] * dot(axis, aa[2]).abs();
        let rb = hb[0] * dot(axis, ab[0]).abs()
            + hb[1] * dot(axis, ab[1]).abs()
            + hb[2] * dot(axis, ab[2]).abs();
        let signed = dot(d, axis);
        let overlap = ra + rb - signed.abs();
        if overlap <= 0.0 {
            return None;
        }
        if overlap < best {
            best = overlap;
            best_axis = scale(axis, axis_sign(signed));
        }
    }
    Some((best_axis, best))
}

fn sphere_obb(
    pa: V3,
    ha: V3,
    aa: [V3; 3],
    pb: V3,
    hb: V3,
    ab: [V3; 3],
    sb: f32,
) -> Option<(V3, f32)> {
    let (bc, bh, ba, sc, r, sign) = if sb > 0.5 {
        (pb, hb, ab, pa, radius(ha), 1.0)
    } else {
        (pa, ha, aa, pb, radius(hb), -1.0)
    };
    let rel = sub(sc, bc);
    let local = [dot(rel, ba[0]), dot(rel, ba[1]), dot(rel, ba[2])];
    let q = [
        local[0].clamp(-bh[0], bh[0]),
        local[1].clamp(-bh[1], bh[1]),
        local[2].clamp(-bh[2], bh[2]),
    ];
    let delta = [local[0] - q[0], local[1] - q[1], local[2] - q[2]];
    let d2 = dot(delta, delta);
    if d2 >= r * r {
        return None;
    }
    let (normal, depth) = if d2 > 1.0e-6 {
        let dist = d2.sqrt();
        (
            scale(
                add(
                    add(scale(ba[0], delta[0]), scale(ba[1], delta[1])),
                    scale(ba[2], delta[2]),
                ),
                sign / dist,
            ),
            r - dist,
        )
    } else {
        let gaps = [bh[0] - q[0].abs(), bh[1] - q[1].abs(), bh[2] - q[2].abs()];
        let axis = if gaps[1] <= gaps[0] && gaps[1] <= gaps[2] {
            1
        } else if gaps[0] <= gaps[2] {
            0
        } else {
            2
        };
        let mut n = scale(ba[axis], axis_sign(q[axis]) * sign);
        if !n.iter().all(|v| v.is_finite()) {
            n = [0.0, 1.0, 0.0];
        }
        (n, gaps[axis] + r)
    };
    Some((normal, depth))
}

/// Sphere against sphere: centre distance against the sum of the radii.
fn sphere_sphere(d: V3, ha: V3, hb: V3) -> Option<(V3, f32)> {
    let rs = radius(ha) + radius(hb);
    let d2 = dot(d, d);
    // The lower guard is the kernel's: two centres in the same place have no
    // direction to separate along, and normalising would answer NaN — which
    // spreads through the gather to every neighbour before anything notices.
    if d2 >= rs * rs || d2 <= 0.0001 {
        return None;
    }
    let dist = d2.sqrt();
    Some((scale(d, 1.0 / dist), rs - dist))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: f32 = 1.0;
    const SPHERE: f32 = 0.0;

    #[test]
    fn a_cube_resting_on_a_wide_floor_is_pushed_up_and_not_sideways() {
        // The property the least-penetration rule exists for: the floor overlaps
        // on all three axes and only the vertical one is shallow.
        let (normal, depth) = contact(
            [0.0, 0.9, 0.0],
            [0.5, 0.5, 0.5],
            BOX,
            [0.0, 0.0, 0.0],
            [10.0, 0.5, 10.0],
            BOX,
        )
        .expect("overlapping boxes touch");
        assert_eq!(normal, [0.0, 1.0, 0.0]);
        assert!((depth - 0.1).abs() < 1e-5, "depth was {depth}");
    }

    #[test]
    fn a_sphere_uses_the_smallest_half_extent_so_it_never_pushes_from_outside_its_box() {
        // A flattened extent: the radius is 0.1, so centres 0.5 apart do not
        // touch even though the largest extent would say they do.
        let flat = [2.0, 0.1, 2.0];
        assert!(contact([0.0; 3], flat, SPHERE, [0.5, 0.0, 0.0], flat, SPHERE).is_none());
        let (normal, _) =
            contact([0.0; 3], flat, SPHERE, [0.15, 0.0, 0.0], flat, SPHERE).expect("touching");
        assert_eq!(normal, [-1.0, 0.0, 0.0]);
    }

    #[test]
    fn the_normal_points_from_b_to_a_whichever_of_the_two_is_the_sphere() {
        // The convention the gather model depends on: each body is A when it
        // computes its own half, so the same pair must answer opposite normals
        // depending on which side asks. A mixed pair is where a translation
        // typically drops the sign.
        let sphere = ([0.0, 1.4, 0.0], [0.5, 0.5, 0.5]);
        let cube = ([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (from_sphere, da) =
            contact(sphere.0, sphere.1, SPHERE, cube.0, cube.1, BOX).expect("touching");
        let (from_box, db) =
            contact(cube.0, cube.1, BOX, sphere.0, sphere.1, SPHERE).expect("touching");
        assert_eq!(from_sphere, [0.0, 1.0, 0.0]);
        assert_eq!(from_box, [0.0, -1.0, 0.0]);
        assert!((da - db).abs() < 1e-6);
    }

    #[test]
    fn two_spheres_at_the_same_point_answer_no_contact_rather_than_a_nan_normal() {
        // NaN never sleeps and spreads to every neighbour in the next gather,
        // which is the failure the kernel's guard was added for.
        assert!(contact([0.0; 3], [1.0; 3], SPHERE, [0.0; 3], [1.0; 3], SPHERE).is_none());
    }

    #[test]
    fn oriented_boxes_reject_an_aabb_only_overlap() {
        let s = (std::f32::consts::FRAC_PI_4 * 0.5).sin();
        let c = (std::f32::consts::FRAC_PI_4 * 0.5).cos();
        let q = [0.0, s, 0.0, c];
        let centre = [
            std::f32::consts::FRAC_1_SQRT_2 * 0.5,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2 * 0.5,
        ];
        assert!(contact_oriented(
            [0.0; 3],
            [2.0, 0.2, 0.2],
            BOX,
            q,
            centre,
            [2.0, 0.2, 0.2],
            BOX,
            q
        )
        .is_none());
    }

    #[test]
    fn sphere_contact_uses_the_box_orientation_and_world_normal() {
        let s = (std::f32::consts::FRAC_PI_4 * 0.5).sin();
        let c = (std::f32::consts::FRAC_PI_4 * 0.5).cos();
        let q = [0.0, s, 0.0, c];
        let centre = [0.0, 0.0, 0.0];
        // Local +X face is turned 45 degrees toward world +X/+Z.
        let sphere = [0.778, 0.0, -0.778];
        let (normal, depth) = contact_oriented(
            sphere,
            [0.2; 3],
            SPHERE,
            [0.0, 0.0, 0.0, 1.0],
            centre,
            [1.0, 0.5, 0.25],
            BOX,
            q,
        )
        .expect("sphere overlaps the rotated face");
        assert!(normal[0] > 0.6 && normal[2] < -0.6, "normal={normal:?}");
        assert!(depth > 0.0);
    }
}
