//! `Buffer` — a `Uint8Array` subclass with its own prototype, per
//! `docs/reference/node/layering.md` line 113: bytes and codecs need no
//! operating system, so `Buffer` is a primitive rather than a `node:buffer`
//! fabrication.
//!
//! # What this replaces
//!
//! Until now `rts-node`'s `buffer.rs` answered every `Buffer.from`/`.alloc`/
//! `.concat` with a plain `Uint8Array`, and refused **every instance method** by
//! name — see that module's own doc, which explains why a functions-over-typed-
//! array shape could not carry `buf.toString()` without a prototype to hang it
//! on. This is that prototype. `Buffer.prototype.toString`/`write`/`slice`/…
//! are real, own the same way `Error.prototype.toString` is, and
//! `Object.keys(buf)` does not see them — the divergence that module's doc
//! names as the reason a fake was rejected.
//!
//! # Why `extends = register_uint8_array`
//!
//! A `Buffer` inherits every `Uint8Array` member — `at`, `set`, `fill` (Buffer's
//! own `fill` below shadows it, taking an encoding), indexing, `.length` — for
//! the same reason `TypeError extends Error` inherits `toString`: the parent's
//! prototype is linked in by the attribute, and nothing here restates a member
//! the parent already answers correctly.
//!
//! # Why `slice` AND `subarray` are declared here, sharing bytes
//!
//! `TypedArray.prototype.slice` copies; `Uint8Array`'s inherited one would too.
//! Node's `Buffer.prototype.slice` does not — it is documented to share memory
//! with the original, kept for backward compatibility with the pre-TypedArray
//! `Buffer`. So both are declared here, over the same view-sharing logic
//! [`ops::windowed`] any other engine's implementation is.
//!
//! # The codecs live in [`codec`], moved rather than copied
//!
//! See that module's doc — `rts-node::buffer` no longer has one of its own.
//!
//! # What an argument has to be, and why that is [`validate`] and not here
//!
//! Every member above used to accept anything and answer something: a negative
//! size clamped to zero, a string where a `Buffer` was wanted compared as empty,
//! an offset past the end read as `NaN`. Node refuses all three with a specific
//! `code`, and **17 files of its own suite died on "Missing expected
//! exception"** because of it (measured 2026-08-24). The checks are one module
//! rather than a line per member because the same three questions are asked
//! fourteen times over — see [`validate`], which also documents the borrow rule
//! that decides their shape.
pub(in crate::entry) mod bigint;
pub(in crate::entry) mod codec;
pub(in crate::entry) mod ops;
pub(in crate::entry) mod search;
pub(in crate::entry) mod statics;
pub(in crate::entry) mod string;
pub(in crate::entry) mod swap;
pub(in crate::entry) mod validate;
pub(in crate::entry) mod write;
use super::buffers::element::Kind;
use super::buffers::uint8_array;
use super::Context;
use crate::entry;
/// `Buffer`.
#[rtse::class("Buffer", extends = uint8_array, method_prototypes)]
impl Buffer {
    /// `Buffer(size)` / `new Buffer(size)` and the legacy source overloads.
    ///
    /// Node keeps this callable for compatibility even though modern code uses
    /// `Buffer.from` or `Buffer.alloc`. Numbers follow the legacy unsafe-size
    /// path only without an encoding; all other sources reuse `from`.
    #[construct]
    fn build(this: u64, source: u64, encoding_or_offset: u64, length: u64) -> u64 {
        let _ = this;
        if entry::number_of(source).is_some() {
            if !matches!(validate::shape_of(encoding_or_offset), validate::Shape::Absent) {
                entry::invalid_arg_type("string", "string", source);
                return entry::undefined_value();
            }
            return statics::alloc_unsafe(source);
        }
        statics::from(source, encoding_or_offset, length)
    }

