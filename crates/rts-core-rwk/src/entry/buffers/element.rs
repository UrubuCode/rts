//! What an element IS: how wide it is, how it is spelled in bytes, and what
//! happens to a number that does not fit.
//!
//! # Why one codec and not one per class
//!
//! The eight typed arrays differ in exactly this file and nowhere else. Written
//! out per class it is eight copies of "gather the bytes, order them, reinterpret
//! them" — and the crate's rule 2 answer applies with unusual force here, because
//! the copies would not fail loudly. They would agree for every value a test
//! happens to try and disagree at the edge: `Int8Array` storing 128 is `-128`,
//! and a copy that reached for a saturating cast would answer `127` while every
//! value under 128 kept working.
//!
//! # Why writing WRAPS and does not saturate
//!
//! `a[0] = 300` on an `Int8Array` stores `44`, not `127`. The language says
//! ToIntegerOrInfinity then modulo 2^n then reinterpret, which is what the
//! hardware does for a truncating store and what every other engine answers.
//! Saturation is the rejected alternative and it is the dangerous one: it is
//! correct for every value in range, so it survives any test written from
//! examples rather than from the edge. `Uint8ClampedArray` is the one class that
//! genuinely saturates, and it is absent — see the module documentation.
//!
//! # Why `Raw`
//!
//! A `DataView` has no element type of its own: its width comes from the method
//! called, not from the object. It still needs to be a [`Kind`] because a view is
//! one record whichever class made it, and a separate "kind or nothing" would put
//! an `Option` on every field read to serve one class.

/// The element type a view reads through.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::entry) enum Kind {
    Int8,
    Uint8,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float32,
    Float64,
    /// A `DataView`, which decides the width per call.
    Raw,
}

impl Kind {
    /// How many bytes one element occupies.
    ///
    /// One for `Raw`, so that a `DataView`'s byte length and its "element" count
    /// are the same number and the range arithmetic needs no special case.
    pub(in crate::entry) const fn size(self) -> usize {
        match self {
            Kind::Int8 | Kind::Uint8 | Kind::Raw => 1,
            Kind::Int16 | Kind::Uint16 => 2,
            Kind::Int32 | Kind::Uint32 | Kind::Float32 => 4,
            Kind::Float64 => 8,
        }
    }

    /// How many bits an integer of this kind keeps, and whether it is signed.
    ///
    /// `None` for the floats and for `Raw`, which is what routes the write to
    /// bit-pattern conversion instead of to modular wrapping.
    const fn integer(self) -> Option<(u32, bool)> {
        match self {
            Kind::Int8 => Some((8, true)),
            Kind::Uint8 => Some((8, false)),
            Kind::Int16 => Some((16, true)),
            Kind::Uint16 => Some((16, false)),
            Kind::Int32 => Some((32, true)),
            Kind::Uint32 => Some((32, false)),
            _ => None,
        }
    }
}

/// The bytes at an offset, as one word, in the byte order asked for.
///
/// A word rather than a `[u8; 8]` because every reinterpretation below is a cast
/// off the low bits, and assembling once is what keeps the endianness decision in
/// exactly one place. Getting it into two places is how a `DataView` and a typed
/// array over the same buffer come to disagree about what byte three is.
fn gathered(bytes: &[u8], at: usize, size: usize, little: bool) -> Option<u64> {
    let slice = bytes.get(at..at.checked_add(size)?)?;
    let mut word = 0u64;
    for (index, byte) in slice.iter().enumerate() {
        let shift = match little {
            true => index,
            false => size - 1 - index,
        };
        word |= u64::from(*byte) << (shift * 8);
    }
    Some(word)
}

/// The number an element holds.
///
/// `None` is out of range or `Raw`, and both are the caller's to answer:
/// out of range is `undefined` for a typed array and a `RangeError` for a
/// `DataView` — which this engine cannot raise where a handler could catch it,
/// so it is `undefined` there too and that is the stated gap.
pub(in crate::entry) fn read(bytes: &[u8], at: usize, kind: Kind, little: bool) -> Option<f64> {
    let word = gathered(bytes, at, kind.size(), little)?;
    Some(match kind {
        Kind::Int8 => f64::from(word as u8 as i8),
        Kind::Uint8 => f64::from(word as u8),
        Kind::Int16 => f64::from(word as u16 as i16),
        Kind::Uint16 => f64::from(word as u16),
        Kind::Int32 => f64::from(word as u32 as i32),
        Kind::Uint32 => f64::from(word as u32),
        Kind::Float32 => f64::from(f32::from_bits(word as u32)),
        Kind::Float64 => f64::from_bits(word),
        // No width of its own. A `DataView` reaches this through the kind its
        // method names, never through `Raw`.
        Kind::Raw => return None,
    })
}

