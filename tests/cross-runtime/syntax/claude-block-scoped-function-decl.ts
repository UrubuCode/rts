// Cross-runtime: a function declaration inside a BLOCK is block-scoped in
// strict code (this module is strict), hoisted to the top of that block, and
// invisible outside it.

{
  // Hoisted within the block: callable before its own text.
  const beforeText = shape();
  function shape(): string { return "inner-block"; }
  console.log("hoisted_in_block=" + beforeText);
  console.log("inner_call=" + shape());
}
console.log("typeof_shape_after=" + typeof (globalThis as any).shape);

// Two sibling blocks each declare their own.
{
  function pick(): number { return 1; }
  console.log("sibling_a=" + pick());
}
{
  function pick(): number { return 2; }
  console.log("sibling_b=" + pick());
}
console.log("typeof_pick_outside=" + typeof (globalThis as any).pick);

// An `if` body is a block too.
if (true) {
  function conditional(): string { return "yes"; }
  console.log("in_if=" + conditional());
}
console.log("typeof_conditional_after=" + typeof (globalThis as any).conditional);

// A block-scoped declaration shadows an outer `const` only inside the block.
const level = (): string => "L0";
const probe = (): string => level();
{
  function level(): string { return "L1"; }
  console.log("inner_level=" + level());
  // `probe` closed over the outer binding, so it is unaffected.
  console.log("probe_from_inside=" + probe());
}
console.log("outer_level=" + level());

// Nested blocks stack.
{
  function depth(): string { return "d1"; }
  {
    function depth(): string { return "d2"; }
    {
      function depth(): string { return "d3"; }
      console.log("d3=" + depth());
    }
    console.log("d2=" + depth());
  }
  console.log("d1=" + depth());
}

// A block-scoped function can close over a `let` of the same block.
{
  let counter = 0;
  function bump(): number { counter += 1; return counter; }
  bump();
  bump();
  console.log("closes_over_block_let=" + bump());
}

// Inside a loop body, a fresh function is created each iteration.
const perIteration: Array<() => number> = [];
for (let i = 0; i < 3; i++) {
  function makeValue(): number { return i * 10; }
  perIteration.push(makeValue);
}
console.log("per_iteration_ids=" + (perIteration[0] === perIteration[1]));
console.log("per_iteration_vals=" + perIteration.map((f) => f()).join(","));

// A function declared in a block is a normal function: it has `prototype` and
// its `name` is the declared one.
{
  function named(): void { /* nothing */ }
  console.log("name=" + named.name);
  console.log("has_prototype=" + Object.prototype.hasOwnProperty.call(named, "prototype"));
  console.log("length=" + named.length);
}

// Inside a function body, a block declaration does not leak to the body scope.
function host(): string {
  const parts: string[] = [];
  {
    function local(): string { return "block"; }
    parts.push(local());
  }
  parts.push(typeof (globalThis as any).local);
  return parts.join(",");
}
console.log("in_function=" + host());

// A `switch` body is one block, so a declaration in a case is visible in the
// whole switch (function declarations are hoisted across the cases).
function inSwitch(n: number): string {
  switch (n) {
    case 0:
      return helper();
    case 1:
      function helper(): string { return "from-helper"; }
      return "case1:" + helper();
    default:
      return "default";
  }
}
console.log("switch_case1=" + inSwitch(1));
console.log("switch_default=" + inSwitch(9));

// A `try` block scopes its declarations too.
try {
  function inTry(): string { return "try"; }
  console.log("in_try=" + inTry());
} catch {
  console.log("unreachable");
}
console.log("typeof_inTry_after=" + typeof (globalThis as any).inTry);
