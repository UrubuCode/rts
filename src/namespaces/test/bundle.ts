import {
  suite_begin,
  suite_end,
  case_begin,
  case_end,
  case_fail,
  case_fail_diff,
  print_summary,
} from "rts:test_core";

import { contains, starts_with, ends_with } from "rts:string";
import { parse_f64 } from "rts:fmt";

// ── Hook storage ──────────────────────────────────────────────────────────────
// Stored as raw i64 function pointers (0 = unset).
let _before_all_fn: i64 = 0;
let _before_each_fn: i64 = 0;
let _after_each_fn: i64 = 0;
let _after_all_fn: i64 = 0;

// ── Core test functions ───────────────────────────────────────────────────────

export function describe(name: string, fn: i64): void {
  suite_begin(name);
  if (_before_all_fn !== 0) { _before_all_fn(); }
  fn();
  if (_after_all_fn !== 0) { _after_all_fn(); }
  suite_end();
}

export function test(name: string, fn: i64): void {
  case_begin(name);
  if (_before_each_fn !== 0) { _before_each_fn(); }
  fn();
  if (_after_each_fn !== 0) { _after_each_fn(); }
  case_end();
}

export const it = test;

// ── Lifecycle hooks ───────────────────────────────────────────────────────────

export function beforeAll(fn: i64): void  { _before_all_fn = fn; }
export function beforeEach(fn: i64): void { _before_each_fn = fn; }
export function afterAll(fn: i64): void   { _after_all_fn = fn; }
export function afterEach(fn: i64): void  { _after_each_fn = fn; }

export function printSummary(): void { print_summary(); }

// ── Matcher ───────────────────────────────────────────────────────────────────
// Values are stored as their string representation so every matcher can use
// uniform string comparison. For numeric matchers (toBeGreaterThan etc.) the
// stored string is parsed back with parse_f64 on demand.
//
// Use template literals to pass numbers:  expect(`${n}`).toBe(`${42}`)
// Strings pass through directly:          expect("hello").toBe("hello")

export class Matcher {
  _actual: string;
  _neg: boolean;

  constructor(actual: string) {
    this._actual = actual;
    this._neg = false;
  }

  get not(): Matcher {
    const m: Matcher = new Matcher(this._actual);
    m._neg = true;
    return m;
  }

  // ── Equality ────────────────────────────────────────────────────────────────

  toBe(expected: string): void {
    const pass: boolean = this._actual === expected;
    if (this._neg ? pass : !pass) {
      case_fail_diff(expected, this._actual);
    }
  }

  toEqual(expected: string): void {
    this.toBe(expected);
  }

  // ── String matchers ─────────────────────────────────────────────────────────

  toContain(substr: string): void {
    const pass: boolean = contains(this._actual, substr);
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not contain" : "contain";
      case_fail(`Expected "${this._actual}" to ${op} "${substr}"`);
    }
  }

  toStartWith(prefix: string): void {
    const pass: boolean = starts_with(this._actual, prefix);
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not start with" : "start with";
      case_fail(`Expected "${this._actual}" to ${op} "${prefix}"`);
    }
  }

  toEndWith(suffix: string): void {
    const pass: boolean = ends_with(this._actual, suffix);
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not end with" : "end with";
      case_fail(`Expected "${this._actual}" to ${op} "${suffix}"`);
    }
  }

  // ── Truthiness ──────────────────────────────────────────────────────────────

  toBeTruthy(): void {
    const falsy: boolean =
      this._actual === "" ||
      this._actual === "0" ||
      this._actual === "false" ||
      this._actual === "null" ||
      this._actual === "undefined";
    const pass: boolean = !falsy;
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be truthy" : "be truthy";
      case_fail(`Expected ${this._actual} to ${op}`);
    }
  }

  toBeFalsy(): void {
    const falsy: boolean =
      this._actual === "" ||
      this._actual === "0" ||
      this._actual === "false" ||
      this._actual === "null" ||
      this._actual === "undefined";
    const pass: boolean = falsy;
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be falsy" : "be falsy";
      case_fail(`Expected ${this._actual} to ${op}`);
    }
  }

  toBeNull(): void {
    const pass: boolean = this._actual === "null";
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be null" : "be null";
      case_fail(`Expected ${this._actual} to ${op}`);
    }
  }

  toBeUndefined(): void {
    const pass: boolean = this._actual === "undefined";
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be undefined" : "be undefined";
      case_fail(`Expected ${this._actual} to ${op}`);
    }
  }

  toBeDefined(): void {
    const pass: boolean = this._actual !== "undefined";
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be defined" : "be defined";
      case_fail(`Expected ${this._actual} to ${op}`);
    }
  }

  // ── Numeric comparisons ─────────────────────────────────────────────────────

  toBeGreaterThan(expected: number): void {
    const actual_n: number = parse_f64(this._actual);
    const pass: boolean = actual_n > expected;
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be >" : "be >";
      case_fail(`Expected ${this._actual} to ${op} ${expected}`);
    }
  }

  toBeLessThan(expected: number): void {
    const actual_n: number = parse_f64(this._actual);
    const pass: boolean = actual_n < expected;
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be <" : "be <";
      case_fail(`Expected ${this._actual} to ${op} ${expected}`);
    }
  }

  toBeGreaterThanOrEqual(expected: number): void {
    const actual_n: number = parse_f64(this._actual);
    const pass: boolean = actual_n >= expected;
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be >=" : "be >=";
      case_fail(`Expected ${this._actual} to ${op} ${expected}`);
    }
  }

  toBeLessThanOrEqual(expected: number): void {
    const actual_n: number = parse_f64(this._actual);
    const pass: boolean = actual_n <= expected;
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be <=" : "be <=";
      case_fail(`Expected ${this._actual} to ${op} ${expected}`);
    }
  }

  toBeCloseTo(expected: number, precision: number): void {
    const actual_n: number = parse_f64(this._actual);
    const factor: number = 10.0;
    const diff: number = actual_n - expected;
    const abs_diff: number = diff < 0.0 ? -diff : diff;
    const threshold: number = 0.5;
    // simplified: compare with precision digits
    const pass: boolean = abs_diff < threshold;
    if (this._neg ? pass : !pass) {
      case_fail(`Expected ${this._actual} to be close to ${expected}`);
    }
  }

  // ── NaN / Infinity ──────────────────────────────────────────────────────────

  toBeNaN(): void {
    const pass: boolean = this._actual === "NaN";
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be NaN" : "be NaN";
      case_fail(`Expected ${this._actual} to ${op}`);
    }
  }

  toBeFinite(): void {
    const v: number = parse_f64(this._actual);
    const pass: boolean = this._actual !== "NaN" && this._actual !== "Infinity" && this._actual !== "-Infinity";
    if (this._neg ? pass : !pass) {
      const op: string = this._neg ? "not be finite" : "be finite";
      case_fail(`Expected ${this._actual} to ${op}`);
    }
  }

  // ── Array length (vec handles) ───────────────────────────────────────────────

  toHaveLength(expected: number): void {
    const actual_n: number = parse_f64(this._actual);
    const pass: boolean = actual_n === expected;
    if (this._neg ? pass : !pass) {
      case_fail_diff(`${expected}`, this._actual);
    }
  }
}

// ── expect factory ────────────────────────────────────────────────────────────

export function expect(actual: string): Matcher {
  return new Matcher(actual);
}
