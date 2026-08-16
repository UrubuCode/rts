// Cross-runtime: `name` inference in the positions a plain declaration does not
// cover — accessors get a "get "/"set " prefix, a symbol-keyed method gets
// "[description]", and an anonymous value in a binding takes the binding's name.

// Accessors on an object literal.
const withAccessors: any = {
  get width(): number { return 1; },
  set width(v: number) { void v; },
  get readOnly(): number { return 2; },
};
const wd = Object.getOwnPropertyDescriptor(withAccessors, "width") as any;
console.log("getter_name=" + JSON.stringify(wd.get.name));
console.log("setter_name=" + JSON.stringify(wd.set.name));
console.log("getter_length=" + wd.get.length);
console.log("setter_length=" + wd.set.length);
const rd = Object.getOwnPropertyDescriptor(withAccessors, "readOnly") as any;
console.log("readonly_getter=" + JSON.stringify(rd.get.name));
console.log("readonly_has_setter=" + (rd.set === undefined));

// Accessors on a class, instance and static.
class Shaped {
  static get kind(): string { return "static"; }
  static set kind(v: string) { void v; }
  get size(): number { return 3; }
  set size(v: number) { void v; }
}
const sd = Object.getOwnPropertyDescriptor(Shaped, "kind") as any;
console.log("static_getter=" + JSON.stringify(sd.get.name));
console.log("static_setter=" + JSON.stringify(sd.set.name));
const id = Object.getOwnPropertyDescriptor(Shaped.prototype, "size") as any;
console.log("proto_getter=" + JSON.stringify(id.get.name));
console.log("proto_setter=" + JSON.stringify(id.set.name));

// Symbol-keyed methods take "[description]".
const described = Symbol("tagName");
const empty = Symbol("");
const bare = Symbol();
const symKeyed: any = {
  [described](): number { return 1; },
  [empty](): number { return 2; },
  [bare](): number { return 3; },
};
console.log("symbol_described=" + JSON.stringify(symKeyed[described].name));
console.log("symbol_empty_desc=" + JSON.stringify(symKeyed[empty].name));
console.log("symbol_no_desc=" + JSON.stringify(symKeyed[bare].name));

// A symbol-keyed accessor combines both rules.
const accessorSym = Symbol("area");
const symAccessor: any = {
  get [accessorSym](): number { return 4; },
  set [accessorSym](v: number) { void v; },
};
const asd = Object.getOwnPropertyDescriptor(symAccessor, accessorSym) as any;
console.log("symbol_getter=" + JSON.stringify(asd.get.name));
console.log("symbol_setter=" + JSON.stringify(asd.set.name));

// A well-known symbol method.
class Iterable {
  *[Symbol.iterator](): Generator<number> { yield 1; }
}
console.log("wellknown_name=" + JSON.stringify((Iterable.prototype as any)[Symbol.iterator].name));

// Computed STRING keys take the computed value.
const dyn = "computedName";
const computed: any = { [dyn](): number { return 1; }, [dyn + "2"]: () => 2 };
console.log("computed_method=" + JSON.stringify(computed[dyn].name));
console.log("computed_arrow=" + JSON.stringify(computed[dyn + "2"].name));

// Binding positions that infer a name.
const fromConst = () => {};
let fromLet = function () {};
const { destructured = () => {} } = {} as any;
const [fromArray = function () {}] = [] as any[];
console.log("const_arrow=" + JSON.stringify(fromConst.name));
console.log("let_fn_expr=" + JSON.stringify(fromLet.name));
console.log("destructured_default=" + JSON.stringify(destructured.name));
console.log("array_default=" + JSON.stringify(fromArray.name));

// A class field holding an arrow takes the field name.
class Fielded {
  handler = () => "handled";
  static staticHandler = function () { return "static"; };
}
console.log("field_arrow=" + JSON.stringify(new Fielded().handler.name));
console.log("static_field=" + JSON.stringify((Fielded as any).staticHandler.name));

// Assignment to an ALREADY DECLARED binding does not infer a name.
let later: any;
later = () => {};
console.log("plain_assignment=" + JSON.stringify(later.name));

// A function passed as an argument or stored in an array stays anonymous.
const inArray: any[] = [() => {}, function () {}];
console.log("in_array_arrow=" + JSON.stringify(inArray[0].name));
console.log("in_array_expr=" + JSON.stringify(inArray[1].name));
function takes(fn: any): string { return JSON.stringify(fn.name); }
console.log("as_argument=" + takes(() => {}));

// A default value in a parameter list infers the parameter's name.
function paramDefault(cb: any = () => {}): string { return JSON.stringify(cb.name); }
console.log("param_default=" + paramDefault());

// An object property holding an anonymous function takes the key.
const propHolder: any = { onClick: function () {}, onKey: () => {} };
console.log("prop_fn_expr=" + JSON.stringify(propHolder.onClick.name));
console.log("prop_arrow=" + JSON.stringify(propHolder.onKey.name));

// A named function expression keeps ITS name, not the binding's.
const bindingName = function innerName(): number { return 1; };
console.log("named_expr_wins=" + JSON.stringify(bindingName.name));

// An anonymous class expression takes the binding name; a named one keeps its own.
const AnonClass = class {};
const NamedClass = class RealName {};
console.log("anon_class=" + JSON.stringify(AnonClass.name));
console.log("named_class=" + JSON.stringify(NamedClass.name));

// `name` is configurable but not writable everywhere.
const nd = Object.getOwnPropertyDescriptor(fromConst, "name") as any;
console.log("name_descriptor=" + nd.writable + "/" + nd.enumerable + "/" + nd.configurable);