    /// `Buffer.poolSize` — Node's allocation-pool size. Meaningless here: this
    /// engine's `allocUnsafe` zero-fills fresh memory rather than drawing from a
    /// shared pool (see `rts-node`'s former module doc), so the property
    /// exists to be read and never influences an allocation.
    #[stat]
    const poolSize: f64 = 8192.0;

    /// `Buffer.alloc(size, fill?, encoding?)`.
    ///
    /// `size` crosses as a raw value and not as an `f64`, and every offset below
    /// does the same: the coercion the attribute would insert answers `NaN` for
    /// `Buffer.alloc('x')`, which is a *range* mistake where Node reports a
    /// *type* one — and `undefined` and `0` become indistinguishable, so
    /// `Buffer.alloc()` would allocate nothing instead of being refused. See
    /// [`validate`].
    #[stat]
    fn alloc(size: u64, fill: u64, encoding: u64) -> u64 {
        statics::alloc(size, fill, encoding)
    }
    /// `Buffer.allocUnsafe(size)`.
    #[stat]
    fn alloc_unsafe(size: u64) -> u64 {
        statics::alloc_unsafe(size)
    }
    /// `Buffer.allocUnsafeSlow(size)`.
    #[stat]
    fn alloc_unsafe_slow(size: u64) -> u64 { statics::alloc_unsafe(size) }
    /// `Buffer.from(source, encodingOrOffset?)`.
    #[stat]
    fn from(source: u64, encoding_or_offset: u64, length: u64) -> u64 {
        statics::from(source, encoding_or_offset, length)
    }
    /// `Buffer.of(...values)`.
    #[stat]
    fn of(a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
        statics::of(a0, a1, a2, a3)
    }
    /// `Buffer.concat(list, totalLength?)`.
    #[stat]
    fn concat(list: u64, total_length: u64) -> u64 {
        statics::concat(list, total_length)
    }
    /// `Buffer.byteLength(source, encoding?)`.
    #[stat]
    fn byte_length(source: u64, encoding: u64) -> f64 {
        statics::byte_length(source, encoding)
    }

    /// `Buffer.isBuffer(value)`.
    #[stat]
    fn is_buffer(value: u64) -> bool {
        statics::is_buffer(value)
    }

    /// `Buffer.isEncoding(name)`.
    #[stat]
    fn is_encoding(encoding: u64) -> bool {
        statics::is_encoding(encoding)
    }

    /// `Buffer.compare(a, b)`. Named apart from the instance method below —
    /// Rust has no overloading — and given the JS name back with `#[js]`.
    #[stat]
    #[js("compare")]
    fn compare_static(a: u64, b: u64) -> f64 {
        statics::compare_values(a, b, "buf1", "buf2")
    }

    /// `buf.toString(encoding?, start?, end?)`.
    fn to_string(this: u64, encoding: u64, start: u64, end: u64) -> u64 {
        string::to_string(this, encoding, start, end)
    }

    /// `buf.write(string, offset?, length?, encoding?)`.
    fn write(this: u64, string: u64, offset: u64, length: u64, encoding: u64) -> f64 {
        ops::write(this, string, offset, length, encoding)
    }

    /// `buf.slice(begin?, end?)` — shares bytes; see the module doc.
    fn slice(this: u64, begin: u64, end: u64) -> u64 {
        ops::windowed(this, begin, end)
    }

    /// `buf.subarray(begin?, end?)` — the same sharing `slice` gives here.
    fn subarray(this: u64, begin: u64, end: u64) -> u64 {
        ops::windowed(this, begin, end)
    }

    /// `buf.equals(other)`.
    fn equals(this: u64, other: u64) -> bool {
        ops::equals(this, other)
    }

    /// `buf.compare(target)`.
    fn compare(this: u64, other: u64) -> f64 {
        statics::compare_values(this, other, "source", "target")
    }

    /// `buf.copy(target, targetStart?, sourceStart?, sourceEnd?)`.
    fn copy(this: u64, target: u64, target_start: u64, source_start: u64, source_end: u64) -> f64 {
        ops::copy(this, target, target_start, source_start, source_end)
    }

