import test from "rts:test";

// Self-referential: uses rts:test to test rts:test primitives.

test.describe("rts:test — assert");

test.it("assert passes on true");
test.assert(true);
test.pass();

test.it("assert passes with truthy number");
test.assert(1, "non-zero should be truthy");
test.pass();

test.it("assert passes with non-empty string");
test.assert("hello", "non-empty string is truthy");
test.pass();

test.describe("rts:test — assert_eq");

test.it("assert_eq passes for equal strings");
test.assert_eq("foo", "foo");
test.pass();

test.it("assert_eq passes with message on equal strings");
test.assert_eq("bar", "bar", "strings should be equal");
test.pass();

test.describe("rts:test — assert_ne");

test.it("assert_ne passes for different strings");
test.assert_ne("foo", "bar");
test.pass();

test.it("assert_ne passes with message for different strings");
test.assert_ne("a", "b", "strings should differ");
test.pass();

test.describe("rts:test — pass / describe / it");

test.it("describe and it are callable");
test.describe("nested suite");
test.it("nested case");
test.pass("nested pass works");

// Named-function callback style
function suite_with_callback() {
  test.it("callback body ran");
  test.pass("named callback works");
}

test.describe("rts:test — describe with callback", suite_with_callback);
