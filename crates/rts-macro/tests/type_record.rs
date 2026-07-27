//! `#[rtse::type]` — a plain Rust struct that marshals to a PLAIN JS OBJECT (a
//! record: `typeof` `"object"`, own fields, no class identity) when returned to
//! JS. Exercises the real macro expansion + Registry (not a hand-rolled
//! `Member`), mirroring `class_param.rs`'s pattern.

use rts_engine::Engine;
use rts_engine::heap::handles::RtseReturn;
use rts_engine::heap::shapes::global_shape_keys;

mod stats {
    #[rtse::r#type]
    pub struct Stats {
        pub size: f64,
        pub mtime: f64,
        pub is_file: bool,
    }

    #[rtse::r#type]
    pub struct Wrapper {
        pub label: String,
        pub inner: Stats,
    }

    #[rtse::class("Fs")]
    pub struct Fs;

    #[rtse::class("Fs")]
    impl Fs {
        #[rtse::statical]
        fn stat(_path: &str) -> Stats {
            Stats {
                size: 42.0,
                mtime: 1000.0,
                is_file: true,
            }
        }

        #[rtse::statical]
        fn wrap(_path: &str) -> Wrapper {
            Wrapper {
                label: "w".to_string(),
                inner: Stats {
                    size: 1.0,
                    mtime: 2.0,
                    is_file: false,
                },
            }
        }
    }
}
use stats::{Fs, Stats};

fn register(e: &mut Engine) {
    stats::register(e);
}

/// A `#[rtse::type]` struct is NOT a class: no `RTSE_CLASS` identity (empty
/// sentinel), no ctor, no Registry class entry.
#[test]
fn record_has_no_class_identity() {
    assert_eq!(Stats::RTSE_CLASS, "");
}

/// The shape's key order matches the struct's declared field order (with
/// `is_file` → `isFile` camelCase), and the shape is a REAL interned
/// global shape (not a fabricated id) — confirming slot 0 through
/// `global_shape_keys` gives back exactly the JS property list `Object.keys`
/// would enumerate, in the SAME order the values were pushed.
#[test]
fn shape_key_order_pins_field_order() {
    let h = Stats {
        size: 1.0,
        mtime: 2.0,
        is_file: true,
    }
    .__rtse_into_handle();
    let raw = rts_engine::heap::handles::with_entry(h, |e| match e {
        Some(rts_engine::heap::handles::Entry::Vec(v)) => v.clone(),
        _ => panic!("expected a shaped Entry::Vec"),
    });
    // slot 0 = boxed shape id; decode it back to a GlobalShapeId.
    let shape_word = raw[0] as u64;
    let shape_id = (shape_word & 0x0000_FFFF_FFFF_FFFF) as u32;
    let keys = global_shape_keys(shape_id).expect("shape interned");
    assert_eq!(keys, vec!["size", "mtime", "isFile"]);
    assert_eq!(raw.len(), keys.len() + 1);
}

/// A mixed-field-kind record returned from a `#[rtse::statical]` member reads
/// back through the REAL Registry `Member` + generated `extern "C"` fn (not a
/// hand-built object) — same shape/slot layout an object-literal `{ size,
/// mtime, isFile }` from the lowering would produce.
#[test]
fn static_returns_record_with_correct_fields() {
    let mut e = Engine::new();
    register(&mut e);
    let class = e.registry().class("Fs").expect("Fs registered");
    let m = class.members.iter().find(|m| m.name == "stat").expect("stat present");

    let path = "x";
    let f: extern "C" fn(*const u8, i64) -> u64 = unsafe { std::mem::transmute(m.fn_ptr) };
    let h = f(path.as_ptr(), path.len() as i64);

    let raw = rts_engine::heap::handles::with_entry(h, |e| match e {
        Some(rts_engine::heap::handles::Entry::Vec(v)) => v.clone(),
        _ => panic!("expected a shaped Entry::Vec"),
    });
    let shape_id = (raw[0] as u64 & 0x0000_FFFF_FFFF_FFFF) as u32;
    let keys = global_shape_keys(shape_id).unwrap();
    assert_eq!(keys, vec!["size", "mtime", "isFile"]);

    // size: 42.0 rides as an inline f64 word (its own bit pattern).
    assert_eq!(f64::from_bits(raw[1] as u64), 42.0);
    // mtime: 1000.0
    assert_eq!(f64::from_bits(raw[2] as u64), 1000.0);
    // isFile: true -> the TRUE singleton word.
    assert_eq!(raw[3] as u64, rts_engine::heap::poly::POLY_TRUE);
}

/// A nested `#[rtse::type]` field marshals to a NESTED shaped object (its own
/// shape, own slots), boxed as an OBJECT-tagged word in the outer record's slot.
#[test]
fn nested_record_field_is_a_nested_object() {
    let mut e = Engine::new();
    register(&mut e);
    let class = e.registry().class("Fs").unwrap();
    let m = class.members.iter().find(|m| m.name == "wrap").unwrap();
    let path = "x";
    let f: extern "C" fn(*const u8, i64) -> u64 = unsafe { std::mem::transmute(m.fn_ptr) };
    let outer_h = f(path.as_ptr(), path.len() as i64);

    let outer = rts_engine::heap::handles::with_entry(outer_h, |e| match e {
        Some(rts_engine::heap::handles::Entry::Vec(v)) => v.clone(),
        _ => panic!("expected outer shaped Entry::Vec"),
    });
    let outer_shape = (outer[0] as u64 & 0x0000_FFFF_FFFF_FFFF) as u32;
    assert_eq!(global_shape_keys(outer_shape).unwrap(), vec!["label", "inner"]);

    // slot 2 ("inner") is a boxed OBJECT word — decode its handle and confirm
    // it is itself a valid shaped object with Stats's own field order.
    let inner_word = outer[2] as u64;
    let inner_handle = rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(
        inner_word & 0x0000_FFFF_FFFF_FFFF,
    );
    let inner = rts_engine::heap::handles::with_entry(inner_handle, |e| match e {
        Some(rts_engine::heap::handles::Entry::Vec(v)) => v.clone(),
        _ => panic!("expected inner shaped Entry::Vec"),
    });
    let inner_shape = (inner[0] as u64 & 0x0000_FFFF_FFFF_FFFF) as u32;
    assert_eq!(
        global_shape_keys(inner_shape).unwrap(),
        vec!["size", "mtime", "isFile"]
    );
}