    /// `buf.fill(value, begin?, end?, encoding?)`.
    fn fill(this: u64, value: u64, begin: u64, end: u64, encoding: u64) -> u64 {
        ops::fill(this, value, begin, end, encoding)
    }

    /// `buf.indexOf(value, byteOffset?, encoding?)`.
    fn index_of(this: u64, value: u64, byte_offset: u64, encoding: u64) -> f64 {
        ops::index_of(this, value, byte_offset, encoding)
    }

    /// `buf.lastIndexOf(value, byteOffset?, encoding?)`.
    #[js("lastIndexOf")]
    fn last_index_of(this: u64, value: u64, byte_offset: u64, encoding: u64) -> f64 {
        ops::last_index_of(this, value, byte_offset, encoding)
    }

    /// `buf.includes(value, byteOffset?, encoding?)`.
    fn includes(this: u64, value: u64, byte_offset: u64, encoding: u64) -> bool {
        ops::includes(this, value, byte_offset, encoding)
    }

    // BigInt64 and BigUint64 accessors share the dedicated word codec.
    #[js("readBigInt64LE")]
    fn read_big_int64_le(this: u64, offset: u64) -> u64 { bigint::read(this, offset, Kind::BigInt64, true) }
    #[js("readBigInt64BE")]
    fn read_big_int64_be(this: u64, offset: u64) -> u64 { bigint::read(this, offset, Kind::BigInt64, false) }
    #[js("readBigUInt64LE")]
    fn read_big_u_int64_le(this: u64, offset: u64) -> u64 { bigint::read(this, offset, Kind::BigUint64, true) }
    #[js("readBigUInt64BE")]
    fn read_big_u_int64_be(this: u64, offset: u64) -> u64 { bigint::read(this, offset, Kind::BigUint64, false) }
    #[js("writeBigInt64LE")]
    fn write_big_int64_le(this: u64, value: u64, offset: u64) -> f64 { bigint::write(this, value, offset, Kind::BigInt64, true) }
    #[js("writeBigInt64BE")]
    fn write_big_int64_be(this: u64, value: u64, offset: u64) -> f64 { bigint::write(this, value, offset, Kind::BigInt64, false) }
    #[js("writeBigUInt64LE")]
    fn write_big_u_int64_le(this: u64, value: u64, offset: u64) -> f64 { bigint::write(this, value, offset, Kind::BigUint64, true) }
    #[js("writeBigUInt64BE")]
    fn write_big_u_int64_be(this: u64, value: u64, offset: u64) -> f64 { bigint::write(this, value, offset, Kind::BigUint64, false) }

    /// `buf.swap16()` — reverse each 16-bit word in place.
    #[js("swap16")]
    fn swap_16(this: u64) -> u64 {
        swap::swap(this, 2)
    }

    /// `buf.swap32()` — reverse each 32-bit word in place.
    #[js("swap32")]
    fn swap_32(this: u64) -> u64 {
        swap::swap(this, 4)
    }

    /// `buf.swap64()` — reverse each 64-bit word in place.
    #[js("swap64")]
    fn swap_64(this: u64) -> u64 {
        swap::swap(this, 8)
    }

    /// `buf.toJSON()`.
    #[js("toJSON")]
    fn to_json(this: u64) -> u64 {
        ops::to_json(this)
    }

