//! A class an addon defines.

mod common;

use core::ffi::c_void;

use common::in_a_program;
use rts_napi::abi::{napi_callback_info, napi_env, napi_value};
use rts_napi::class::{napi_property_attributes, napi_property_descriptor};
use rts_napi::{Env, class, env, functions, handles, napi_status, objects, values};

/// How many times the constructor body ran, on THIS thread.
///
/// Thread-local for the reason `tests/wrap.rs` states: cargo runs these on
/// several threads and two of them construct, so a shared counter reads the
/// other test's work.
thread_local! {
    static CONSTRUCTED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// An empty descriptor, so a test writes only the fields it means.
fn descriptor(name: *const core::ffi::c_char) -> napi_property_descriptor {
    napi_property_descriptor {
        utf8name: name,
        name: napi_value(core::ptr::null_mut()),
        method: None,
        getter: None,
        setter: None,
        value: napi_value(core::ptr::null_mut()),
        attributes: napi_property_attributes::napi_default,
        data: core::ptr::null_mut(),
    }
}

/// The class body: records that it ran and answers its own `this`.
///
/// # Safety
///
/// Called by the engine.
unsafe extern "C" fn construct(env: napi_env, info: napi_callback_info) -> napi_value {
    CONSTRUCTED.set(CONSTRUCTED.get() + 1);
    let mut this = handles::none();
    let mut argc = 0usize;
    // SAFETY: locals, and `argc` says none are wanted.
    unsafe {
        functions::napi_get_cb_info(
            env,
            info,
            &mut argc,
            core::ptr::null_mut(),
            &mut this,
            core::ptr::null_mut(),
        )
    };
    this
}

/// A method that answers 7.
///
/// # Safety
///
/// Called by the engine.
unsafe extern "C" fn answer_seven(env: napi_env, _info: napi_callback_info) -> napi_value {
    let mut answer = handles::none();
    // SAFETY: live env, local out-parameter.
    unsafe { values::napi_create_double(env, 7.0, &mut answer) };
    answer
}

#[test]
fn a_defined_class_constructs_and_its_method_is_on_the_prototype() {
    in_a_program(|| {
        CONSTRUCTED.set(0);
        let raw = Env::new().into_raw();

        let mut method = descriptor(c"seven".as_ptr());
        method.method = Some(answer_seven);
        let properties = [method];

        let mut class_value = handles::none();
        // SAFETY: live env, one descriptor, NUL-terminated name.
        let status = unsafe {
            class::napi_define_class(
                raw,
                c"Thing".as_ptr(),
                usize::MAX,
                Some(construct),
                core::ptr::null_mut(),
                properties.len(),
                properties.as_ptr(),
                &mut class_value,
            )
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut instance = handles::none();
        // SAFETY: a handle from the open scope, no arguments.
        let status = unsafe {
            class::napi_new_instance(raw, class_value, 0, core::ptr::null(), &mut instance)
        };
        assert_eq!(status, napi_status::napi_ok);
        assert_eq!(CONSTRUCTED.get(), 1, "the body ran");

        // The method is found through the prototype, which is what makes it a
        // class rather than an object with a function on it.
        let mut found = handles::none();
        // SAFETY: a handle from the open scope, NUL-terminated literal.
        unsafe { objects::napi_get_named_property(raw, instance, c"seven".as_ptr(), &mut found) };
        let mut callable = false;
        // SAFETY: same.
        unsafe { functions::napi_is_callable(raw, found, &mut callable) };
        assert!(callable, "inherited from the prototype");

        let mut answer = handles::none();
        // SAFETY: same.
        unsafe {
            functions::napi_call_function(raw, instance, found, 0, core::ptr::null(), &mut answer)
        };
        let mut number = 0.0;
        // SAFETY: same.
        unsafe { values::napi_get_value_double(raw, answer, &mut number) };
        assert_eq!(number, 7.0);

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn an_instance_is_one_by_instanceof() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut class_value = handles::none();
        // SAFETY: live env, no descriptors.
        unsafe {
            class::napi_define_class(
                raw,
                c"Thing".as_ptr(),
                usize::MAX,
                Some(construct),
                core::ptr::null_mut(),
                0,
                core::ptr::null(),
                &mut class_value,
            )
        };
        let mut instance = handles::none();
        // SAFETY: a handle from the open scope.
        unsafe {
            class::napi_new_instance(raw, class_value, 0, core::ptr::null(), &mut instance)
        };

        let mut is_instance = false;
        // SAFETY: handles from the open scope.
        let status =
            unsafe { class::napi_instanceof(raw, instance, class_value, &mut is_instance) };
        assert_eq!(status, napi_status::napi_ok);
        assert!(is_instance, "the prototype link is what this reads");

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_static_member_lands_on_the_constructor_and_not_on_the_instance() {
    // The one attribute that IS honoured, and the reason it is: it changes
    // where the member lives, which the other three do not.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut on_class = descriptor(c"make".as_ptr());
        on_class.method = Some(answer_seven);
        on_class.attributes = napi_property_attributes::napi_static;
        let properties = [on_class];

        let mut class_value = handles::none();
        // SAFETY: live env, one descriptor.
        unsafe {
            class::napi_define_class(
                raw,
                c"Thing".as_ptr(),
                usize::MAX,
                Some(construct),
                core::ptr::null_mut(),
                properties.len(),
                properties.as_ptr(),
                &mut class_value,
            )
        };

        let mut found = handles::none();
        // SAFETY: a handle from the open scope, NUL-terminated literal.
        unsafe { objects::napi_get_named_property(raw, class_value, c"make".as_ptr(), &mut found) };
        let mut callable = false;
        // SAFETY: same.
        unsafe { functions::napi_is_callable(raw, found, &mut callable) };
        assert!(callable, "on the constructor");

        let mut instance = handles::none();
        // SAFETY: same.
        unsafe {
            class::napi_new_instance(raw, class_value, 0, core::ptr::null(), &mut instance)
        };
        let mut absent = handles::none();
        // SAFETY: same.
        unsafe { objects::napi_get_named_property(raw, instance, c"make".as_ptr(), &mut absent) };
        let mut kind = rts_napi::napi_valuetype::napi_object;
        // SAFETY: same.
        unsafe { values::napi_typeof(raw, absent, &mut kind) };
        assert_eq!(
            kind,
            rts_napi::napi_valuetype::napi_undefined,
            "and not on the instance"
        );

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_getter_descriptor_runs_on_read() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut object) };

        let mut accessor = descriptor(c"seven".as_ptr());
        accessor.getter = Some(answer_seven);
        let properties = [accessor];
        // SAFETY: a handle from the open scope, one descriptor.
        let status = unsafe {
            class::napi_define_properties(raw, object, properties.len(), properties.as_ptr())
        };
        assert_eq!(status, napi_status::napi_ok);

        let mut read = handles::none();
        // SAFETY: same.
        unsafe { objects::napi_get_named_property(raw, object, c"seven".as_ptr(), &mut read) };
        let mut number = 0.0;
        // SAFETY: same.
        unsafe { values::napi_get_value_double(raw, read, &mut number) };
        assert_eq!(number, 7.0, "reading called the getter");

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn a_value_descriptor_is_put_as_a_plain_property() {
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut object) };
        let mut value = handles::none();
        // SAFETY: same.
        unsafe { values::napi_create_double(raw, 3.0, &mut value) };

        let mut plain = descriptor(c"three".as_ptr());
        plain.value = value;
        let properties = [plain];
        // SAFETY: handles from the open scope.
        unsafe {
            class::napi_define_properties(raw, object, properties.len(), properties.as_ptr())
        };

        let mut read = handles::none();
        // SAFETY: same.
        unsafe { objects::napi_get_named_property(raw, object, c"three".as_ptr(), &mut read) };
        let mut number = 0.0;
        // SAFETY: same.
        unsafe { values::napi_get_value_double(raw, read, &mut number) };
        assert_eq!(number, 3.0);

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

#[test]
fn more_arguments_than_the_convention_carries_is_refused() {
    // Truncating would let a constructor read its fifth argument as
    // `undefined` and build a wrong object in silence.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut class_value = handles::none();
        // SAFETY: live env, no descriptors.
        unsafe {
            class::napi_define_class(
                raw,
                c"Thing".as_ptr(),
                usize::MAX,
                Some(construct),
                core::ptr::null_mut(),
                0,
                core::ptr::null(),
                &mut class_value,
            )
        };
        let argv = [handles::none(); 5];
        let mut instance = handles::none();
        // SAFETY: five handles, which is one more than the convention carries.
        let status = unsafe {
            class::napi_new_instance(raw, class_value, 5, argv.as_ptr(), &mut instance)
        };
        assert_eq!(status, napi_status::napi_invalid_arg);
        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}

/// Unused by the tests above, but the type must stay `Send`-free and this keeps
/// the import honest.
const _: Option<*mut c_void> = None;

#[test]
fn a_getter_answers_the_same_through_the_named_door_and_the_keyed_one() {
    // The regression guard for a claim `objects.rs` makes and once broke.
    // `napi_get_named_property` used `get_member`, which reads a data property
    // and cannot run user code — so a getter answered `undefined` by name and
    // 7 by key. Three doors, two rooms.
    in_a_program(|| {
        let raw = Env::new().into_raw();
        let mut object = handles::none();
        // SAFETY: live env.
        unsafe { objects::napi_create_object(raw, &mut object) };

        let mut accessor = descriptor(c"seven".as_ptr());
        accessor.getter = Some(answer_seven);
        let properties = [accessor];
        // SAFETY: a handle from the open scope, one descriptor.
        unsafe {
            class::napi_define_properties(raw, object, properties.len(), properties.as_ptr())
        };

        let mut by_name = handles::none();
        // SAFETY: NUL-terminated literal.
        unsafe { objects::napi_get_named_property(raw, object, c"seven".as_ptr(), &mut by_name) };

        let mut key = handles::none();
        // SAFETY: live env.
        unsafe {
            values::napi_create_string_utf8(raw, c"seven".as_ptr(), usize::MAX, &mut key)
        };
        let mut by_key = handles::none();
        // SAFETY: handles from the open scope.
        unsafe { objects::napi_get_property(raw, object, key, &mut by_key) };

        let (mut named, mut keyed) = (0.0, 0.0);
        // SAFETY: handles from the open scope.
        unsafe {
            values::napi_get_value_double(raw, by_name, &mut named);
            values::napi_get_value_double(raw, by_key, &mut keyed);
        }
        assert_eq!(named, 7.0, "by name");
        assert_eq!(keyed, 7.0, "and by key");

        // SAFETY: from `into_raw`, destroyed once.
        unsafe { env::destroy(raw) };
    });
}
