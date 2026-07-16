// Cross-runtime: o argumento `space` do JSON.stringify (numero, string, clamp, invalidos).
// Nao imprime o texto multi-linha cru: usa JSON.stringify do resultado pra
// deixar a diferenca de indentacao visivel numa unica linha.
const obj = { a: 1, b: { c: 2 } };
const arr = [1, [2]];

// --- numero: 0..10 indenta com N espacos
console.log("space_0=" + JSON.stringify(JSON.stringify(obj, null, 0)));
console.log("space_1=" + JSON.stringify(JSON.stringify(obj, null, 1)));
console.log("space_2=" + JSON.stringify(JSON.stringify(obj, null, 2)));
console.log("space_3=" + JSON.stringify(JSON.stringify(obj, null, 3)));

// --- clamp em 10: 11, 100 e Infinity dao o mesmo que 10
const s10 = JSON.stringify(obj, null, 10);
console.log("space_10_eq_11=" + (s10 === JSON.stringify(obj, null, 11)));
console.log("space_10_eq_100=" + (s10 === JSON.stringify(obj, null, 100)));
console.log("space_10_eq_inf=" + (s10 === JSON.stringify(obj, null, Infinity)));
console.log("space_10_indent=" + JSON.stringify(s10.split("\n")[1]));

// --- negativo / NaN / zero => sem indentacao (igual a compacto)
const compact = JSON.stringify(obj);
console.log("neg_is_compact=" + (JSON.stringify(obj, null, -1) === compact));
console.log("nan_is_compact=" + (JSON.stringify(obj, null, NaN) === compact));
console.log("zero_is_compact=" + (JSON.stringify(obj, null, 0) === compact));
console.log("null_is_compact=" + (JSON.stringify(obj, null, null) === compact));
console.log("undef_is_compact=" + (JSON.stringify(obj, null, undefined) === compact));

// --- fracionario trunca (2.9 => 2)
console.log("frac_eq_2=" + (JSON.stringify(obj, null, 2.9) === JSON.stringify(obj, null, 2)));

// --- string: usada literalmente como indentacao
console.log("str_dash=" + JSON.stringify(JSON.stringify(obj, null, "--")));
console.log("str_tab=" + JSON.stringify(JSON.stringify(obj, null, "\t")));
console.log("str_empty_is_compact=" + (JSON.stringify(obj, null, "") === compact));

// --- string > 10 chars: truncada nos 10 primeiros
const long = JSON.stringify(obj, null, "0123456789ABCDEF");
console.log("str_clamp=" + JSON.stringify(long.split("\n")[1]));

// --- wrappers Number/String sao desembrulhados
console.log("num_wrapper=" + (JSON.stringify(obj, null, new Number(2)) === JSON.stringify(obj, null, 2)));
console.log("str_wrapper=" + (JSON.stringify(obj, null, new String("--")) === JSON.stringify(obj, null, "--")));

// --- tipos invalidos (bool/objeto) sao ignorados => compacto
console.log("bool_is_compact=" + (JSON.stringify(obj, null, true) === compact));
console.log("obj_is_compact=" + (JSON.stringify(obj, null, {} as any) === compact));

// --- array indentado
console.log("arr_space=" + JSON.stringify(JSON.stringify(arr, null, 1)));

// --- containers vazios NAO ganham quebra de linha
console.log("empty_obj=" + JSON.stringify(JSON.stringify({}, null, 2)));
console.log("empty_arr=" + JSON.stringify(JSON.stringify([], null, 2)));
console.log("empty_nested=" + JSON.stringify(JSON.stringify({ a: {}, b: [] }, null, 1)));

// --- escalar no topo ignora space
console.log("scalar=" + JSON.stringify(JSON.stringify(5, null, 4)));
