//! `Point` — a G1 proof of the `#[rtse::class]` authoring macro. A NORMAL Rust
//! struct + impl; the macro generates the extern-C ABI glue + `register(e)`. The
//! struct is usable as plain Rust (`Point::new(3.0, 4.0).sum()`) AND as a JS class
//! (`new Point(3,4).sum()`). Storage is the generic `Entry::Rtse`.

/// A 2D point. Normal Rust struct. `#[rtse::class]` on the struct exposes the
/// `#[rtse::variable]` fields (getter + setter); the impl adds ctor/methods.
#[rtse::class("RtsePoint")]
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
}
