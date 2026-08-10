//! Objects, properties, arrays, and reading a string into the addon's buffer.

mod common;

use common::{in_a_program, string};
use rts_napi_rwk::{Env, env, handles, napi_status, napi_valuetype, objects, values};

#[test]
fn a_property_set_by_key_is_found_by_name_and_the_other_way_round() {
    // The claim `objects.rs` makes in its module doc: named, keyed and
    // indexed are three doors to one room. If they were three
    // implementations, this is the test that would catch it.
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { objects::napi_create_object(env, &mut object) };

        // SAFETY: live env.
        let key = unsafe { string(env, c"answer") };
        let mut value = handles::none();
        // SAFETY: same.
        unsafe { values::napi_create_double(env, 42.0, &mut value) };
        // SAFETY: handles from the open scope.
        assert_eq!(
            unsafe { objects::napi_set_property(env, object, key, value) },
            napi_status::napi_ok
        );

        let mut read = handles::none();
        // SAFETY: a NUL-terminated literal.
        let status = unsafe {
            objects::napi_get_named_property(env, object, c"answer".as_ptr(), &mut read)
        };
        assert_eq!(status, napi_status::napi_ok);
        let mut number = 0.0;
        // SAFETY: handles from the open scope.
        unsafe { values::napi_get_value_double(env, read, &mut number) };
        assert_eq!(number, 42.0, "set by key, read by name");
        // SAFETY: from `into_raw`, dropped once.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn a_missing_property_is_ok_and_undefined_not_a_failure() {
    // The ABI's answer and the language's are the same one, and an addon
    // that treated `napi_ok` as "it was there" would be wrong in both.
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(env, &mut object) };

        let mut read = handles::none();
        // SAFETY: a NUL-terminated literal.
        let status = unsafe {
            objects::napi_get_named_property(env, object, c"absent".as_ptr(), &mut read)
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut kind = napi_valuetype::napi_object;
        // SAFETY: a handle from the open scope.
        unsafe { values::napi_typeof(env, read, &mut kind) };
        assert_eq!(kind, napi_valuetype::napi_undefined);

        let mut present = true;
        // SAFETY: live env, handles from the open scope.
        let key = unsafe { string(env, c"absent") };
        // SAFETY: same.
        unsafe { objects::napi_has_property(env, object, key, &mut present) };
        assert!(
            !present,
            "`has` is how an addon tells absent from present-and-undefined"
        );
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn an_array_answers_its_length_and_its_elements() {
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut array = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_array_with_length(env, 3, &mut array) };

        let mut is_array = false;
        // SAFETY: a handle from the open scope.
        unsafe { objects::napi_is_array(env, array, &mut is_array) };
        assert!(is_array);

        let mut length = 0u32;
        // SAFETY: same.
        unsafe { objects::napi_get_array_length(env, array, &mut length) };
        assert_eq!(length, 3, "the length the ABI was asked for");

        let mut value = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(env, 7.0, &mut value) };
        // SAFETY: handles from the open scope.
        unsafe { objects::napi_set_element(env, array, 1, value) };

        let mut read = handles::none();
        // SAFETY: same.
        unsafe { objects::napi_get_element(env, array, 1, &mut read) };
        let mut number = 0.0;
        // SAFETY: same.
        unsafe { values::napi_get_value_double(env, read, &mut number) };
        assert_eq!(number, 7.0);
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn reading_a_string_measures_first_and_never_splits_a_character() {
    // Both halves of `napi_get_value_string_utf8`'s contract, and the second
    // is the one that bites: truncating "é" in the middle hands the addon
    // half a code point, which its own C library renders as garbage.
    in_a_program(|| {
        let env = Env::new().into_raw();
        // SAFETY: live env.
        let handle = unsafe { string(env, c"héllo") };

        let mut needed = 0usize;
        // SAFETY: null buffer is the ABI's "measure it" form.
        let status = unsafe {
            objects::napi_get_value_string_utf8(
                env,
                handle,
                core::ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(needed, 6, "five characters, six bytes — é is two");

        // Room for `h`, then one byte of `é` — which must NOT be written.
        let mut buffer = [0i8; 3];
        let mut written = 0usize;
        // SAFETY: three writable bytes, and `bufsize` says three.
        let status = unsafe {
            objects::napi_get_value_string_utf8(
                env,
                handle,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut written,
            )
        };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(written, 1, "stopped at the character boundary, not at 2");
        assert_eq!(buffer[0], b'h' as i8);
        assert_eq!(buffer[1], 0, "and terminated");
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn property_names_are_the_object_s_own_keys() {
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(env, &mut object) };
        let mut value = handles::none();
        // SAFETY: same.
        unsafe { values::napi_create_double(env, 1.0, &mut value) };
        // SAFETY: a NUL-terminated literal, handles from the open scope.
        unsafe { objects::napi_set_named_property(env, object, c"a".as_ptr(), value) };
        // SAFETY: same.
        unsafe { objects::napi_set_named_property(env, object, c"b".as_ptr(), value) };

        let mut names = handles::none();
        // SAFETY: live env.
        let status = unsafe { objects::napi_get_property_names(env, object, &mut names) };
        assert_eq!(status, napi_status::napi_ok);
        let mut length = 0u32;
        // SAFETY: a handle from the open scope.
        unsafe { objects::napi_get_array_length(env, names, &mut length) };
        assert_eq!(length, 2);
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

#[test]
fn asking_a_number_for_its_property_names_is_refused() {
    in_a_program(|| {
        let env = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(env, 1.0, &mut number) };
        let mut names = handles::none();
        // SAFETY: a handle from the open scope.
        let status = unsafe { objects::napi_get_property_names(env, number, &mut names) };
        assert_eq!(
            status,
            napi_status::napi_object_expected,
            "the ABI has a status for this and it is not `generic_failure`"
        );
        // SAFETY: as above.
        unsafe { env::destroy(env) };
    });
}

