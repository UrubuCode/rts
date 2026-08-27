//! In-place byte-order swaps for `Buffer` views.

use super::super::buffers::{view_of, window_mut};
use super::super::errors;
use super::super::with_current;
use super::validate;

/// Reverse each fixed-width word in the current Buffer view, in place.
pub(in crate::entry) fn swap(this: u64, width: usize) -> u64 {
    let Some(count) = validate::bytes("buffer", this) else {
        return this;
    };
    if count % width != 0 {
        errors::invalid_buffer_size(width * 8);
        return this;
    }
    with_current(|context| {
        if let Some(view) = view_of(context, this)
            && let Some(bytes) = window_mut(context, &view)
        {
            for word in bytes.chunks_exact_mut(width) {
                word.reverse();
            }
        }
        this
    })
}
