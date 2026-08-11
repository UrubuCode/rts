//! The 128-bit mark that says "this object is mine".

mod common;

use common::in_a_program;
use rts_napi_rwk::tags::napi_type_tag;
use rts_napi_rwk::{Env, handles, napi_status, objects, tags, values};

/// One addon's UUID.
const MINE: napi_type_tag = napi_type_tag {
    lower: 0x0123_4567_89ab_cdef,
    upper: 0xfedc_ba98_7654_3210,
};

/// Another's, differing in one bit — the case a sloppy comparison passes.
const THEIRS: napi_type_tag = napi_type_tag {
    lower: 0x0123_4567_89ab_cdef,
    upper: 0xfedc_ba98_7654_3211,
};

#[test]
fn a_tagged_object_answers_to_its_own_tag_and_not_another() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { objects::napi_create_object(raw, &mut object) };

        // SAFETY: a handle from the open scope, a local tag.
        let status = unsafe { tags::napi_type_tag_object(raw, object, &MINE) };
        assert_eq!(status, napi_status::napi_ok);

        let mut matches = false;
        // SAFETY: a handle from the open scope, local tag and out-parameter.
        unsafe { tags::napi_check_object_type_tag(raw, object, &MINE, &mut matches) };
        assert!(matches);

        // The one-bit difference is the whole point: an addon that trusts this
        // before an unwrap is trusting it not to answer for a neighbour's UUID.
        let mut wrong = true;
        // SAFETY: a handle from the open scope, local tag and out-parameter.
        unsafe { tags::napi_check_object_type_tag(raw, object, &THEIRS, &mut wrong) };
        assert!(!wrong);
    });
}

#[test]
fn an_untagged_object_matches_nothing() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { objects::napi_create_object(raw, &mut object) };

        let mut matches = true;
        // SAFETY: a handle from the open scope, local tag and out-parameter.
        let status =
            unsafe { tags::napi_check_object_type_tag(raw, object, &MINE, &mut matches) };
        assert_eq!(status, napi_status::napi_ok);
        assert!(!matches);
    });
}

#[test]
fn tagging_twice_is_refused_and_the_first_tag_stands() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { objects::napi_create_object(raw, &mut object) };
        // SAFETY: a handle from the open scope, a local tag.
        unsafe { tags::napi_type_tag_object(raw, object, &MINE) };

        // SAFETY: a handle from the open scope, a local tag.
        let status = unsafe { tags::napi_type_tag_object(raw, object, &THEIRS) };
        assert_eq!(status, napi_status::napi_invalid_arg);

        // The refusal has to leave the object as it was. A second tag that
        // half-landed would make the check answer for whichever the table met
        // first, which is the ownership confusion the tag exists to prevent.
        let mut matches = false;
        // SAFETY: a handle from the open scope, local tag and out-parameter.
        unsafe { tags::napi_check_object_type_tag(raw, object, &MINE, &mut matches) };
        assert!(matches);
    });
}

#[test]
fn two_objects_carry_their_own_tags() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut first = handles::none();
        let mut second = handles::none();
        // SAFETY: live env, local out-parameters.
        unsafe {
            objects::napi_create_object(raw, &mut first);
            objects::napi_create_object(raw, &mut second);
        }
        // SAFETY: handles from the open scope, local tags.
        unsafe {
            tags::napi_type_tag_object(raw, first, &MINE);
            tags::napi_type_tag_object(raw, second, &THEIRS);
        }

        let mut answer = false;
        // SAFETY: handles from the open scope, local tags and out-parameter.
        unsafe { tags::napi_check_object_type_tag(raw, second, &THEIRS, &mut answer) };
        assert!(answer);
        // SAFETY: handles from the open scope, local tags and out-parameter.
        unsafe { tags::napi_check_object_type_tag(raw, second, &MINE, &mut answer) };
        assert!(!answer);
    });
}

#[test]
fn a_number_cannot_be_tagged() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut number = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { values::napi_create_double(raw, 3.0, &mut number) };

        // SAFETY: a handle from the open scope, a local tag.
        let status = unsafe { tags::napi_type_tag_object(raw, number, &MINE) };
        assert_eq!(status, napi_status::napi_object_expected);
    });
}
