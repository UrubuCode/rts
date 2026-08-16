// Cross-runtime: RegExp construction clones source and can replace flags.
const original = /a+/gi;
original.lastIndex = 3;
const same = new RegExp(original);
const changed = new RegExp(original, "m");
console.log(same.source, same.flags, same.lastIndex);
console.log(changed.source, changed.flags, changed.lastIndex);

