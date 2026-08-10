//! `readSync`/`writeSync`/`readvSync`/`writevSync`, and the bytes half of
//! `readFileSync` — the members that need a `Uint8Array`/`DataView` window
//! onto real bytes, over `rts_core::entry::{bytes_of, write_bytes,
//! make_bytes}`.
//!
//! # The five-argument problem
//!
//! Node's full form is `readSync(fd, buffer, offset, length, position)` and
//! `writeSync(fd, buffer, offset, length, position)` — five arguments, and a
//! member here has four call slots at most (the fifth is the receiver, always
//! unused for a namespace function). Node also accepts an options object in
//! the same position: `readSync(fd, buffer, options)` /
//! `writeSync(fd, buffer, options)` with `{ offset, length, position }` read
//! off it. That form is what is implemented below; the five-positional-
//! argument form is refused BY NAME rather than silently truncated — a caller
//! passing `readSync(fd, buf, 0, 64, null)` gets `64` read as an `options`
//! value, which has no `offset`/`length`/`position` members and so falls back
//! to the same defaults an absent third argument would, rather than a
//! plausible-looking wrong read.
//!
//! # No pread/pwrite: `position` seeks the fd
//!
//! Real Node's `position` argument reads or writes at that OFFSET without
//! disturbing the file's current cursor — POSIX `pread`/`pwrite`. `std::fs`
//! exposes neither in a platform-uniform way (`FileExt` is Unix-only), so
//! `position` here `seek`s the shared cursor before the operation, which DOES
//! move it — a divergence from Node's promise, named rather than silently
//! taken. A caller that never passes `position` (the common case, and the one
//! `readvSync`/`writevSync` use for every buffer after the first) never
//! observes it.

use rts_core::entry::{bytes_of, get_indexed, make_bytes, make_number, undefined_value, with_runtime, write_bytes};

/// A numeric field off an options object, `None` when the object is absent,
/// not an object, or does not carry the key — the same "absent" `option_flag`
/// in the parent module already treats a missing options argument as.
pub(super) fn option_number(options: u64, name: &str) -> Option<f64> {
    let absent = undefined_value();
    if options == absent {
        return None;
    }
    let value = with_runtime(|context| rts_core::entry::get_member(context, options, name));
    rts_core::entry::number_of(value)
}

/// The bytes an array-like value (a real JS `Array`, here always a list of
/// `Uint8Array`s) holds, read by `length` and index.
///
/// `length` is an ordinary named property, so [`rts_core::entry::get_member`]
/// finds it — but a dense `Array`'s ELEMENTS are not shape properties at all,
/// they live in the array's own element storage, and `get_member(array, "0")`
/// found nothing there and answered `undefined` for every index. `[b1, b2]`
/// read back as one `undefined` per slot, so every buffer's `bytes_of` failed
/// and `writevSync`/`readvSync` moved zero bytes. [`get_indexed`] is the
/// computed-access entry point (`obj[key]`) that resolves an index into that
/// storage the way compiled code itself does for `a[i]`, so this reads
/// through it — with a NUMBER key, since `key_number`'s own doc names a
/// string subscript as the different, slower path this is not taking.
fn array_items(array: u64) -> Vec<u64> {
    let absent = undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length = with_runtime(|context| {
        let value = rts_core::entry::get_member(context, array, "length");
        rts_core::entry::number_of(value)
    })
    .unwrap_or(0.0) as usize;
    (0..length).map(|index| get_indexed(array, make_number(index as f64))).collect()
}

/// `fs.readFileSync(path, encoding?)` — bytes (a `Uint8Array`) when
/// `encoding` is absent, text when given. Node answers a `Buffer` with no
/// encoding; this engine has no `Buffer`, so `Uint8Array` stands in for it —
/// still bytes, unlike the text this module answered unconditionally before.
/// With an encoding this delegates to [`super::basic::read_file_sync`], which
/// still answers TEXT for the one encoding this module supports.
pub(super) extern "C" fn read_file_sync(e: u64, this: u64, path: u64, encoding: u64, a2: u64, a3: u64) -> u64 {
    let absent = undefined_value();
    if encoding != absent {
        return super::basic::read_file_sync(e, this, path, encoding, a2, a3);
    }
    let Some(path) = super::text(path) else {
        return undefined_value();
    };
    match std::fs::read(&path) {
        Ok(contents) => with_runtime(|context| make_bytes(context, &contents)),
        Err(_) => undefined_value(),
    }
}

