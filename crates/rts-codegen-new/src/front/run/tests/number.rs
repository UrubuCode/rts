//! P4: data-driven Number instance-method dispatch (the receiver is the `f64`
//! primitive) via the Registry mirror.

use super::assert_stdout;

#[test]
fn number_to_fixed() {
    assert_stdout("console.log((3.14159).toFixed(2));", "3.14\n");
}
