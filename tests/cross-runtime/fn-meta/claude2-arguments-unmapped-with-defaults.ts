// Cross-runtime: an `arguments` object is UNMAPPED whenever the parameter list
// is not simple (a default, a rest or a pattern) and inside a class body. Then
// writing to `arguments[i]` never moves the parameter, and writing to the
// parameter never moves `arguments[i]`.

// 1) A default in the list makes the object unmapped in both directions.
function withDefault(a: number, b: number = 5): string {
  const before = String(a);
  arguments[0] = 99;
  const afterIndexWrite = String(a);
  a = 7;
  const afterParamWrite = String(arguments[0]);
  return before + "/" + afterIndexWrite + "/" + afterParamWrite + "/b=" + b;
}
console.log("default_unmapped=" + withDefault(1));

// 2) A rest parameter does the same.
function withRest(a: number, ...others: number[]): string {
  arguments[0] = 42;
  return a + "|" + arguments[0] + "|rest=" + others.join(",");
}
console.log("rest_unmapped=" + withRest(1, 2, 3));

// 3) A destructuring parameter too.
function withPattern([a]: number[]): string {
  arguments[0] = "replaced";
  return a + "|" + String(arguments[0]);
}
console.log("pattern_unmapped=" + withPattern([8]));

// 4) A class method's body is strict, so its `arguments` is unmapped even with
//    a simple parameter list.
class Strictly {
  probe(a: number): string {
    arguments[0] = 100;
    const seen = a;
    a = 3;
    return seen + "|" + String(arguments[0]);
  }
}
console.log("class_method_unmapped=" + new Strictly().probe(1));

// 5) `arguments.length` counts what was PASSED, not what was declared.
function counts(a: number, b: number = 0, c: number = 0): string {
  return "passed=" + arguments.length + " declared=" + counts.length;
}
console.log("counts_one=" + counts(1));
console.log("counts_three=" + counts(1, 2, 3));
console.log("counts_extra=" + counts(1, 2, 3, 4, 5));

// 6) An argument that was not passed is absent from the object, not undefined.
function absence(a: number, b: number = 1): string {
  return "has0=" + (0 in arguments) + " has1=" + (1 in arguments) +
    " keys=" + Object.keys(arguments).join(",");
}
console.log("absence=" + absence(1));

// 7) The object is array-LIKE, not an array.
function shape(a: number = 0): string {
  return "isArray=" + Array.isArray(arguments) +
    " typeof=" + typeof arguments +
    " tag=" + Object.prototype.toString.call(arguments) +
    " hasMap=" + (typeof (arguments as any).map);
}
console.log("shape=" + shape(1, 2 as any));

// 8) It is iterable, and its iterator is the array one.
function iterates(a: number = 0): string {
  const collected: string[] = [];
  for (const v of arguments as any) collected.push(String(v));
  const spread = [...(arguments as any)];
  const converted = Array.from(arguments as any);
  return collected.join(",") + "|" + spread.join(",") + "|" + converted.length +
    "|sameIterator=" + ((arguments as any)[Symbol.iterator] === Array.prototype.values);
}
console.log("iterates=" + iterates(1, 2 as any, 3 as any));

// 9) Writing past the end adds an ordinary property and moves `length` only if
//    the index is written through `length` itself.
function beyond(a: number = 0): string {
  const startLength = arguments.length;
  (arguments as any)[5] = "far";
  return "start=" + startLength + " after=" + arguments.length + " read=" + (arguments as any)[5];
}
console.log("beyond=" + beyond(1));

// 10) `length` is writable on the object.
function lengthWritable(a: number = 0): string {
  const accepted = Reflect.set(arguments, "length", 9);
  return "accepted=" + accepted + " value=" + arguments.length;
}
console.log("length_writable=" + lengthWritable(1, 2 as any));

// 11) An arrow inside sees the ENCLOSING function's arguments, having none of
//     its own.
function outerArguments(a: number = 0): string {
  const inner = (): string => String(arguments.length) + ":" + String((arguments as any)[0]);
  return inner();
}
console.log("arrow_sees_outer=" + outerArguments(7, 8 as any));

// 12) A nested ordinary function has its own.
function nestedOwn(a: number = 0): string {
  function inner(x: number = 0): string {
    return "inner=" + arguments.length;
  }
  return inner(1, 2 as any) + " outer=" + arguments.length;
}
console.log("nested_own=" + nestedOwn(1, 2 as any, 3 as any));

// 13) Copying the object gives a real array that no longer tracks anything.
function copied(a: number = 0): string {
  const snapshot = Array.prototype.slice.call(arguments);
  (arguments as any)[0] = "changed";
  return snapshot.join(",") + "|live=" + String((arguments as any)[0]);
}
console.log("copied=" + copied(1, 2 as any));

// 14) The object survives the call it was made in, and keeps its values.
function escapes(a: number = 0): any {
  return arguments;
}
const escaped: any = escapes(1, 2 as any);
console.log("escaped=" + escaped.length + "|" + escaped[0] + "," + escaped[1]);

// 15) Two calls make two objects.
console.log("distinct_objects=" + (escapes(1) !== escapes(1)));

// 16) An unmapped object still reflects the parameter's INITIAL value, so a
//     default that was applied is invisible to it.
function defaultInvisible(a: number, b: number = 500): string {
  return "b=" + b + " arguments1=" + String((arguments as any)[1]) + " length=" + arguments.length;
}
console.log("default_invisible=" + defaultInvisible(1));

// 17) `callee` is present on an unmapped object in sloppy code only when it is
//     not poisoned; its presence is probed rather than read.
function calleeProbe(a: number = 0): string {
  const d: any = Object.getOwnPropertyDescriptor(arguments, "callee");
  return "has_callee_slot=" + (d !== undefined);
}
console.log("callee_slot=" + calleeProbe(1));

// 18) Deleting an index removes it without changing `length`.
function deletes(a: number = 0): string {
  const accepted = Reflect.deleteProperty(arguments, "0");
  return "accepted=" + accepted + " has0=" + (0 in arguments) + " length=" + arguments.length;
}
console.log("deletes=" + deletes(1, 2 as any));

// 19) The indices are ordinary writable, enumerable, configurable properties.
function indexAttrs(a: number = 0): string {
  const d: any = Object.getOwnPropertyDescriptor(arguments, "0");
  return "w=" + d.writable + " e=" + d.enumerable + " c=" + d.configurable;
}
console.log("index_attrs=" + indexAttrs(1));

// 20) A generator's own arguments object behaves the same way.
function* generated(a: number = 0): Generator<string> {
  yield "len=" + arguments.length;
  (arguments as any)[0] = "gen";
  yield "a=" + a + " arg0=" + String((arguments as any)[0]);
}
const g = generated(1, 2 as any);
console.log("generator_1=" + g.next().value);
console.log("generator_2=" + g.next().value);
