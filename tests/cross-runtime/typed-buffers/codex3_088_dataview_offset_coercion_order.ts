// Cross-runtime: DataView coerces byte offset before the value argument.
const seen: string[] = [];
const view = new DataView(new ArrayBuffer(8));
const offset = { valueOf() { seen.push("offset"); return 1; } };
const value = { valueOf() { seen.push("value"); return 0x1234; } };
view.setUint16(offset as any, value as any, true);
console.log(seen.join(","));
console.log(view.getUint8(1), view.getUint8(2), view.getUint16(1, true));

