//! Class-typed PARAMETER support (the mirror of the `-> Self` / `-> OtherClass`
//! return work): `#[rtse::statical]`/`#[rtse::method]` accepting `&T`/`&mut T`/`T`
//! where `T` is another `#[rtse::class]` struct. Exercises the real macro
//! expansion + Registry (not a hand-rolled `Member`).

use rts_engine::heap::handles::{alloc_rtse, with_rtse};
use rts_engine::Engine;

// Each `#[rtse::class]` impl generates a module-scoped `pub fn register` (the
// same one-class-per-module shape `rts-shared/src/globals/<class>/mod.rs`
// uses) — a own `mod` per class avoids a `register` name collision when two
// classes live in one test file.
mod pt_a {
    #[rtse::class("PtA")]
    #[derive(Clone)]
    pub struct PtA {
        pub x: f64,
    }

    #[rtse::class("PtA")]
    impl PtA {
        #[rtse::ctor]
        fn new(x: f64) -> Self {
            PtA { x }
        }

        // Static taking TWO class refs (no receiver).
        #[rtse::statical]
        fn distance(a: &PtA, b: &PtA) -> f64 {
            (a.x - b.x).abs()
        }

        // Instance method taking a class ref ALONGSIDE the receiver.
        #[rtse::method]
        fn add(self: &PtA, other: &PtA) -> f64 {
            self.x + other.x
        }

        // `&mut T` non-receiver param: mutate the clone, write it back.
        #[rtse::method]
        fn bump_other(self: &PtA, other: &mut PtA) -> f64 {
            other.x += 1.0;
            self.x + other.x
        }
    }
}
use pt_a::PtA;

mod unrelated {
    #[rtse::class("Unrelated")]
    #[derive(Clone)]
    pub struct Unrelated {
        pub n: i64,
    }

    #[rtse::class("Unrelated")]
    impl Unrelated {
        #[rtse::ctor]
        fn new(n: i64) -> Self {
            Unrelated { n }
        }
    }
}
use unrelated::Unrelated;

fn register(e: &mut Engine) {
    pt_a::register(e);
    unrelated::register(e);
}

#[test]
fn static_takes_two_class_refs() {
    let mut e = Engine::new();
    register(&mut e);
    let ha = alloc_rtse::<PtA>("PtA", PtA { x: 3.0 });
    let hb = alloc_rtse::<PtA>("PtA", PtA { x: 7.0 });

    let class = e.registry().class("PtA").expect("PtA registered");
    let m = class
        .members
        .iter()
        .find(|m| m.name == "distance")
        .expect("distance static present");
    // Static: no receiver slot, two Handle args.
    assert_eq!(m.sig.args.len(), 2);

    let f: extern "C" fn(u64, u64) -> f64 = unsafe { std::mem::transmute(m.fn_ptr) };
    assert_eq!(f(ha, hb), 4.0);
}

#[test]
fn method_takes_class_ref_alongside_receiver() {
    let mut e = Engine::new();
    register(&mut e);
    let ha = alloc_rtse::<PtA>("PtA", PtA { x: 3.0 });
    let hb = alloc_rtse::<PtA>("PtA", PtA { x: 7.0 });

    let class = e.registry().class("PtA").expect("PtA registered");
    let m = class
        .members
        .iter()
        .find(|m| m.name == "add")
        .expect("add method present");
    // Instance method: receiver + one class-ref arg.
    assert_eq!(m.sig.args.len(), 2);

    let f: extern "C" fn(u64, u64) -> f64 = unsafe { std::mem::transmute(m.fn_ptr) };
    assert_eq!(f(ha, hb), 10.0);
}

#[test]
fn mut_class_ref_param_writes_back() {
    let mut e = Engine::new();
    register(&mut e);
    let ha = alloc_rtse::<PtA>("PtA", PtA { x: 1.0 });
    let hb = alloc_rtse::<PtA>("PtA", PtA { x: 5.0 });

    let class = e.registry().class("PtA").unwrap();
    let m = class.members.iter().find(|m| m.name == "bumpOther").unwrap();
    let f: extern "C" fn(u64, u64) -> f64 = unsafe { std::mem::transmute(m.fn_ptr) };
    assert_eq!(f(ha, hb), 7.0); // 1.0 + (5.0 + 1.0)

    // The `&mut` clone-out wrote back: `hb`'s x is now 6.0.
    with_rtse::<PtA, _>(hb, |p| assert_eq!(p.unwrap().x, 6.0));
}

#[test]
fn wrong_class_handle_is_rejected_not_misread() {
    let mut e = Engine::new();
    register(&mut e);
    let ha = alloc_rtse::<PtA>("PtA", PtA { x: 3.0 });
    // A handle of the WRONG Rust type — same handle table, different `T`.
    let wrong = alloc_rtse::<Unrelated>("Unrelated", Unrelated { n: 9 });

    let class = e.registry().class("PtA").unwrap();
    let m = class.members.iter().find(|m| m.name == "distance").unwrap();
    let f: extern "C" fn(u64, u64) -> f64 = unsafe { std::mem::transmute(m.fn_ptr) };
    // `with_rtse::<PtA,_>` downcast fails on `wrong` -> bails to `f64::default()`
    // (`0.0`), never reinterprets `Unrelated`'s bytes as a `PtA`.
    assert_eq!(f(ha, wrong), 0.0);
}
