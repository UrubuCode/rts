//! The broad phase: a spatial hash, rebuilt once per sub-step.
//!
//! # The numbers are the kernel's, deliberately
//!
//! 8192 buckets, 32 slots each, and the 3D hash of `cellHash` in
//! `engine/rigid/gpurigid.ts` — which that project in turn shares with its fluid
//! solver. Two different answers to "what is a neighbourhood" between the GPU
//! backend and this one would put the two solvers on different pair sets, which
//! is the one divergence a position-comparison parity test cannot diagnose,
//! because everything would look slightly wrong rather than one thing looking
//! very wrong.
//!
//! # Why this build needs no atomics, where the kernel's does
//!
//! On the GPU one thread per body races every other to claim a slot, so the
//! claim is an `atomicAdd` — and the kernel isolates it in a pass of its own
//! because an atomic inside the hot pass pessimised the whole thing (+33 ms/frame
//! on DX12, measured in that project). Here the build is one sequential pass over
//! n bodies: at 2000 bodies it is 2000 increments and 2000 stores, which is
//! nothing beside the pair arithmetic that was measured as 98-99% of the cost.
//! Parallelising it would need the atomics back to buy a fraction of a percent.
//!
//! # What overflow does, and why it is not an error
//!
//! A bucket past its 32nd body drops the extra, exactly as the kernel does. That
//! is a missed pair rather than a crash, and it is the behaviour the GPU already
//! has — matching it is worth more here than being right differently.

/// How many buckets the hash has. `& 8191` in [`cell_hash`] depends on it.
pub const CELLS: usize = 8192;

/// How many bodies one bucket records.
pub const SLOTS: usize = 32;

/// Which bodies are near which cell.
pub struct Grid {
    counts: Vec<u32>,
    slots: Vec<u32>,
}

/// The bucket a cell coordinate falls in.
///
/// The multiplications wrap, which is what the WGSL does with `i32` arithmetic;
/// Rust would panic on overflow in a debug build instead, and a coordinate far
/// from the origin reaches it easily.
#[inline]
pub fn cell_hash(gx: i32, gy: i32, gz: i32) -> usize {
    let h = gx.wrapping_mul(73856093) ^ gy.wrapping_mul(19349663) ^ gz.wrapping_mul(83492791);
    (h as u32 & 8191) as usize
}

/// The cell coordinate a world position falls in.
#[inline]
pub fn cell_of(p: [f32; 3], size: f32) -> (i32, i32, i32) {
    (
        (p[0] / size).floor() as i32,
        (p[1] / size).floor() as i32,
        (p[2] / size).floor() as i32,
    )
}

impl Grid {
    /// An empty grid, allocated once and refilled per sub-step.
    ///
    /// One allocation of 8192 + 8192·32 words is 1 MB and it is held for the
    /// life of the world, because a sub-step that allocated it would pay 1 MB of
    /// zeroing four times a frame for a table it is about to overwrite anyway.
    pub fn new() -> Self {
        Self {
            counts: vec![0; CELLS],
            slots: vec![0; CELLS * SLOTS],
        }
    }

    /// Records every body by the cell its CENTRE falls in.
    ///
    /// The centre, and not the volume it covers — a body straddles cells. What
    /// makes the 27-cell scan exact rather than approximate is the cell being at
    /// least the largest DIAMETER wide, which is the caller's business
    /// ([`super::cell_size`]) and is why the size is not decided here.
    pub fn build(&mut self, positions: &[f32], count: usize, size: f32) {
        self.counts.fill(0);
        for body in 0..count {
            let p = [
                positions[body * 4],
                positions[body * 4 + 1],
                positions[body * 4 + 2],
            ];
            let (gx, gy, gz) = cell_of(p, size);
            let cell = cell_hash(gx, gy, gz);
            let at = self.counts[cell] as usize;
            if at < SLOTS {
                self.slots[cell * SLOTS + at] = body as u32;
                self.counts[cell] += 1;
            }
        }
    }

    /// The bodies recorded in one bucket.
    #[inline]
    pub fn bucket(&self, cell: usize) -> &[u32] {
        let at = cell * SLOTS;
        &self.slots[at..at + self.counts[cell] as usize]
    }

    /// Calls `visit` with every body in the 27 cells around `p`.
    ///
    /// The scan is written once because both places that need it — waking a
    /// sleeping body and solving its pairs — must agree about what "near" means,
    /// and the kernel has these as two copies of the same triple loop.
    #[inline]
    pub fn near(&self, p: [f32; 3], size: f32, mut visit: impl FnMut(usize)) {
        let (gx, gy, gz) = cell_of(p, size);
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    for &body in self.bucket(cell_hash(gx + dx, gy + dy, gz + dz)) {
                        visit(body as usize);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_found_from_every_one_of_the_twenty_seven_cells_around_it() {
        // The property the whole broad phase rests on: a neighbour one cell away
        // in any direction is still a candidate. A hash that collided the wrong
        // way, or a scan short by one, silently loses contacts.
        let mut grid = Grid::new();
        let positions = [5.0f32, 5.0, 5.0, 0.0];
        grid.build(&positions, 1, 1.0);
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let from = [5.0 + dx as f32, 5.0 + dy as f32, 5.0 + dz as f32];
                    let mut seen = 0;
                    grid.near(from, 1.0, |_| seen += 1);
                    assert!(seen >= 1, "lost the body from offset {dx},{dy},{dz}");
                }
            }
        }
    }

    #[test]
    fn a_bucket_past_its_capacity_drops_bodies_instead_of_growing() {
        // Matching the kernel matters more than being right differently: the GPU
        // has 32 slots and no way to grow one, so a CPU that kept all of them
        // would solve pairs the GPU never sees.
        let mut positions = vec![0.0f32; (SLOTS + 8) * 4];
        for body in 0..SLOTS + 8 {
            positions[body * 4] = 0.1 * body as f32;
        }
        let mut grid = Grid::new();
        grid.build(&positions, SLOTS + 8, 100.0);
        let (gx, gy, gz) = cell_of([0.0; 3], 100.0);
        assert_eq!(grid.bucket(cell_hash(gx, gy, gz)).len(), SLOTS);
    }

    #[test]
    fn a_position_far_from_the_origin_hashes_instead_of_overflowing() {
        // `i32` multiplication by 83492791 overflows well inside the range a
        // game reaches; the WGSL wraps and a debug build of Rust would panic.
        let (gx, gy, gz) = cell_of([1.0e6, -4.0e6, 9.0e6], 1.0);
        assert!(cell_hash(gx, gy, gz) < CELLS);
    }
}