    /// `buf.readInt8(offset?)`.
    fn read_int8(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Int8, true)
    }

    /// `buf.writeInt8(value, offset?)`.
    fn write_int8(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Int8, true)
    }

    /// `buf.readUInt8(offset?)`.
    fn read_u_int8(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Uint8, true)
    }

    /// `buf.writeUInt8(value, offset?)`.
    fn write_u_int8(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Uint8, true)
    }

    /// `buf.readUInt16LE(offset?)`.
    #[js("readUInt16LE")]
    fn read_u_int16_le(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Uint16, true)
    }

    /// `buf.readUInt16BE(offset?)`.
    #[js("readUInt16BE")]
    fn read_u_int16_be(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Uint16, false)
    }

    /// `buf.writeUInt16LE(value, offset?)`.
    #[js("writeUInt16LE")]
    fn write_u_int16_le(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Uint16, true)
    }

    /// `buf.writeUInt16BE(value, offset?)`.
    #[js("writeUInt16BE")]
    fn write_u_int16_be(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Uint16, false)
    }

    /// `buf.readInt16LE(offset?)`.
    #[js("readInt16LE")]
    fn read_int16_le(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Int16, true)
    }

    /// `buf.readInt16BE(offset?)`.
    #[js("readInt16BE")]
    fn read_int16_be(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Int16, false)
    }

    /// `buf.writeInt16LE(value, offset?)`.
    #[js("writeInt16LE")]
    fn write_int16_le(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Int16, true)
    }

    /// `buf.writeInt16BE(value, offset?)`.
    #[js("writeInt16BE")]
    fn write_int16_be(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Int16, false)
    }

    /// `buf.readUIntLE(offset?, byteLength)`.
    #[js("readUIntLE")]
    fn read_u_int_le(this: u64, offset: u64, byte_length: u64) -> f64 {
        ops::read_variable(this, offset, byte_length, true, false)
    }

    /// `buf.readUIntBE(offset?, byteLength)`.
    #[js("readUIntBE")]
    fn read_u_int_be(this: u64, offset: u64, byte_length: u64) -> f64 {
        ops::read_variable(this, offset, byte_length, false, false)
    }

    /// `buf.writeUIntLE(value, offset?, byteLength)`.
    #[js("writeUIntLE")]
    fn write_u_int_le(this: u64, value: f64, offset: u64, byte_length: u64) -> f64 {
        ops::write_variable(this, value, offset, byte_length, true, false)
    }

    /// `buf.writeUIntBE(value, offset?, byteLength)`.
    #[js("writeUIntBE")]
    fn write_u_int_be(this: u64, value: f64, offset: u64, byte_length: u64) -> f64 {
        ops::write_variable(this, value, offset, byte_length, false, false)
    }

    /// `buf.readIntLE(offset?, byteLength)`.
    #[js("readIntLE")]
    fn read_int_le(this: u64, offset: u64, byte_length: u64) -> f64 {
        ops::read_variable(this, offset, byte_length, true, true)
    }

    /// `buf.readIntBE(offset?, byteLength)`.
    #[js("readIntBE")]
    fn read_int_be(this: u64, offset: u64, byte_length: u64) -> f64 {
        ops::read_variable(this, offset, byte_length, false, true)
    }

    /// `buf.writeIntLE(value, offset?, byteLength)`.
    #[js("writeIntLE")]
    fn write_int_le(this: u64, value: f64, offset: u64, byte_length: u64) -> f64 {
        ops::write_variable(this, value, offset, byte_length, true, true)
    }

    /// `buf.writeIntBE(value, offset?, byteLength)`.
    #[js("writeIntBE")]
    fn write_int_be(this: u64, value: f64, offset: u64, byte_length: u64) -> f64 {
        ops::write_variable(this, value, offset, byte_length, false, true)
    }

    /// `buf.readUInt32LE(offset?)`.
    ///
    /// The four `UInt32` members were the hole in this family and not a
    /// deliberate omission: `Kind::Uint32` has always been in the codec, the
    /// declarations were simply absent, and `test-buffer-readuint.js` iterates
    /// `['UInt8', 'UInt16BE', 'UInt16LE', 'UInt32BE', 'UInt32LE']` — so it died
    /// calling `undefined` before it could reach a single argument check.
    #[js("readUInt32LE")]
    fn read_u_int32_le(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Uint32, true)
    }

    /// `buf.readUInt32BE(offset?)`.
    #[js("readUInt32BE")]
    fn read_u_int32_be(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Uint32, false)
    }

    /// `buf.writeUInt32LE(value, offset?)`.
    #[js("writeUInt32LE")]
    fn write_u_int32_le(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Uint32, true)
    }

    /// `buf.writeUInt32BE(value, offset?)`.
    #[js("writeUInt32BE")]
    fn write_u_int32_be(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Uint32, false)
    }

    /// `buf.readInt32LE(offset?)`.
    #[js("readInt32LE")]
    fn read_int32_le(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Int32, true)
    }

    /// `buf.readInt32BE(offset?)`.
    #[js("readInt32BE")]
    fn read_int32_be(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Int32, false)
    }

    /// `buf.writeInt32LE(value, offset?)`.
    #[js("writeInt32LE")]
    fn write_int32_le(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Int32, true)
    }

    /// `buf.writeInt32BE(value, offset?)`.
    #[js("writeInt32BE")]
    fn write_int32_be(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Int32, false)
    }

    /// `buf.readFloatLE(offset?)`.
    #[js("readFloatLE")]
    fn read_float_le(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Float32, true)
    }

    /// `buf.writeFloatLE(value, offset?)`.
    #[js("writeFloatLE")]
    fn write_float_le(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Float32, true)
    }

    /// `buf.readFloatBE(offset?)`.
    #[js("readFloatBE")]
    fn read_float_be(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Float32, false)
    }

    /// `buf.writeFloatBE(value, offset?)`.
    #[js("writeFloatBE")]
    fn write_float_be(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Float32, false)
    }

    /// `buf.readDoubleLE(offset?)`.
    #[js("readDoubleLE")]
    fn read_double_le(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Float64, true)
    }

    /// `buf.readDoubleBE(offset?)`.
    #[js("readDoubleBE")]
    fn read_double_be(this: u64, offset: u64) -> f64 {
        ops::read_num(this, offset, Kind::Float64, false)
    }

    /// `buf.writeDoubleBE(value, offset?)`.
    #[js("writeDoubleBE")]
    fn write_double_be(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Float64, false)
    }

    /// `buf.writeDoubleLE(value, offset?)`.
    #[js("writeDoubleLE")]
    fn write_double_le(this: u64, value: f64, offset: u64) -> f64 {
        ops::write_num(this, value, offset, Kind::Float64, true)
    }
}

