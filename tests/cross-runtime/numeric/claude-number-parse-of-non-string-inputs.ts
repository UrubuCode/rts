// ONE thing: Number(), parseInt() and parseFloat() over NON-string inputs.
// Number applies ToNumber; the two parsers apply ToString first and then read a
// PREFIX — which is why parseInt(0.0000005) is 5 and parseInt(1e21) is 1.

function show(label: string, v: any): void {
  let n = "";
  let pi = "";
  let pf = "";
  try { n = String(Number(v)); } catch (e) { n = "!" + (e as any).constructor.name; }
  try { pi = String(parseInt(v)); } catch (e) { pi = "!" + (e as any).constructor.name; }
  try { pf = String(parseFloat(v)); } catch (e) { pf = "!" + (e as any).constructor.name; }
  console.log(label + " | Number:" + n + " | parseInt:" + pi + " | parseFloat:" + pf);
}

// --- the famous one: ToString(5e-7) is "5e-7", so parseInt reads "5" ---
show("5e-7", 0.0000005);
show("1e-6", 0.000001);
show("1e-7", 0.0000001);
show("1.5e-9", 0.0000000015);
show("1e21", 1e21);
show("1e20", 1e20);
show("1e-21", 1e-21);
show("MAX_VALUE", Number.MAX_VALUE);
show("MIN_VALUE", Number.MIN_VALUE);
show("maxsafe", Number.MAX_SAFE_INTEGER);

// --- ordinary numbers still round-trip ---
show("255", 255);
show("neg255p9", -255.9);
show("zero", 0);
show("negzero", -0);
show("infinity", Infinity);
show("neginfinity", -Infinity);
show("nan", NaN);

// --- the nullish and boolean values split the three functions apart ---
show("null", null);
show("undefined", undefined);
show("true", true);
show("false", false);

// --- arrays go through ToString, so one element behaves like its contents ---
show("emptyarr", []);
show("arr15", [15]);
show("arr_two", [1, 2]);
show("nested", [[7]]);
show("arr_str", ["3.5abc"]);
show("arr_null", [null]);
show("arr_undef", [undefined]);
show("arr_nested_empty", [[]]);

// --- plain objects and hand-written coercion hooks ---
show("obj", {});
show("valueof8", { valueOf: function () { return 8; } });
show("tostring9p5x", { toString: function () { return "9.5x"; } });
show("both", { valueOf: function () { return 1; }, toString: function () { return "2"; } });
show("valueof_obj", { valueOf: function () { return {}; }, toString: function () { return "3.25"; } });

// --- wrappers unwrap to their primitive ---
show("Number_wrapper", new Number(3.7));
show("String_wrapper", new String("4.2xyz"));
show("Boolean_wrapper", new Boolean(true));

// --- BigInt is fine for all three, because it stringifies cleanly ---
show("bigint", 10n);
show("bigint_neg", -10n);
show("bigint_huge", 2n ** 70n);

// --- Symbol is refused by all three, at ToNumber and at ToString alike ---
show("symbol", Symbol("s"));

// --- where Number and the parsers genuinely disagree, spelled out ---
console.log("--- disagreement summary ---");
console.log("Number('')=" + Number("") + " parseInt('')=" + parseInt(""));
console.log("Number(null)=" + Number(null) + " parseInt(null)=" + parseInt(null as any));
console.log("Number([])=" + Number([]) + " parseFloat([])=" + parseFloat([] as any));
console.log("Number('0x10')=" + Number("0x10") + " parseFloat('0x10')=" + parseFloat("0x10"));
console.log("Number('12abc')=" + Number("12abc") + " parseInt('12abc')=" + parseInt("12abc"));
console.log("Number('Infinity')=" + Number("Infinity") + " parseInt('Infinity')=" + parseInt("Infinity"));
console.log("Number(' 12 ')=" + Number(" 12 ") + " parseFloat(' 12 ')=" + parseFloat(" 12 "));
console.log("Number('1_0')=" + Number("1_0") + " parseInt('1_0')=" + parseInt("1_0"));
console.log("Number_noargs=" + Number());
