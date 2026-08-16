// Cross-runtime: the whole body of a `switch` is ONE block, so a `let`/`const`
// declared in a later case is in the temporal dead zone while an earlier case
// runs. Braces around a case give it a scope of its own.

function probe(n: number): string {
  switch (n) {
    case 0:
      try {
        return "saw:" + String(later);
      } catch (e) {
        return "tdz:" + (e as any).constructor.name;
      }
    case 1:
      let later = "declared-in-case-1";
      return "case1:" + later;
    default:
      return "default:" + typeof later;
  }
}
console.log("case0=" + probe(0));
console.log("case1=" + probe(1));

// After the declaring case has run once, a NEW entry to the switch is a new
// block, so the dead zone is back.
console.log("case0_again=" + probe(0));

// `typeof` does not rescue a TDZ binding either.
function typeofProbe(n: number): string {
  switch (n) {
    case 0:
      try {
        return "typeof:" + typeof pending;
      } catch (e) {
        return "typeof_threw:" + (e as any).constructor.name;
      }
    case 1:
      let pending = 1;
      return "one:" + pending;
    default:
      return "other";
  }
}
console.log("typeof_case0=" + typeofProbe(0));
console.log("typeof_case1=" + typeofProbe(1));

// Falling through INTO the declaration initialises it, so the next case sees it.
function fallthrough(n: number): string {
  const seen: string[] = [];
  switch (n) {
    case 0:
      seen.push("zero");
    // falls through
    case 1:
      let carried = "init@" + n;
      seen.push(carried);
    // falls through
    case 2:
      seen.push("two:" + carried);
      break;
    default:
      seen.push("default");
  }
  return seen.join(">");
}
console.log("fall_from_0=" + fallthrough(0));
console.log("fall_from_1=" + fallthrough(1));

// Braces make each case its own block, so the same name can be reused.
function braced(n: number): string {
  switch (n) {
    case 0: {
      const v = "block-zero";
      return v;
    }
    case 1: {
      const v = "block-one";
      return v;
    }
    default: {
      const v = "block-default";
      return v;
    }
  }
}
console.log("braced0=" + braced(0));
console.log("braced1=" + braced(1));
console.log("braced9=" + braced(9));

// `var` in a case is function-scoped, so it exists (as undefined) everywhere.
function varInCase(n: number): string {
  switch (n) {
    case 0:
      return "case0:" + String(hoisted);
    case 1:
      var hoisted = "assigned";
      return "case1:" + hoisted;
    default:
      return "default:" + String(hoisted);
  }
}
console.log("var_case0=" + varInCase(0));
console.log("var_case1=" + varInCase(1));
console.log("var_default=" + varInCase(9));

// A `const` in a case behaves the same way as a `let`.
function constInCase(n: number): string {
  switch (n) {
    case 0:
      try {
        return "saw:" + String(fixed);
      } catch (e) {
        return "tdz:" + (e as any).constructor.name;
      }
    case 1:
      const fixed = "constant";
      return "case1:" + fixed;
    default:
      return "default";
  }
}
console.log("const_case0=" + constInCase(0));
console.log("const_case1=" + constInCase(1));

// A closure made in an early case and called after the declaration ran.
function deferred(): string {
  let capture: (() => string) | null = null;
  switch (0) {
    case 0:
      capture = () => "late:" + value;
    // falls through
    case 1:
      let value = "now-set";
      break;
    default:
      break;
  }
  return (capture as any)();
}
console.log("deferred=" + deferred());

// A closure made before the declaration and called BEFORE it is initialised.
function tooEarly(): string {
  let capture: (() => string) | null = null;
  switch (0) {
    case 0:
      capture = () => "read:" + value;
      try {
        return (capture as any)();
      } catch (e) {
        return "closure_tdz:" + (e as any).constructor.name;
      }
    case 1:
      let value = "unreached";
      return value;
    default:
      return "default";
  }
}
console.log("closure_before_init=" + tooEarly());

// The block is created once per ENTRY, so a loop around the switch re-arms the
// dead zone each time.
const armed: string[] = [];
for (let round = 0; round < 2; round++) {
  switch (round) {
    case 0:
      try {
        armed.push("read:" + String(perEntry));
      } catch (e) {
        armed.push("tdz" + round);
      }
      break;
    case 1:
      try {
        armed.push("read:" + String(perEntry));
      } catch (e) {
        armed.push("tdz" + round);
      }
      break;
    default:
      break;
  }
  // eslint-disable-next-line no-unused-vars
  let perEntry = "never-reached";
  void perEntry;
}
console.log("per_entry=" + armed.join(","));

// A `let` in the switch body is one binding shared by every case that runs.
function shared2(n: number): string {
  const out: string[] = [];
  switch (n) {
    case 0:
      let counter = 0;
      counter += 1;
      out.push("c" + counter);
    // falls through
    case 1:
      counter = (counter === undefined ? 100 : counter) + 10;
      out.push("c" + counter);
      break;
    default:
      out.push("default");
  }
  return out.join(">");
}
console.log("shared_binding=" + shared2(0));

// The switch's discriminant is evaluated in the ENCLOSING scope, so it cannot
// see the block's own let — assert it sees the outer binding instead.
const shared = "outer-shared";
function discriminant(): string {
  switch (shared) {
    case "outer-shared": {
      const shadow = "inner";
      return "matched:" + shadow;
    }
    default:
      return "no-match";
  }
}
console.log("discriminant=" + discriminant());
