// Cross-runtime: automatic semicolon insertion. A newline ends a statement only
// when the next token cannot continue it — except after `return`, `break`,
// `continue` and before a postfix `++`/`--`, where the newline is decisive.

// 1) `return` with its value on the next line answers undefined.
function returnNewline(): any {
  return
  "never-delivered";
}
console.log("return_newline=" + String(returnNewline()));

// 2) The value on the SAME line is delivered normally.
function returnSameLine(): any {
  return "delivered";
}
console.log("return_same_line=" + returnSameLine());

// 3) A line starting with an operator continues the previous expression.
function continuedByOperator(): number {
  let total = 1
    + 2
    + 3;
  return total;
}
console.log("continued_operator=" + continuedByOperator());

// 4) A line starting with `(` continues it too — this is a CALL, not two
//    statements.
function continuedByParen(): string {
  const f = function (x: string): string { return "called:" + x; };
  const value = f
  ("arg");
  return value;
}
console.log("continued_paren=" + continuedByParen());

// 5) A line starting with `[` is a member access on the previous expression.
function continuedByBracket(): string {
  const arr = ["zero", "one"]
  const picked = arr
  [1];
  return picked;
}
console.log("continued_bracket=" + continuedByBracket());

// 6) A line starting with `.` continues a member chain.
function continuedByDot(): string {
  return "abc"
    .toUpperCase()
    .slice(0, 2);
}
console.log("continued_dot=" + continuedByDot());

// 7) `++` on its own line binds to the NEXT line, as a prefix, because a
//    postfix operator may not be preceded by a line break.
function postfixNewline(): string {
  let a = 1;
  let b = 10;
  a
  ++
  b;
  return "a=" + a + " b=" + b;
}
console.log("postfix_newline=" + postfixNewline());

// 8) On one line, the same tokens are a postfix increment.
function postfixSameLine(): string {
  let a = 1;
  a++;
  return "a=" + a;
}
console.log("postfix_same_line=" + postfixSameLine());

// 9) `break` with its label on the next line is a plain break — but a plain
//    break out of a labelled BLOCK is a syntax error, so this uses a loop where
//    the unlabelled break is legal and leaves the inner loop only.
function breakNewline(): string {
  const seen: string[] = [];
  // A value of this name exists so the stray `outer;` statement ASI leaves
  // behind is a harmless expression rather than an unresolved reference.
  const outer = "not-a-label-here";
  outer: for (let i = 0; i < 2; i++) {
    for (let j = 0; j < 3; j++) {
      if (j === 1) break
      outer;
      seen.push(i + "" + j);
    }
    seen.push("outer-continues" + i);
  }
  return seen.join(",");
}
console.log("break_newline=" + breakNewline());

// 10) With the label on the same line, the outer loop is what ends.
function breakSameLine(): string {
  const seen: string[] = [];
  outer: for (let i = 0; i < 2; i++) {
    for (let j = 0; j < 3; j++) {
      if (j === 1) break outer;
      seen.push(i + "" + j);
    }
    seen.push("outer-continues" + i);
  }
  return seen.join(",");
}
console.log("break_same_line=" + breakSameLine());

// 11) `continue` behaves the same way: the label must share the line.
function continueNewline(): string {
  const seen: string[] = [];
  const outer = "not-a-label-here";
  outer: for (let i = 0; i < 2; i++) {
    for (let j = 0; j < 2; j++) {
      if (j === 0) continue
      outer;
      seen.push(i + "" + j);
    }
  }
  return seen.join(",");
}
console.log("continue_newline=" + continueNewline());

// 12) Statements without semicolons before a `}` are terminated by ASI.
function noSemisInBlock(): number {
  let n = 0
  n += 5
  return n
}
console.log("no_semis=" + noSemisInBlock());

// 13) A `throw` argument must be on the same line; here it is, and the newline
//     inside the STRING makes no difference.
function throwSameLine(): string {
  try {
    throw new RangeError("x");
  } catch (e) {
    return (e as any).constructor.name;
  }
}
console.log("throw_same_line=" + throwSameLine());

// 14) The three semicolons of a `for` head are never inserted, and an empty
//     head section is written explicitly.
function forHeadSemis(): string {
  let i = 0;
  for (;;) {
    i += 1;
    if (i > 3) break;
  }
  return "i=" + i;
}
console.log("for_head=" + forHeadSemis());

// 15) A `do-while` gets an ASI after its closing paren, so the next line starts
//     a new statement.
function doWhileAsi(): string {
  let n = 0;
  do n += 1; while (n < 3)
  const after = "next-statement";
  return n + "/" + after;
}
console.log("do_while_asi=" + doWhileAsi());

// 16) An expression statement beginning with a template literal continues the
//     previous line as a tagged template.
function taggedByNewline(): string {
  const tag = function (parts: any): string { return "tagged:" + parts[0]; };
  const value = tag
  `body`;
  return value;
}
console.log("tagged_newline=" + taggedByNewline());

// 17) A line starting with a unary minus continues the previous expression as
//     a subtraction, not as a new negation statement.
function minusContinues(): string {
  let a = 10
  let b = 4
  const c = a
  - b;
  return "c=" + c;
}
console.log("minus_continues=" + minusContinues());

// 18) An `if` without braces takes exactly one statement; the next line is
//     outside it.
function singleStatementIf(): string {
  const seen: string[] = [];
  if (false)
    seen.push("inside");
  seen.push("outside");
  return seen.join(",") || "empty";
}
console.log("single_statement_if=" + singleStatementIf());

// 19) An arrow body on the next line is fine as long as the `=>` shares the
//     parameter's line.
function arrowNewlineBody(): string {
  const f = (x: number): string =>
    "got:" + x;
  return f(3);
}
console.log("arrow_newline_body=" + arrowNewlineBody());

// 20) A ternary split across lines is one expression.
function ternaryLines(): string {
  const v = 5;
  return v > 3
    ? "big"
    : "small";
}
console.log("ternary_lines=" + ternaryLines());

// 21) An object literal on the line after `return` is lost the same way a value
//     is; the braces are then a block statement that never runs.
function returnObjectNewline(): any {
  return
  { ok: true };
}
console.log("return_object_newline=" + String(returnObjectNewline()));
