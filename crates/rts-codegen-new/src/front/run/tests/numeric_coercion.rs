//! Numeric coercion edge cases: boolean ToNumber in arithmetic / unary operators,
//! and the `-0` literal preserving IEEE-754 negative zero.

use super::assert_stdout;

#[test]
fn boolean_tonumber_arith() {
    assert_stdout("console.log(true+1,false+5,true*3+false);", "2 5 3\n");
    assert_stdout("console.log(-true,~true,~1.5);", "-1 -2 -2\n");
}

#[test]
fn negative_zero_literal() {
    assert_stdout("console.log(1/-0);", "-Infinity\n");
    assert_stdout("console.log(Object.is(0,-0),(-0===0));", "false true\n");
}
