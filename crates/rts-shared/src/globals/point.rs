//! `Point` — a G1 proof of the `#[rtse::class]` authoring macro. A NORMAL Rust
//! struct + impl; the macro generates the extern-C ABI glue + `register(e)`. The
//! struct is usable as plain Rust (`Point::new(3.0, 4.0).sum()`) AND as a JS class
//! (`new Point(3,4).sum()`). Storage is the generic `Entry::Rtse`.

/// A 2D point. Normal Rust struct. `#[rtse::class]` on the struct exposes the
/// `#[rtse::variable]` fields (getter + setter); the impl adds ctor/methods.
#[rtse::class("RtsePoint")]
#[derive(Clone)]
pub struct Point {
    #[rtse::variable]
    x: f64,
    #[rtse::variable(readonly)]
    y: f64,
}

#[rtse::class("RtsePoint")]
impl Point {
    #[rtse::ctor]
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    #[rtse::method]
    fn sum(self: &Point) -> f64 {
        self.x + self.y
    }

    #[rtse::method]
    fn scaled(self: &Point, k: f64) -> f64 {
        (self.x + self.y) * k
    }

    #[rtse::method(name = "label")]
    fn label(self: &Point) -> String {
        format!("({},{})", self.x, self.y)
    }

    // Static method (`statical`, not the `static` keyword): `Point.unit()`.
    #[rtse::statical]
    fn unit() -> f64 {
        1.0
    }

    // `&str` param → StrPtr marshalling.
    #[rtse::method]
    fn tagged(self: &Point, prefix: &str) -> String {
        format!("{}{}", prefix, self.x + self.y)
    }

    // `&mut self` — mutates the boxed struct in place (with_rtse_mut).
    #[rtse::method]
    fn bump(self: &mut Point) -> f64 {
        self.x += 1.0;
        self.x + self.y
    }

    // F3: a REAL Rust `async fn` — `.await` works in the body and the method is a
    // genuine Future in Rust. The macro drives it with `rts_engine::block_on` and
    // wraps the String in a settled Promise for JS. JS: `await p.labelAsync()`.
    #[rtse::asynch]
    async fn label_async(self: &Point) -> String {
        format!("({},{})", self.x, self.y)
    }

    // F2: overload by ARITY — two Rust fns, same JS name `at`, distinct arities.
    // No macro feature needed: each emits its own Member (unique symbol); the
    // engine's registry dispatch picks by argc (`class_member(class,method,argc)`).
    // `p.at()` → sum; `p.at(k)` → scaled.
    #[rtse::method(name = "at")]
    fn at0(self: &Point) -> f64 {
        self.x + self.y
    }

    #[rtse::method(name = "at")]
    fn at1(self: &Point, k: f64) -> f64 {
        (self.x + self.y) * k
    }

    // F4: optional trailing param. `optional = 1` → the LAST param defaults to
    // `undefined`; the engine admits `p.scaledOr1()` (arity window [0,1]) and
    // injects the undefined sentinel, which arrives as NaN — the body reads that
    // as its own default (1.0). JS: `p.scaledOr1()`===sum, `p.scaledOr1(2)`===2*sum.
    #[rtse::method(name = "scaledOr1", optional = 1)]
    fn scaled_or1(self: &Point, k: f64) -> f64 {
        let k = if k.is_nan() { 1.0 } else { k };
        (self.x + self.y) * k
    }

    // String property via getter/setter (InstanceGetter/InstanceSetter) — a
    // COMPUTED String property, which `#[rtse::variable]` (scalar fields only)
    // can't express. `p.tag` reads; `p.tag = "abcd"` sets x from the &str.
    #[rtse::getter]
    fn tag(self: &Point) -> String {
        format!("P{}", self.x as i64)
    }

    #[rtse::setter]
    fn set_tag(self: &mut Point, v: &str) {
        self.x = v.len() as f64;
    }

    // F8: array-classified return (`Vec<String>` → ts `string[]`). JS gets a real
    // array: `p.parts()` → ["3","4"], `.length`===2, `[0]`==="3".
    #[rtse::method(name = "parts")]
    fn parts(self: &Point) -> Vec<String> {
        vec![format!("{}", self.x as i64), format!("{}", self.y as i64)]
    }

    // F8-nested: `Vec<(String,String)>` → ts `string[][]` (`[string,string][]`),
    // e.g. a collection's `entries()`. JS: `p.pairs()` → [["x","3"],["y","4"]].
    #[rtse::method(name = "pairs")]
    fn pairs(self: &Point) -> Vec<(String, String)> {
        vec![
            ("x".to_string(), format!("{}", self.x as i64)),
            ("y".to_string(), format!("{}", self.y as i64)),
        ]
    }
}
