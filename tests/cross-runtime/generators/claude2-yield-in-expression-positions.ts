// Cross-runtime: `yield` is an EXPRESSION, so it appears mid-expression and the
// surrounding operands are evaluated around it in source order. Each sent value
// lands in exactly one hole; the results say which.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const trace: string[] = [];
function side(tag: string, v: any) { trace.push(tag); return v; }

// A driver: feeds values in order and collects everything the generator yields.
function drive(g: Generator<any, any, any>, sends: any[]): string {
  const out: string[] = [];
  let r = g.next();
  let i = 0;
  while (!r.done) {
    out.push("y:" + String(r.value));
    r = g.next(sends[i++]);
  }
  out.push("ret:" + String(r.value));
  return out.join(" ");
}

// 1) yield on both sides of a binary operator, left first
trace.length = 0;
function* binary(): Generator<any, any, any> {
  const sum = (yield "L") + (yield "R");
  return sum;
}
log("binary=" + drive(binary(), [10, 5]));

// 2) yield inside an array literal, evaluated left to right
function* inArray(): Generator<any, any, any> {
  const arr = [yield "a", side("mid", 99), yield "b"];
  return arr.join("|");
}
trace.length = 0;
log("array=" + drive(inArray(), ["A", "B"]) + " sides=" + trace.join(","));

// 3) yield as a call ARGUMENT: the callee is evaluated first, then each arg
trace.length = 0;
function join3(a: any, b: any, c: any) { trace.push("callee-run"); return a + "/" + b + "/" + c; }
function* inCall(): Generator<any, any, any> {
  return (side("callee-read", join3))(yield "x", side("arg2", "M"), yield "y");
}
log("call=" + drive(inCall(), ["X", "Y"]) + " order=" + trace.join(","));

// 4) yield in an object literal value, and in a COMPUTED key
trace.length = 0;
function* inObject(): Generator<any, any, any> {
  const o: any = { [yield "key"]: yield "value", fixed: side("fixed", 1) };
  return JSON.stringify(o);
}
log("object=" + drive(inObject(), ["k1", "v1"]) + " sides=" + trace.join(","));

// 5) yield on the right of an assignment to a member, with the object
//    expression evaluated FIRST
trace.length = 0;
function* memberAssign(): Generator<any, any, any> {
  const holder: any = { slot: "initial" };
  side("holder", holder)[yield "prop"] = yield "val";
  return JSON.stringify(holder);
}
log("memberAssign=" + drive(memberAssign(), ["p", "V"]) + " sides=" + trace.join(","));

// 6) yield inside a template literal
function* inTemplate(): Generator<any, any, any> {
  return `pre-${yield "t1"}-mid-${yield "t2"}-post`;
}
log("template=" + drive(inTemplate(), ["A", "B"]));

// 7) yield in a ternary: only the taken branch runs
function* ternary(flag: boolean): Generator<any, any, any> {
  const v = flag ? yield "then" : yield "else";
  return String(v);
}
log("ternaryTrue=" + drive(ternary(true), ["T"]));
log("ternaryFalse=" + drive(ternary(false), ["F"]));

// 8) yield behind a SHORT-CIRCUIT operator that skips it
function* shortCircuit(): Generator<any, any, any> {
  const a = false && (yield "never1");
  const b = true || (yield "never2");
  const c = null ?? (yield "taken");
  return String(a) + "," + String(b) + "," + String(c);
}
log("shortCircuit=" + drive(shortCircuit(), ["C"]));

// 9) yield as the operand of a unary operator and inside a comparison
function* unary(): Generator<any, any, any> {
  const neg = -(yield "num");
  const cmp = (yield "left") < (yield "right");
  return neg + "," + cmp;
}
log("unary=" + drive(unary(), [7, 1, 2]));

// 10) yield in a compound assignment: the target is READ before the yield
function* compound(): Generator<any, any, any> {
  let acc = 10;
  acc += yield "add";
  acc *= yield "mul";
  return String(acc);
}
log("compound=" + drive(compound(), [5, 2]));

// 11) yield in the discriminant of a switch, and inside a case body
function* inSwitch(): Generator<any, any, any> {
  switch (yield "which") {
    case "a": return "chose-a:" + (yield "a-extra");
    case "b": return "chose-b";
    default: return "chose-default";
  }
}
log("switchA=" + drive(inSwitch(), ["a", "EX"]));
log("switchDefault=" + drive(inSwitch(), ["zzz"]));

// 12) yield inside the argument of yield* -- the delegate expression is
//     evaluated once, after the inner yield settles
function* inner(prefix: string) { yield prefix + "-i1"; yield prefix + "-i2"; return prefix + "-done"; }
function* delegating(): Generator<any, any, any> {
  const r = yield* inner(yield "prefix");
  return "delegated:" + r;
}
log("yieldInsideDelegate=" + drive(delegating(), ["P"]));

// 13) a bare `yield` with no operand yields undefined
function* bare(): Generator<any, any, any> {
  const got = yield;
  return "got:" + String(got);
}
log("bare=" + drive(bare(), ["S"]));

console.log("end");
