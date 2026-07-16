// Cross-runtime: toJSON chamado dentro de estruturas aninhadas.
// Datas fixas via Date.UTC (deterministico, sem Date.now()).
const d = new Date(Date.UTC(2024, 0, 2, 3, 4, 5, 678));

// --- Date solta e dentro de estrutura
console.log("date_top=" + JSON.stringify(d));
console.log("date_in_obj=" + JSON.stringify({ when: d }));
console.log("date_in_arr=" + JSON.stringify([d]));
console.log("date_deep=" + JSON.stringify({ a: { b: [{ c: d }] } }));
console.log("date_two=" + JSON.stringify({ x: d, y: d }));

// --- toJSON recebe a CHAVE como argumento
const keySpy: any = { toJSON: function (k: any) { return "key:" + String(k); } };
console.log("key_top=" + JSON.stringify(keySpy));
console.log("key_in_obj=" + JSON.stringify({ myField: keySpy }));
console.log("key_in_arr=" + JSON.stringify([keySpy, keySpy]));
console.log("key_nested=" + JSON.stringify({ outer: { inner: keySpy } }));

// --- toJSON de Date honra a chave tambem
const dateKey = new Date(Date.UTC(2020, 5, 15));
console.log("date_iso=" + JSON.stringify({ d: dateKey }));

// --- retorno do toJSON e serializado recursivamente
const nested: any = { toJSON: function () { return { inner: d, list: [1, 2] }; } };
console.log("recursive=" + JSON.stringify(nested));

// --- toJSON que retorna outro objeto com toJSON
const chained: any = { toJSON: function () { return keySpy; } };
console.log("chained=" + JSON.stringify({ f: chained }));

// --- toJSON em classe
class Money {
  amount: number;
  constructor(a: number) { this.amount = a; }
  toJSON() { return this.amount + " BRL"; }
}
console.log("class_top=" + JSON.stringify(new Money(10)));
console.log("class_in_obj=" + JSON.stringify({ price: new Money(20) }));
console.log("class_in_arr=" + JSON.stringify([new Money(1), new Money(2)]));

// --- toJSON retornando primitivos
console.log("ret_num=" + JSON.stringify({ v: { toJSON: function () { return 42; } } }));
console.log("ret_null=" + JSON.stringify({ v: { toJSON: function () { return null; } } }));
console.log("ret_bool=" + JSON.stringify({ v: { toJSON: function () { return true; } } }));
console.log("ret_arr=" + JSON.stringify({ v: { toJSON: function () { return [1]; } } }));

// --- toJSON NAO-funcao e ignorado (serializa o objeto normalmente)
console.log("not_fn=" + JSON.stringify({ toJSON: 5, other: 1 }));

// --- toJSON herdado do prototipo funciona
function Base(this: any) { this.v = 3; }
Base.prototype.toJSON = function () { return "from_proto"; };
console.log("inherited=" + JSON.stringify({ b: new (Base as any)() }));

// --- toJSON com space (indentacao aplicada ao RESULTADO do toJSON)
console.log("with_space=" + JSON.stringify(JSON.stringify({ d: d }, null, 1)));
