import { describe, test, expect } from "rts:test";

// `outer.throw(e)` and `outer.return(v)` while `outer` is parked inside `yield*`
// reach the INNER iterator first. When the inner's own `throw` or `return` raises,
// the error belongs to the `yield*` expression: a `try` written around it catches.
// Node and Bun answer from the `catch`; this engine used to end `outer` and let the
// error escape to whoever called `outer.throw`.

function* plain() {
  yield 1;
  yield 2;
}

function* catchesAroundDelegation() {
  try {
    yield* plain();
  } catch (e) {
    yield "outer caught " + e;
  }
}

function* raisesOnReturn() {
  try {
    yield 1;
  } finally {
    throw "from return";
  }
}

function* catchesTheReturnRaise() {
  try {
    yield* raisesOnReturn();
  } catch (e) {
    yield "outer caught " + e;
  }
}

describe("a raise from the delegated iterator is the yield*'s", () => {
  test("throw() forwarded to an inner iterator that raises is caught around yield*", () => {
    const g = catchesAroundDelegation();
    g.next();
    const answered = g.throw("boom");
    expect(answered.value).toBe("outer caught boom");
    expect(answered.done).toBe(false);
  });

  test("return() forwarded to an inner iterator whose finally raises is caught too", () => {
    const g = catchesTheReturnRaise();
    g.next();
    const answered = g.return(3);
    expect(answered.value).toBe("outer caught from return");
    expect(answered.done).toBe(false);
  });
});
