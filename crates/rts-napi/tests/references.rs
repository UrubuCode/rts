//! References: what an addon keeps after the call that made it.

mod common;

use common::in_a_program;
use rts_napi::abi::napi_ref;
use rts_napi::{Env, env, handles, napi_status, references, values};

#[test]
fn a_strong_reference_answers_its_value_after_the_scope_that_made_it_closed() {
    // The point of a reference. The handle belongs to a scope; the reference
    // does not, and reading it once that scope is gone is the whole use case.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut reference = napi_ref(core::ptr::null_mut());
        {
            // SAFETY: the pointer came from `into_raw` and is live.
            let scoped = unsafe { handles::env_of(raw) }.expect("a live env");
            scoped.open();
        }
        let mut handle = handles::none();
        // SAFETY: live env, local out-parameter.
        unsafe { values::napi_create_double(raw, 3.25, &mut handle) };
        // SAFETY: a handle from the open scope.
        let status = unsafe { references::napi_create_reference(raw, handle, 1, &mut reference) };
        assert_eq!(status, napi_status::napi_ok);
        {
            // SAFETY: as above.
            let scoped = unsafe { handles::env_of(raw) }.expect("a live env");
            assert!(scoped.close(), "the scope the handle belonged to");
        }

        let mut read = handles::none();
        // SAFETY: a reference this test made.
        let status = unsafe { references::napi_get_reference_value(raw, reference, &mut read) };
        assert_eq!(status, napi_status::napi_ok);
        let mut number = 0.0;
        // SAFETY: a handle from the surviving scope.
        unsafe { values::napi_get_value_double(raw, read, &mut number) };
        assert_eq!(number, 3.25);

        // SAFETY: made here, deleted once.
        unsafe { references::napi_delete_reference(raw, reference) };
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn unreffing_to_zero_stops_holding_and_reffing_back_up_holds_again() {
    // The transition this phase exists for. Observed through the engine's own
    // bookkeeping rather than by watching a collection, because what is being
    // pinned is WHICH mechanism the reference is using, not whether the
    // collector works — `rts-core`'s own tests pin that.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut handle = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_string_utf8(raw, c"kept".as_ptr(), usize::MAX, &mut handle) };
        let mut reference = napi_ref(core::ptr::null_mut());
        // SAFETY: a handle from the open scope.
        unsafe { references::napi_create_reference(raw, handle, 1, &mut reference) };

        let mut count = 9u32;
        // SAFETY: a reference this test made.
        unsafe { references::napi_reference_unref(raw, reference, &mut count) };
        assert_eq!(count, 0);

        // Still readable: nothing has collected it, and a weak reference reads
        // its value right up until something does.
        let mut read = handles::none();
        // SAFETY: as above.
        unsafe { references::napi_get_reference_value(raw, reference, &mut read) };
        assert_eq!(
            rts_core::entry::text_of(
                // SAFETY: a handle from the open scope.
                unsafe { handles::value_of(read) }.expect("a slot")
            )
            .as_deref(),
            Some("kept")
        );

        // SAFETY: as above.
        unsafe { references::napi_reference_ref(raw, reference, &mut count) };
        assert_eq!(count, 1, "and it holds again");

        // SAFETY: made here, deleted once.
        unsafe { references::napi_delete_reference(raw, reference) };
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn unreffing_below_zero_is_refused_rather_than_saturated() {
    // Saturating would silently balance a `ref` the addon never made, and the
    // next `unref` would drop a hold that belongs to someone else's pairing.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut handle = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 1.0, &mut handle) };
        let mut reference = napi_ref(core::ptr::null_mut());
        // SAFETY: a handle from the open scope.
        unsafe { references::napi_create_reference(raw, handle, 0, &mut reference) };

        let mut count = 9u32;
        // SAFETY: a reference this test made.
        let status = unsafe { references::napi_reference_unref(raw, reference, &mut count) };
        assert_eq!(status, napi_status::napi_generic_failure);

        // SAFETY: made here, deleted once.
        unsafe { references::napi_delete_reference(raw, reference) };
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn deleting_a_reference_twice_is_refused_rather_than_freeing_twice() {
    // The second delete arrives as a pointer this module no longer boxed, and
    // calling `Box::from_raw` on it is the classic double free. The live list
    // is what makes that answerable at all.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut handle = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 1.0, &mut handle) };
        let mut reference = napi_ref(core::ptr::null_mut());
        // SAFETY: a handle from the open scope.
        unsafe { references::napi_create_reference(raw, handle, 1, &mut reference) };

        // SAFETY: made here.
        assert_eq!(
            unsafe { references::napi_delete_reference(raw, reference) },
            napi_status::napi_ok
        );
        // SAFETY: deliberately the same pointer again, which is what this pins.
        assert_eq!(
            unsafe { references::napi_delete_reference(raw, reference) },
            napi_status::napi_invalid_arg
        );
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn destroying_the_environment_frees_the_references_it_made() {
    // An addon that unloads without calling `napi_delete_reference` is the
    // common case. If `env::destroy` did not sweep them, every one of them
    // would keep its value alive for the life of the process.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut handle = handles::none();
        // SAFETY: live env.
        unsafe { values::napi_create_double(raw, 1.0, &mut handle) };
        let mut reference = napi_ref(core::ptr::null_mut());
        // SAFETY: a handle from the open scope.
        unsafe { references::napi_create_reference(raw, handle, 1, &mut reference) };

        // SAFETY: from `into_raw`, destroyed once — and the reference goes with
        // it, which is what would leak.
        unsafe { env::destroy(raw) };
    });
}
