// Cross-runtime: JSON.stringify de numeros edge (-0, Infinity, NaN, muito grandes).
// --- zero negativo vira "0"
console.log("neg_zero=" + JSON.stringify(-0));
console.log("neg_zero_obj=" + JSON.stringify({ a: -0 }));
console.log("neg_zero_arr=" + JSON.stringify([-0, 0]));
console.log("pos_zero=" + JSON.stringify(0));

// --- nao-finitos viram "null"
console.log("inf=" + JSON.stringify(Infinity));
console.log("neg_inf=" + JSON.stringify(-Infinity));
console.log("nan=" + JSON.stringify(NaN));
console.log("nonfinite_obj=" + JSON.stringify({ a: Infinity, b: -Infinity, c: NaN }));
console.log("nonfinite_arr=" + JSON.stringify([Infinity, -Infinity, NaN]));
console.log("div_zero=" + JSON.stringify(1 / 0));

// --- muito grandes / muito pequenos (notacao exponencial)
console.log("max_safe=" + JSON.stringify(9007199254740991));
console.log("max_safe_plus=" + JSON.stringify(9007199254740993));
console.log("max_value=" + JSON.stringify(1.7976931348623157e308));
console.log("min_value=" + JSON.stringify(5e-324));
console.log("e21=" + JSON.stringify(1e21));
console.log("e20=" + JSON.stringify(1e20));
console.log("neg_e21=" + JSON.stringify(-1e21));
console.log("e_minus_7=" + JSON.stringify(1e-7));
console.log("e_minus_6=" + JSON.stringify(0.000001));

// --- fracionarios / precisao
console.log("third=" + JSON.stringify(1 / 3));
console.log("point_sum=" + JSON.stringify(0.1 + 0.2));
console.log("big_frac=" + JSON.stringify(123456789.123456789));
console.log("neg_frac=" + JSON.stringify(-0.5));

// --- overflow por multiplicacao
console.log("overflow=" + JSON.stringify(1e308 * 10));

// --- Number wrapper e coercao
console.log("wrapper=" + JSON.stringify(new Number(5)));
console.log("wrapper_nan=" + JSON.stringify(new Number(NaN)));

// --- inteiro negativo grande
console.log("neg_big=" + JSON.stringify(-9007199254740991));
console.log("int_round=" + JSON.stringify(100));
console.log("float_int=" + JSON.stringify(100.0));
