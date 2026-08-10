//! Bytes an addon reads and writes in place.

mod common;

use core::ffi::c_void;

use common::in_a_program;
use rts_napi_rwk::{Env, buffers, env, handles, napi_status, values};

#[test]
fn a_buffer_is_written_through_the_pointer_and_the_program_sees_it() {
    // The whole reason `bytes_pointer` exists. If this handed back a copy —
    // which is what the safe accessor answers — the writes below would land in
    // a temporary and the value would still be zeroes.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut data: *mut c_void = core::ptr::null_mut();
        let mut buffer = handles::none();
        // SAFETY: live env, local out-parameters.
        let status = unsafe { buffers::napi_create_buffer(raw, 4, &mut data, &mut buffer) };
        assert_eq!(status, napi_status::napi_ok);
        assert!(!data.is_null());

        // SAFETY: four bytes the call just handed over.
        unsafe {
            let bytes = core::slice::from_raw_parts_mut(data.cast::<u8>(), 4);
            bytes.copy_from_slice(&[1, 2, 3, 4]);
        }

        // Read back through the ENGINE, not through the pointer: the question
        // is whether the program's value changed.
        // SAFETY: a handle from the open scope.
        let word = unsafe { handles::value_of(buffer) }.expect("a slot");
        let seen = rts_core::entry::with_runtime(|context| {
            rts_core::entry::bytes_of(context, word)
        });
        assert_eq!(seen.as_deref(), Some(&[1u8, 2, 3, 4][..]));

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_copy_starts_with_the_addon_s_bytes() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let source = [9u8, 8, 7];
        let mut data: *mut c_void = core::ptr::null_mut();
        let mut buffer = handles::none();
        // SAFETY: three readable bytes.
        let status = unsafe {
            buffers::napi_create_buffer_copy(
                raw,
                source.len(),
                source.as_ptr().cast(),
                &mut data,
                &mut buffer,
            )
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut length = 0usize;
        let mut read: *mut c_void = core::ptr::null_mut();
        // SAFETY: a handle from the open scope.
        unsafe { buffers::napi_get_buffer_info(raw, buffer, &mut read, &mut length) };
        assert_eq!(length, 3);
        // SAFETY: three bytes the call reported.
        let seen = unsafe { core::slice::from_raw_parts(read.cast::<u8>(), length) };
        assert_eq!(seen, &source[..]);

        // A COPY: writing through the addon's original must not change it.
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_buffer_is_a_buffer_and_a_number_is_not() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut buffer = handles::none();
        // SAFETY: live env.
        unsafe {
            buffers::napi_create_buffer(raw, 1, core::ptr::null_mut(), &mut buffer)
        };

        let mut is_buffer = false;
        // SAFETY: a handle from the open scope.
        unsafe { buffers::napi_is_buffer(raw, buffer, &mut is_buffer) };
        assert!(is_buffer);

        let mut number = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 1.0, &mut number) };
        // SAFETY: a handle from the open scope.
        unsafe { buffers::napi_is_buffer(raw, number, &mut is_buffer) };
        assert!(!is_buffer);

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn asking_for_the_element_type_is_refused_rather_than_guessed() {
    // The engine does not export a view's element type, and answering
    // `uint8_array` for everything would make an addon read a `Float64Array`
    // eight times too many elements from. A status is the honest answer.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut buffer = handles::none();
        // SAFETY: live env.
        unsafe {
            buffers::napi_create_buffer(raw, 8, core::ptr::null_mut(), &mut buffer)
        };

        let mut length = 0usize;
        // SAFETY: a handle from the open scope, only the length wanted.
        let status = unsafe {
            buffers::napi_get_typedarray_info(
                raw,
                buffer,
                core::ptr::null_mut(),
                &mut length,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(length, 8, "the part it can answer, it answers");

        let mut kind = 0i32;
        // SAFETY: same, but now asking for the element type.
        let status = unsafe {
            buffers::napi_get_typedarray_info(
                raw,
                buffer,
                &mut kind,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(
            status,
            napi_status::napi_generic_failure,
            "and refuses the part it cannot"
        );

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_pointer_survives_another_buffer_being_allocated() {
    // The contract `bytes_pointer` states: each buffer's bytes are their own
    // allocation, so making another one moves table headers and not the bytes.
    // An addon that filled a buffer, allocated a second, and found the first
    // full of somebody else's data would have no way to diagnose it.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut first_data: *mut c_void = core::ptr::null_mut();
        let mut first = handles::none();
        // SAFETY: live env.
        unsafe { buffers::napi_create_buffer(raw, 2, &mut first_data, &mut first) };
        // SAFETY: two bytes just handed over.
        unsafe {
            core::slice::from_raw_parts_mut(first_data.cast::<u8>(), 2).copy_from_slice(&[5, 6])
        };

        for _ in 0..64 {
            let mut other = handles::none();
            // SAFETY: live env.
            unsafe {
                buffers::napi_create_buffer(raw, 32, core::ptr::null_mut(), &mut other)
            };
        }

        // SAFETY: the same pointer, after sixty-four allocations.
        let seen = unsafe { core::slice::from_raw_parts(first_data.cast::<u8>(), 2) };
        assert_eq!(seen, &[5u8, 6][..]);

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}