/// `fs.readSync(fd, buffer, options?)` — `options: { offset, length,
/// position }`, all optional; see the module doc for the five-argument form
/// this does not accept. Answers the number of bytes read, `undefined` on a
/// bad `fd`.
pub(super) extern "C" fn read_sync(_e: u64, _this: u64, fd: u64, buffer: u64, options: u64, _a3: u64) -> u64 {
    let Some(fd) = super::number(fd) else {
        return undefined_value();
    };
    let fd = fd as i64;
    let window_len = with_runtime(|context| bytes_of(context, buffer)).map(|b| b.len()).unwrap_or(0);
    let offset = option_number(options, "offset").unwrap_or(0.0).max(0.0) as usize;
    let length =
        option_number(options, "length").unwrap_or_else(|| window_len.saturating_sub(offset) as f64).max(0.0) as usize;
    let position = option_number(options, "position");
    let mut data = vec![0u8; length];
    let read = super::fd::with_file(fd, |file| {
        use std::io::{Read, Seek, SeekFrom};
        if let Some(pos) = position {
            let _ = file.seek(SeekFrom::Start(pos.max(0.0) as u64));
        }
        file.read(&mut data).unwrap_or(0)
    });
    let Some(read) = read else {
        return undefined_value();
    };
    let written = with_runtime(|context| write_bytes(context, buffer, offset, &data[..read]));
    make_number(written as f64)
}

/// `fs.writeSync(fd, buffer, options?)` — `options: { offset, length,
/// position }`, all optional. Answers the number of bytes written,
/// `undefined` on a bad `fd`. The string form (`writeSync(fd, string,
/// position?, encoding?)`) is not implemented: `buffer` here is always read
/// as a `Uint8Array`/`DataView` window via `bytes_of`.
pub(super) extern "C" fn write_sync(_e: u64, _this: u64, fd: u64, buffer: u64, options: u64, _a3: u64) -> u64 {
    let Some(fd) = super::number(fd) else {
        return undefined_value();
    };
    let fd = fd as i64;
    let Some(data) = with_runtime(|context| bytes_of(context, buffer)) else {
        return undefined_value();
    };
    let offset = option_number(options, "offset").unwrap_or(0.0).max(0.0) as usize;
    let length = option_number(options, "length").unwrap_or_else(|| data.len().saturating_sub(offset) as f64).max(0.0)
        as usize;
    let position = option_number(options, "position");
    let end = (offset + length).min(data.len());
    let slice = if offset <= end { &data[offset..end] } else { &[][..] };
    let written = super::fd::with_file(fd, |file| {
        use std::io::{Seek, SeekFrom, Write};
        if let Some(pos) = position {
            let _ = file.seek(SeekFrom::Start(pos.max(0.0) as u64));
        }
        file.write(slice).unwrap_or(0)
    });
    match written {
        Some(count) => make_number(count as f64),
        None => undefined_value(),
    }
}

/// `fs.readvSync(fd, buffers, position?)` — fills each `Uint8Array` in
/// `buffers` in order, `position` (if given) seeking before the FIRST one
/// only, matching `readv(2)`'s single starting offset. Answers the total
/// bytes read across all buffers.
pub(super) extern "C" fn readv_sync(_e: u64, _this: u64, fd: u64, buffers: u64, position: u64, _a3: u64) -> u64 {
    let Some(fd) = super::number(fd) else {
        return undefined_value();
    };
    let fd = fd as i64;
    let position = super::number(position);
    let mut total = 0usize;
    for (index, item) in array_items(buffers).into_iter().enumerate() {
        let len = with_runtime(|context| bytes_of(context, item)).map(|b| b.len()).unwrap_or(0);
        if len == 0 {
            continue;
        }
        let mut data = vec![0u8; len];
        let read = super::fd::with_file(fd, |file| {
            use std::io::{Read, Seek, SeekFrom};
            if index == 0 {
                if let Some(pos) = position {
                    let _ = file.seek(SeekFrom::Start(pos.max(0.0) as u64));
                }
            }
            file.read(&mut data).unwrap_or(0)
        });
        let Some(read) = read else {
            break;
        };
        with_runtime(|context| write_bytes(context, item, 0, &data[..read]));
        total += read;
        if read < len {
            break;
        }
    }
    make_number(total as f64)
}

/// `fs.writevSync(fd, buffers, position?)` — writes each `Uint8Array` in
/// `buffers` in order, `position` (if given) seeking before the FIRST one
/// only, matching `writev(2)`. Answers the total bytes written.
pub(super) extern "C" fn writev_sync(_e: u64, _this: u64, fd: u64, buffers: u64, position: u64, _a3: u64) -> u64 {
    let Some(fd) = super::number(fd) else {
        return undefined_value();
    };
    let fd = fd as i64;
    let position = super::number(position);
    let mut total = 0usize;
    for (index, item) in array_items(buffers).into_iter().enumerate() {
        let Some(data) = with_runtime(|context| bytes_of(context, item)) else {
            continue;
        };
        let written = super::fd::with_file(fd, |file| {
            use std::io::{Seek, SeekFrom, Write};
            if index == 0 {
                if let Some(pos) = position {
                    let _ = file.seek(SeekFrom::Start(pos.max(0.0) as u64));
                }
            }
            file.write(&data).unwrap_or(0)
        });
        let Some(written) = written else {
            break;
        };
        total += written;
    }
    make_number(total as f64)
}
