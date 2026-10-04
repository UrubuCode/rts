// A `SyntaxError` from generated source says WHAT is wrong, not what was given.
//
// `new Function(body)` with a body that does not parse used to answer
// `SyntaxError: the Function body did not compile: <the body>` — the input read
// back at the program. The parser had the answer and the hook's signature threw
// it away: `FunctionCompiler` was `fn(&[String], &str) -> Option<u64>`, and a
// `None` has nowhere to carry a reason. Issue #2892.
//
// Why this matters here and nowhere else: source compiled at run time has no
// file for the author to open, so the message is the only diagnostic there is.
// A template engine that compiles each template to a function (`kire`) could
// only report the generated code back to the person who wrote the template.
//
// # What is asserted, and what deliberately is not
//
// The three engines word the same fault differently — measured 2026-10-03,
// node 22.23.2 and bun 1.4.0 beside this one:
//
//   body                     | rts                            | node                          | bun
//   -------------------------|--------------------------------|-------------------------------|------
//   `let const = 1;`         | Expected ';', '}' or <eof>     | Unexpected token 'const'      | Unexpected keyword 'const'
//   `if (true { return 1; }` | Expected ')', got '{'          | Unexpected token '{'          | Unexpected token '{'. …
//   `return 1; }`            | Expression expected            | Unexpected token '}'          | Parser error
//   `var 1x = 2;`            | Identifier cannot follow number| Invalid or unexpected token   | No identifiers allowed …
//
// So the phrase is NOT asserted: nothing depends on it and pinning it would
// pin SWC's wording. What is asserted is what all three agree on and what the
// defect was about — a `SyntaxError` is raised, the message does not contain
// the body, and it is not the old "did not compile" sentence. Two of the four
// also agree on a token across all three engines (`{` and the word
// token/identifier), and those are asserted per case.
//
// `return 42;` is the control, and it is the case a tightened check would
// break: a `Function` body IS a function body, so `return` at its top is
// legal. Note that `eval("return 42;")` is a different question — see the end
// of this file.

import { describe, test, expect } from "rts:test";

const AsyncFunction = (async () => {}).constructor as any;

const broken = [
  "let const = 1;",
  "if (true { return 1; }",
  "return 1; }",
  "var 1x = 2;",
];

/// The `SyntaxError` a builder raised for this body, or `null` if none did.
function failureOf(build: (body: string) => unknown, body: string): Error | null {
  try {
    build(body);
    return null;
  } catch (raised) {
    return raised as Error;
  }
}

describe("a Function body that does not parse reports the parser's reason", () => {
  test("every broken body still raises a SyntaxError", () => {
    for (const body of broken) {
      const raised = failureOf((b) => new Function(b), body);
      expect(raised === null).toBe(false);
      expect((raised as Error).name).toBe("SyntaxError");
    }
  });

  test("the message names the fault and not the input", () => {
    for (const body of broken) {
      const message = String((failureOf((b) => new Function(b), body) as Error).message);
      // The defect, stated as the test that catches it coming back: the whole
      // body was the message.
      expect(message.includes(body)).toBe(false);
      expect(message.includes("did not compile")).toBe(false);
      expect(message.length > 0).toBe(true);
    }
  });

  test("an unclosed `if` condition names the brace, as all three engines do", () => {
    const raised = failureOf((b) => new Function(b), "if (true { return 1; }");
    const message = String((raised as Error).message);
    expect(message.includes("{")).toBe(true);
  });

  test("an identifier after a number names the token, as all three engines do", () => {
    const message = String(
      (failureOf((b) => new Function(b), "var 1x = 2;") as Error).message,
    ).toLowerCase();
    expect(message.includes("token") || message.includes("identifier")).toBe(true);
  });

  test("a legal body still compiles and runs — the control", () => {
    const made = new Function("return 42;") as () => number;
    expect(made()).toBe(42);
    const adding = new Function("a", "b", "return a + b;") as (a: number, b: number) => number;
    expect(adding(2, 3)).toBe(5);
  });
});

describe("the async function constructor takes the same door", () => {
  // `(async () => {}).constructor` is what a library reaches for to build an
  // async function from text, and it goes through the same runtime hook. It
  // had the same useless message and is fixed by the same change, so it is
  // measured rather than assumed.
  test("a broken async body reports the reason too", () => {
    for (const body of broken) {
      const raised = failureOf((b) => new AsyncFunction(b), body);
      expect(raised === null).toBe(false);
      expect((raised as Error).name).toBe("SyntaxError");
      expect(String((raised as Error).message).includes(body)).toBe(false);
      expect(String((raised as Error).message).includes("did not compile")).toBe(false);
    }
  });

  test("a legal async body still compiles", () => {
    const made = new AsyncFunction("return 7;") as () => Promise<number>;
    expect(typeof made).toBe("function");
  });
});

describe("eval reports the reason through the same hook", () => {
  // `eval` is a second callback on the same seam (`EvalCompiler`), and it had
  // the matching message — `the eval source did not compile: <source>`.
  test("broken eval source names the fault", () => {
    for (const source of ["let const = 1;", "var 1x = 2;"]) {
      const raised = failureOf((s) => eval(s), source);
      expect(raised === null).toBe(false);
      expect((raised as Error).name).toBe("SyntaxError");
      expect(String((raised as Error).message).includes(source)).toBe(false);
      expect(String((raised as Error).message).includes("did not compile")).toBe(false);
    }
  });

  test("legal eval source still runs", () => {
    expect(eval("1 + 1")).toBe(2);
  });
});
