// ONE thing: `switch` matches by `===`, and on NUMBERS that is IEEE equality —
// so NaN matches nothing including itself, +0 and -0 match each other, and no
// amount of looking alike gets a string into a numeric case.
//
// Written because the emitter stopped asking the runtime for the label test
// when both sides are proven doubles and started emitting the machine's compare
// instruction. Those three lines above are exactly what that instruction has to
// keep answering, and the two spellings below are the point: a subject the
// compiler can prove numeric takes the instruction, one that arrives from a
// call cannot, and NEITHER is allowed to disagree with the other.

function label(x: any): string {
  switch (x) {
    case NaN:
      return "case_NaN";
    case 0:
      return "case_0";
    case -0:
      return "case_-0";
    case 1:
      return "case_1";
    case Infinity:
      return "case_Infinity";
    case -Infinity:
      return "case_-Infinity";
    case "1":
      return "case_string_1";
    case true:
      return "case_true";
    case null:
      return "case_null";
    default:
      return "default";
  }
}

// The unproven spelling: the value arrives from a call, so nothing static knows
// what it is and the label test has to ask the runtime.
function opaque(x: any): any {
  return x;
}

const probes: [string, any][] = [
  ["NaN", NaN],
  ["0/0", 0 / 0],
  ["+0", 0],
  ["-0", -0],
  ["1", 1],
  ["1.0", 1.0],
  ["Infinity", Infinity],
  ["-Infinity", -Infinity],
  ["'1'", "1"],
  ["'0'", "0"],
  ["true", true],
  ["false", false],
  ["null", null],
  ["undefined", undefined],
  ["2", 2],
];

for (const [name, value] of probes) {
  console.log(name, label(value), label(opaque(value)));
}

// The proven spelling, written so the subject is a local the compiler can see
// is a number from its initialiser onwards. Its answer must equal the one above
// for the same value.
function provenLabel(seed: number): string {
  let x = seed;
  x = x + 0;
  switch (x) {
    case 0:
      return "case_0";
    case 1:
      return "case_1";
    case 2:
      return "case_2";
    default:
      return "default";
  }
}

console.log("proven -0", provenLabel(-0), "vs +0", provenLabel(0));
console.log("proven NaN", provenLabel(NaN));
console.log("proven 1", provenLabel(1), "2", provenLabel(2), "3", provenLabel(3));

// Fall-through still falls through when the match was an instruction: the
// comparison changed, the control flow did not.
function fallThrough(n: number): string {
  const seen: string[] = [];
  switch (n) {
    case 1:
      seen.push("one");
    // falls through
    case 2:
      seen.push("two");
      break;
    case 3:
      seen.push("three");
      break;
    default:
      seen.push("none");
  }
  return seen.join(",");
}

for (const n of [1, 2, 3, 4]) console.log("fall", n, fallThrough(n));

// A subject that is proven numeric and a label that is NOT: the pair is mixed,
// so the test cannot be the instruction, and the answer still has to be right.
function mixedLabels(n: number): string {
  switch (n) {
    case 1:
      return "one";
    case "2" as any:
      return "string_two";
    case 3:
      return "three";
    default:
      return "none";
  }
}

for (const n of [1, 2, 3]) console.log("mixed", n, mixedLabels(n));
