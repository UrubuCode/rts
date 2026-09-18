//! A typed array's element kind as the byte the stream carries.
//!
//! Written out rather than derived from the enum's order: the byte is a format
//! surface that outlives any reordering of `buffers::element::Kind`, and a
//! derived number would change a stored file's meaning the day someone
//! inserted a variant.

use super::super::buffers::element::Kind;

pub(super) fn number_of(kind: Kind) -> u8 {
    match kind {
        Kind::Int8 => 0,
        Kind::Uint8 => 1,
        Kind::Uint8Clamped => 2,
        Kind::Int16 => 3,
        Kind::Uint16 => 4,
        Kind::Int32 => 5,
        Kind::Uint32 => 6,
        Kind::Float32 => 7,
        Kind::Float64 => 8,
        Kind::BigInt64 => 9,
        Kind::BigUint64 => 10,
        // The walk refuses a `DataView` before a node exists for it.
        Kind::Raw => 11,
    }
}

pub(super) fn kind_of(number: u8) -> Option<Kind> {
    Some(match number {
        0 => Kind::Int8,
        1 => Kind::Uint8,
        2 => Kind::Uint8Clamped,
        3 => Kind::Int16,
        4 => Kind::Uint16,
        5 => Kind::Int32,
        6 => Kind::Uint32,
        7 => Kind::Float32,
        8 => Kind::Float64,
        9 => Kind::BigInt64,
        10 => Kind::BigUint64,
        _ => return None,
    })
}
