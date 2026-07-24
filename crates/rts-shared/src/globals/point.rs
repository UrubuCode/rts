//! `Point` — a G1 proof of the `#[rtse::class]` authoring macro. A NORMAL Rust
//! struct + impl; the macro generates the extern-C ABI glue + `register(e)`. The
//! struct is usable as plain Rust (`Point::new(3.0, 4.0).sum()`) AND as a JS class
//! (`new Point(3,4).sum()`). Storage is the generic `Entry::Rtse`.

/// A 2D point. Normal Rust struct.
pub struct Point {
    x: f64,
    y: f64,
}

#[rtse::class("Point")]
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
}