/// The bits an element would hold, which is where the wrapping happens.
pub(in crate::entry) fn to_bits(value: f64, kind: Kind) -> u64 {
    match kind {
        Kind::Float32 => u64::from((value as f32).to_bits()),
        Kind::Float64 => value.to_bits(),
        Kind::Raw => 0,
        _ => match kind.integer() {
            Some((bits, _)) => wrapped(value, bits),
            None => 0,
        },
    }
}

/// ToIntegerOrInfinity, then modulo 2^bits.
///
/// The modulus is a power of two, so `%` on an `f64` is exact — no rounding is
/// introduced by taking the remainder before the cast. Casting first was the
/// rejected alternative and it is wrong twice over: `as u32` on an out-of-range
/// `f64` saturates in Rust, and it saturates *silently*.
fn wrapped(value: f64, bits: u32) -> u64 {
    // NaN and both infinities become zero, which is ToIntegerOrInfinity followed
    // by the modulo the specification applies to a non-finite as zero.
    if !value.is_finite() {
        return 0;
    }
    let modulus = 2f64.powi(bits as i32);
    let mut folded = value.trunc() % modulus;
    if folded < 0.0 {
        folded += modulus;
    }
    folded as u64
}

/// Writes an element, in the byte order asked for.
///
/// Answers whether it landed. Out of range writes nothing, which is what a typed
/// array does — `a[99] = 1` on a three-element one is not an error and not a
/// property either.
pub(in crate::entry) fn write(
    bytes: &mut [u8],
    at: usize,
    kind: Kind,
    value: f64,
    little: bool,
) -> bool {
    let size = kind.size();
    if kind == Kind::Raw {
        return false;
    }
    let Some(end) = at.checked_add(size) else {
        return false;
    };
    if end > bytes.len() {
        return false;
    }
    let word = to_bits(value, kind);
    for index in 0..size {
        let shift = match little {
            true => index,
            false => size - 1 - index,
        };
        bytes[at + index] = (word >> (shift * 8)) as u8;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_past_the_width_wraps_rather_than_clamping() {
        // The one mistake that looks right for every value under 128. 300 stored
        // into an `Int8Array` is 44, and a saturating implementation answers 127
        // while passing every test written from small numbers.
        let mut bytes = [0u8; 1];
        assert!(write(&mut bytes, 0, Kind::Int8, 300.0, true));
        assert_eq!(read(&bytes, 0, Kind::Int8, true), Some(44.0));

        // And the sign comes from the reinterpretation, not from the input:
        // 128 fits in eight bits and is negative when they are read as signed.
        assert!(write(&mut bytes, 0, Kind::Int8, 128.0, true));
        assert_eq!(read(&bytes, 0, Kind::Int8, true), Some(-128.0));

        assert!(write(&mut bytes, 0, Kind::Uint8, -1.0, true));
        assert_eq!(read(&bytes, 0, Kind::Uint8, true), Some(255.0));
    }

    #[test]
    fn a_fraction_truncates_toward_zero_and_a_nan_is_zero() {
        let mut bytes = [0u8; 4];
        assert!(write(&mut bytes, 0, Kind::Int32, -1.9, true));
        assert_eq!(read(&bytes, 0, Kind::Int32, true), Some(-1.0));
        assert!(write(&mut bytes, 0, Kind::Int32, f64::NAN, true));
        assert_eq!(read(&bytes, 0, Kind::Int32, true), Some(0.0));
        assert!(write(&mut bytes, 0, Kind::Uint32, 4_294_967_296.0, true));
        assert_eq!(read(&bytes, 0, Kind::Uint32, true), Some(0.0));
    }

    #[test]
    fn the_two_byte_orders_are_the_same_bytes_read_the_other_way() {
        // What `DataView`'s `littleEndian` argument selects, and the reason the
        // gathering is written once: a typed array and a view over the same
        // bytes must disagree only where the caller asked them to.
        let mut bytes = [0u8; 2];
        assert!(write(&mut bytes, 0, Kind::Uint16, 0x0102 as f64, false));
        assert_eq!(bytes, [0x01, 0x02], "big-endian puts the high byte first");
        assert_eq!(read(&bytes, 0, Kind::Uint16, true), Some(0x0201 as f64));
    }

    #[test]
    fn a_float_keeps_its_bit_pattern_through_the_narrower_type() {
        let mut bytes = [0u8; 8];
        assert!(write(&mut bytes, 0, Kind::Float32, 0.5, true));
        assert_eq!(read(&bytes, 0, Kind::Float32, true), Some(0.5));
        // 0.1 is not representable in either width, and the narrower one loses
        // more of it — which is the class's behaviour and not a defect.
        assert!(write(&mut bytes, 0, Kind::Float64, 0.1, true));
        assert_eq!(read(&bytes, 0, Kind::Float64, true), Some(0.1));
        assert!(write(&mut bytes, 0, Kind::Float32, 0.1, true));
        assert_ne!(read(&bytes, 0, Kind::Float32, true), Some(0.1));
    }
}
