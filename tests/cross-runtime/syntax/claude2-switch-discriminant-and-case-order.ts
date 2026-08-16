// Cross-runtime: what a `switch` actually evaluates. The discriminant is
// evaluated exactly once, the case expressions are evaluated in source order
// only until one matches, and a `default` placed anywhere is chosen only after
// every case has been compared and failed.

const trace: string[] = [];
function probe<T>(label: string, value: T): T {
  trace.push(label);
  return value;
}

// 1) The discriminant runs once even when several cases are compared.
function once(v: number): string {
  switch (probe("disc", v)) {
    case 1: return "one";
    case 2: return "two";
    case 3: return "three";
    default: return "none";
  }
}
trace.length = 0;
console.log("once_result=" + once(3));
console.log("once_trace=" + trace.join(","));

// 2) Case expressions are evaluated top to bottom and stop at the match.
function stopsAtMatch(v: number): string {
  switch (v) {
    case probe("c1", 1): return "one";
    case probe("c2", 2): return "two";
    case probe("c3", 3): return "three";
    default: return "none";
  }
}
trace.length = 0;
console.log("stops_result=" + stopsAtMatch(2));
console.log("stops_trace=" + trace.join(","));

// 3) With no match, every case expression is evaluated before default runs.
trace.length = 0;
console.log("nomatch_result=" + stopsAtMatch(9));
console.log("nomatch_trace=" + trace.join(","));

// 4) A `default` in the middle does not stop the search: the cases BELOW it are
//    still compared, and only then is control given to default.
function defaultInMiddle(v: number): string {
  const out: string[] = [];
  switch (v) {
    case probe("first", 1):
      out.push("first");
      break;
    default:
      out.push("default");
      break;
    case probe("last", 2):
      out.push("last");
      break;
  }
  return out.join("+");
}
trace.length = 0;
console.log("mid_default_hit=" + defaultInMiddle(2));
console.log("mid_default_hit_trace=" + trace.join(","));
trace.length = 0;
console.log("mid_default_miss=" + defaultInMiddle(9));
console.log("mid_default_miss_trace=" + trace.join(","));

// 5) Fallthrough from default into the case after it, when default is chosen.
function fallsIntoNext(v: number): string {
  const out: string[] = [];
  switch (v) {
    case 1:
      out.push("one");
    default:
      out.push("default");
    case 2:
      out.push("two");
  }
  return out.join(",");
}
console.log("fall_1=" + fallsIntoNext(1));
console.log("fall_2=" + fallsIntoNext(2));
console.log("fall_9=" + fallsIntoNext(9));

// 6) No match and no default: nothing runs, but the discriminant still did.
function noDefault(v: number): string {
  const out: string[] = ["before"];
  switch (probe("disc-nd", v)) {
    case 1:
      out.push("one");
      break;
  }
  out.push("after");
  return out.join(",");
}
trace.length = 0;
console.log("no_default=" + noDefault(5));
console.log("no_default_trace=" + trace.join(","));

// 7) An empty switch body: the discriminant runs, nothing else does.
trace.length = 0;
switch (probe("empty-disc", 1)) {
}
console.log("empty_switch_trace=" + trace.join(","));

// 8) The comparison is strict, and a case expression that would coerce is
//    still evaluated — it just never matches.
function strictOnly(v: any): string {
  switch (v) {
    case probe("case-str-1", "1"): return "string-one";
    case probe("case-num-1", 1): return "number-one";
    default: return "other";
  }
}
trace.length = 0;
console.log("strict_num=" + strictOnly(1));
console.log("strict_num_trace=" + trace.join(","));
trace.length = 0;
console.log("strict_str=" + strictOnly("1"));
console.log("strict_str_trace=" + trace.join(","));

// 9) Objects match by identity, so an equal-looking literal does not.
const target = { k: 1 };
function byIdentity(v: any): string {
  switch (v) {
    case target: return "same-object";
    default: return "different";
  }
}
console.log("identity_same=" + byIdentity(target));
console.log("identity_copy=" + byIdentity({ k: 1 }));

// 10) A `break` inside a switch that sits inside a loop belongs to the SWITCH:
//     the loop keeps going.
function breakBelongsToSwitch(): string {
  const out: string[] = [];
  for (let i = 0; i < 3; i++) {
    switch (i) {
      case 1:
        out.push("skip" + i);
        break;
      default:
        out.push("keep" + i);
    }
    out.push("post" + i);
  }
  return out.join(",");
}
console.log("break_belongs=" + breakBelongsToSwitch());

// 11) The discriminant is evaluated before ANY case expression.
function orderOfFirstEval(): string {
  switch (probe("D", 7)) {
    case probe("A", 1): return "a";
    case probe("B", 7): return "b";
    default: return "d";
  }
}
trace.length = 0;
console.log("first_eval=" + orderOfFirstEval());
console.log("first_eval_trace=" + trace.join(","));

// 12) A case expression may be any expression, including one that reads a
//     variable the switch body will later change — it was already read.
function readsOnce(): string {
  let bound = 5;
  const out: string[] = [];
  switch (5) {
    case probe("bound", bound):
      bound = 99;
      out.push("matched-with-" + bound);
      break;
    default:
      out.push("unmatched");
  }
  return out.join(",");
}
trace.length = 0;
console.log("reads_once=" + readsOnce());
console.log("reads_once_trace=" + trace.join(","));
