// Cross-runtime: structuredClone preserves cycles, shared identity, Map keys, and Set values.
const shared: any = { value: 3 };
const root: any = {
  first: shared,
  second: shared,
  map: new Map<any, any>([[shared, { ref: shared }]]),
  set: new Set<any>([shared]),
};
root.self = root;
const clone: any = structuredClone(root);
const key = [...clone.map.keys()][0];
console.log(clone !== root, clone.self === clone);
console.log(clone.first === clone.second, key === clone.first);
console.log(clone.map.get(key).ref === clone.first, clone.set.has(clone.first));

