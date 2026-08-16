// Cross-runtime: the temporal dead zone. A `let`, `const` or `class` binding
// exists from the top of its block but throws ReferenceError on every touch —
// read, write and `typeof` alike — until its declaration is evaluated.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

// A read before the declaration, inside the same block.
console.log("let-read=" + probe(() => {
  const r = probe(() => (later as any));
  let later = 1;
  return r + "/then:" + later;
}));

// typeof does NOT protect a TDZ binding, though it does protect a name that
// was never declared at all.
console.log("typeof-tdz=" + probe(() => {
  const t = probe(() => typeof (pending as any));
  let pending = 1;
  return t + "/after:" + typeof pending;
}));
console.log("typeof-missing-global=" + typeof (globalThis as any).neverDeclaredAnywhere);

// A write before the declaration is refused the same way.
console.log("let-write=" + probe(() => {
  const w = probe(() => {
    assignFirst = 5;
    return "wrote";
  });
  let assignFirst = 1;
  return w + "/final:" + assignFirst;
}));

// const behaves identically.
console.log("const-tdz=" + probe(() => {
  const r = probe(() => (fixed as any));
  const fixed = 2;
  return r + "/value:" + fixed;
}));

// A class declaration is in TDZ too, so `new` before it fails.
console.log("class-tdz=" + probe(() => {
  const r = probe(() => new (Later as any)());
  class Later {}
  return r + "/then:" + typeof Later;
}));

// The heritage clause is evaluated before the binding is initialised, so a
// class cannot extend itself.
console.log("extends-self=" + probe(() => {
  class SelfRef extends (SelfRef as any) {}
  return typeof SelfRef;
}));

// A static initialiser referring to its OWN class binding is fine: by then the
// binding inside the class scope is initialised.
console.log("static-self=" + probe(() => {
  class Ok {
    static via: string = "ok";
    static self: string = Ok.via + "-again";
  }
  return Ok.self;
}));

// A default parameter may not see a later parameter; the other direction is
// allowed.
function forwardOrder(a: any = (b as any), b: any = 2): string {
  return String(a) + "," + String(b);
}
console.log("param-forward=" + probe(() => forwardOrder()));
console.log("param-forward-supplied=" + probe(() => forwardOrder(1)));

function backwardOrder(a: any = 1, b: any = a + 1): string {
  return a + "," + b;
}
console.log("param-backward=" + probe(() => backwardOrder()));

// The parameter scope is outside the body scope, so a default may not reach a
// body-level let of the same name.
function shadowed(v: any = (hidden as any)): string {
  let hidden = "body";
  return String(v) + "/" + hidden;
}
console.log("param-body-let=" + probe(() => shadowed()));
console.log("param-body-let-supplied=" + probe(() => shadowed("given")));

// The dead zone is per BLOCK: an inner declaration shadows the outer name for
// the whole inner block, including the part above itself.
const shared = "outer";
console.log("block-shadow=" + probe(() => {
  const seen: string[] = [];
  {
    seen.push(probe(() => (shared as any)));
    const shared = "inner";
    seen.push("inner-value:" + shared);
  }
  seen.push("outer-value:" + shared);
  return seen.join("|");
}));

// A loop body gets a fresh binding per iteration, each with its own dead zone.
const results: string[] = [];
for (let i = 0; i < 3; i++) {
  results.push(probe(() => {
    const before = probe(() => (perIteration as any));
    let perIteration = i;
    return before + ":" + perIteration;
  }));
}
console.log("loop=" + results.join("|"));

// switch shares one block across every case, so a case ABOVE the declaration
// is still inside the dead zone.
function inSwitch(n: number): string {
  switch (n) {
    case 0:
      return probe(() => (declaredInCase as any));
    case 1:
      let declaredInCase = "one";
      return "declared:" + declaredInCase;
    default:
      return "none";
  }
}
console.log("switch-0=" + inSwitch(0));
console.log("switch-1=" + inSwitch(1));
console.log("switch-2=" + inSwitch(2));

// For contrast, `var` is hoisted and initialised to undefined, and a function
// declaration is fully hoisted.
function hoisted(): string {
  const v = String(varName);
  const f = typeof named;
  var varName = 1;
  function named(): number {
    return 1;
  }
  return v + "/" + f + "/" + varName;
}
console.log("var-hoisted=" + hoisted());

// The TDZ is a run-time state, not a parse-time one: never running the code
// above a declaration never raises anything.
console.log("never-run=" + probe(() => {
  let touched = "no";
  if (false) {
    touched = (unreached as any);
  }
  let unreached = "yes";
  return touched + "/" + unreached;
}));
