// JSON.rawJSON / JSON.isRawJSON (ES2024): validation, brand, verbatim stringify.
const J: any = JSON;
for (const s of ['123', '"str"', 'true', 'null', '1.5e3', '{}', '[]', ' 1', '1 ', '', '-0', 'false']) {
  try {
    const r = J.rawJSON(s);
    console.log(JSON.stringify(s), 'ok', JSON.stringify(r), Object.isFrozen(r), Object.getPrototypeOf(r), Object.keys(r), r.rawJSON, J.isRawJSON(r));
  } catch (e: any) {
    console.log(JSON.stringify(s), e.name);
  }
}
const r = J.rawJSON('1e1000');
console.log(JSON.stringify({ a: r, b: [r, J.rawJSON('"x"')], c: 1 }));
console.log(JSON.stringify(r), JSON.stringify([J.rawJSON('null')], null, 2));
console.log(J.isRawJSON({}), J.isRawJSON({ rawJSON: '1' }), J.isRawJSON(1), J.isRawJSON(null), J.isRawJSON());
console.log(J.rawJSON(123).rawJSON);
const fake = Object.freeze(Object.assign(Object.create(null), { rawJSON: '1' }));
console.log(J.isRawJSON(fake), JSON.stringify({ f: fake }));
const o = J.rawJSON('"q"');
console.log(JSON.stringify(Object.getOwnPropertyDescriptor(o, 'rawJSON')), Object.isFrozen(o), o instanceof Object, typeof o);
console.log(JSON.stringify({ a: { b: [J.rawJSON('12345678901234567890')] } }, null, 2));
console.log(JSON.stringify({ a: r }, (k, v) => v));
console.log(typeof J.rawJSON, J.rawJSON.length, J.isRawJSON.length, J.rawJSON.name);
