// Cross-runtime: computed field keys evaluate at class definition, values per instance.
const seen: string[] = [];
const key = () => { seen.push("key"); return "value"; };
const init = () => { seen.push("init"); return seen.length; };
class Example {
  [key()] = init();
}
console.log(seen.join(","));
const a: any = new Example();
const b: any = new Example();
console.log(a.value, b.value);
console.log(seen.join(","));

