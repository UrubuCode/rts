import { describe, test, expect } from "rts:test";

function t_eq_string() {
  expect("foo").toBe("foo");
}

function t_eq_number_as_string() {
  const n = 21 + 21;
  expect(`${n}`).toBe("42");
}

function t_contains() {
  expect("hello-rts").toContain("rts");
}

function t_prefix() {
  expect("namespace:rts").toStartWith("namespace:");
}

function t_suffix() {
  expect("fixtures.test.ts").toEndWith(".ts");
}

function t_greater_than() {
  expect(`${10}`).toBeGreaterThan(5);
}

function t_less_than_or_equal() {
  expect(`${5}`).toBeLessThanOrEqual(5);
}

function t_truthy() {
  expect("ok").toBeTruthy();
}

function t_falsy() {
  expect("0").toBeFalsy();
}

function suite_matchers() {
  test("toBe with string", t_eq_string);
  test("toBe with numeric interpolation", t_eq_number_as_string);
  test("toContain", t_contains);
  test("toStartWith", t_prefix);
  test("toEndWith", t_suffix);
  test("toBeGreaterThan", t_greater_than);
  test("toBeLessThanOrEqual", t_less_than_or_equal);
  test("toBeTruthy", t_truthy);
  test("toBeFalsy", t_falsy);
}

describe("rts:test matchers", suite_matchers);
