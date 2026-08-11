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
    let d = sub(pa, pb);

    if sa > 0.5 && sb > 0.5 {
        return box_box(d, ha, hb);
    }
    if sa < 0.5 && sb < 0.5 {
        return sphere_sphere(d, ha, hb);
    }
    sphere_box(pa, ha, pb, hb, sb)
}

/// Box against box, resolved on the axis of LEAST penetration.
///
/// That choice is what makes a cube falling onto a wide floor be pushed up
/// rather than sideways: every axis overlaps, and the shallowest one is the one
/// the body actually came through.
fn box_box(d: V3, ha: V3, hb: V3) -> Option<(V3, f32)> {
    let o = [
        (ha[0] + hb[0]) - d[0].abs(),
        (ha[1] + hb[1]) - d[1].abs(),
        (ha[2] + hb[2]) - d[2].abs(),
    ];
    if o[0] <= 0.0 || o[1] <= 0.0 || o[2] <= 0.0 {
        return None;
    }
    if o[1] <= o[0] && o[1] <= o[2] {
        return Some(([0.0, axis_sign(d[1]), 0.0], o[1]));
    }
    if o[0] <= o[2] {
        return Some(([axis_sign(d[0]), 0.0, 0.0], o[0]));
    }
    Some(([0.0, 0.0, axis_sign(d[2])], o[2]))
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

/// Sphere against box: the point of the box closest to the sphere's centre.
///
/// `sign` converts the normal — which leaves the box towards the sphere — into
/// the B-towards-A convention: `+1` when A is the sphere, `-1` when A is the box.
fn sphere_box(pa: V3, ha: V3, pb: V3, hb: V3, sb: f32) -> Option<(V3, f32)> {
    let (bc, bh, sc, r, sign) = match sb > 0.5 {
        true => (pb, hb, pa, radius(ha), 1.0f32),
        false => (pa, ha, pb, radius(hb), -1.0f32),
    };
    let rel = sub(sc, bc);
    let q = [
        rel[0].clamp(-bh[0], bh[0]),
        rel[1].clamp(-bh[1], bh[1]),
        rel[2].clamp(-bh[2], bh[2]),
    ];
    let w = sub(sc, add(bc, q));
    let d2 = dot(w, w);
    if d2 >= r * r {
        return None;
    }
    if d2 > 0.000001 {
        let dist = d2.sqrt();
        return Some((scale(w, sign / dist), r - dist));
    }
    // The centre is INSIDE the box: there is no closest-point direction, so it
    // leaves through the face with the least clearance.
    let g = [
        bh[0] - q[0].abs(),
        bh[1] - q[1].abs(),
        bh[2] - q[2].abs(),
    ];
    if g[1] <= g[0] && g[1] <= g[2] {
        return Some(([0.0, axis_sign(q[1]) * sign, 0.0], g[1] + r));
    }
    if g[0] <= g[2] {
        return Some(([axis_sign(q[0]) * sign, 0.0, 0.0], g[0] + r));
    }
    Some(([0.0, 0.0, axis_sign(q[2]) * sign], g[2] + r))
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
}
