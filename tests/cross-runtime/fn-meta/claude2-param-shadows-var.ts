// Cross-runtime: a parameter and a body `var` of the same name. With a SIMPLE
// parameter list they are one binding, so `var x;` keeps the argument. With a
// default, a rest or a pattern in the list, the body gets a separate COPY — and
// a closure made in the parameter list keeps watching the parameter.

// 1) A bare `var` redeclaration does not reset the parameter to undefined.
function bareVar(x: any): string {
  var x;
  return String(x);
}
console.log("bare_var=" + bareVar("argument"));
console.log("bare_var_missing=" + bareVar());

// 2) A `var` WITH an initializer overwrites it.
function initialisedVar(x: any): string {
  var x = "body-value";
  return String(x);
}
console.log("initialised_var=" + initialisedVar("argument"));

// 3) The overwrite is local: the caller's value is untouched.
const passed = { v: "caller" };
function mutatesLocal(x: any): string {
  x = "reassigned";
  return String(x);
}
console.log("local_only=" + mutatesLocal(passed.v) + "|caller=" + passed.v);

// 4) With a SIMPLE list, the parameter and the body `var` are one binding, so a
//    closure made in the body sees the later write.
function oneBinding(x: any): string {
  const read = () => String(x);
  var x = "written-later";
  return read();
}
console.log("one_binding=" + oneBinding("argument"));

// 5) With a DEFAULT in the list, the body's `var` is a copy: a closure created
//    in the PARAMETER list keeps reading the parameter.
function separateScopes(x: any, peek: any = () => String(x)): string {
  var x = "body-copy";
  return "body=" + String(x) + " param=" + peek();
}
console.log("separate_scopes=" + separateScopes("argument"));

// 6) The same split with a rest parameter present.
function withRest(x: any, peek: any = () => String(x), ...others: any[]): string {
  var x = "body-copy";
  return "body=" + String(x) + " param=" + peek() + " rest=" + others.length;
}
console.log("with_rest=" + withRest("argument"));

// 7) A closure made in the BODY of the same function sees the body copy.
function bodyClosure(x: any, peek: any = () => String(x)): string {
  var x = "body-copy";
  const bodyPeek = () => String(x);
  x = "changed-again";
  return "body=" + bodyPeek() + " param=" + peek();
}
console.log("body_closure=" + bodyClosure("argument"));

// 8) A parameter shadows an outer binding of the same name for the whole call.
var outerName = "outer";
function shadowsOuter(outerName: string): string {
  const inner = outerName;
  outerName = "changed-inside";
  return inner + "/" + outerName;
}
console.log("shadows_outer=" + shadowsOuter("parameter") + "|outer_after=" + outerName);

// 9) A nested function's parameter shadows the enclosing function's.
function nestedShadow(v: string): string {
  function inner(v: string): string {
    return "inner:" + v;
  }
  return inner("nested") + " outer:" + v;
}
console.log("nested_shadow=" + nestedShadow("outer-arg"));

// 10) A hoisted function DECLARATION whose name matches a parameter replaces
//     the argument for the whole body.
function hoistedOverParam(dup: any): string {
  const seenAtEntry = typeof dup;
  return seenAtEntry + "/" + typeof dup;
  function dup(): void {}
}
console.log("hoisted_over_param=" + hoistedOverParam("string-argument"));

// 11) Assigning to that name afterwards works normally.
function hoistedThenAssigned(dup: any): string {
  const before = typeof dup;
  dup = "now-a-string";
  return before + "->" + typeof dup;
  function dup(): void {}
}
console.log("hoisted_then_assigned=" + hoistedThenAssigned(1));

// 12) A `let` inside an inner BLOCK may shadow the parameter there only.
function blockShadow(v: string): string {
  let seen = "";
  {
    let v = "block";
    seen = v;
  }
  return seen + "/" + v;
}
console.log("block_shadow=" + blockShadow("param"));

// 13) A default expression sees parameters to its LEFT, never the body.
function leftOnly(a: number, b: number = a * 2): string {
  var a = 100;
  return "a=" + a + " b=" + b;
}
console.log("left_only=" + leftOnly(3));

// 14) A default may call a function declared in an enclosing scope, but a
//     function declared in the BODY is not visible from the parameter list.
function outerHelper(): string {
  return "outer-helper";
}
function defaultUsesOuter(v: string = outerHelper()): string {
  function outerHelper(): string {
    return "body-helper";
  }
  return v + "|body=" + outerHelper();
}
console.log("default_uses_outer=" + defaultUsesOuter());

// 15) The parameter's own value is not shared between calls.
function accumulates(acc: any = []): string {
  acc.push("x");
  return String(acc.length);
}
console.log("fresh_default=" + accumulates() + "," + accumulates() + "," + accumulates());

// 16) A shared default OBJECT is, because the expression names one.
const sharedBox: any = { count: 0 };
function sharesBox(box: any = sharedBox): string {
  box.count += 1;
  return String(box.count);
}
console.log("shared_default=" + sharesBox() + "," + sharesBox() + "," + sharesBox({ count: 10 }));

// 17) A parameter named like the function itself shadows it inside.
function selfName(selfName: any): string {
  return typeof selfName;
}
console.log("self_name=" + selfName("shadowed") + "/" + typeof selfName);

// 18) A destructured parameter's names shadow the same way, and a body `var`
//     of one of those names is again a copy.
function patternShadow({ p }: any, peek: any = () => String(p)): string {
  var p = "body-copy";
  return "body=" + String(p) + " param=" + peek();
}
console.log("pattern_shadow=" + patternShadow({ p: "from-pattern" }));

// 19) The copy is made at entry, so a parameter mutated later in the list's own
//     scope is not seen by the body.
function mutatedInList(a: any, touch: any = (): void => { a = "changed-in-list"; }): string {
  var a = a;
  touch();
  return "body=" + String(a);
}
console.log("mutated_in_list=" + mutatedInList("start"));

// 20) Two nested functions each with the same parameter name stay independent.
function level1(v: string): string {
  function level2(v: string): string {
    function level3(v: string): string {
      return "3:" + v;
    }
    return level3(v + "-c") + " 2:" + v;
  }
  return level2(v + "-b") + " 1:" + v;
}
console.log("three_levels=" + level1("a"));