/// Register Buffer and install Node's spelling aliases without generating a
/// second native wrapper for each `UInt`/`Uint` pair.
pub(in crate::entry) fn register_buffer_with_aliases(context: &mut Context) -> u64 {
    let constructor = register_buffer(context);
    let Some(prototype) = super::class_support::prototype(context, "Buffer") else {
        return constructor;
    };
    for (upper, lower) in [
        ("readUInt8", "readUint8"),
        ("writeUInt8", "writeUint8"),
        ("readUInt16LE", "readUint16LE"),
        ("writeUInt16LE", "writeUint16LE"),
        ("readUInt16BE", "readUint16BE"),
        ("writeUInt16BE", "writeUint16BE"),
        ("readUInt32LE", "readUint32LE"),
        ("writeUInt32LE", "writeUint32LE"),
        ("readUInt32BE", "readUint32BE"),
        ("writeUInt32BE", "writeUint32BE"),
        ("readUIntLE", "readUintLE"),
        ("writeUIntLE", "writeUintLE"),
        ("readUIntBE", "readUintBE"),
        ("writeUIntBE", "writeUintBE"),
        ("readBigUInt64LE", "readBigUint64LE"),
        ("writeBigUInt64LE", "writeBigUint64LE"),
        ("readBigUInt64BE", "readBigUint64BE"),
        ("writeBigUInt64BE", "writeBigUint64BE"),
    ] {
        let value = super::modules::get_member(context, prototype, upper);
        if value != super::modules::undefined_in(context) {
            super::modules::put_member(context, prototype, lower, value);
        }
    }
    write::install_aliases(context, prototype);
    constructor
}
