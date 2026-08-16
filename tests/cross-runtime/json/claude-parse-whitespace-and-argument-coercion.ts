// Cross-runtime: JSON.parse coerces its FIRST argument with ToString before
// parsing, and accepts exactly four whitespace characters — tab, LF, CR and
// space. Everything JavaScript also treats as whitespace is a SyntaxError here.

function p(label: string, text: any): void {
  try {
    const v = JSON.parse(text);
    console.log(label + "=ok:" + JSON.stringify(v));
  } catch (e: any) {
    console.log(label + "=" + e.constructor.name);
  }
}

// --- the four legal whitespace characters, before and after the value ---
p("space", " 1 ");
p("tab", "\t1\t");
p("lf", "\n1\n");
p("cr", "\r1\r");
p("mixed", " \t\r\n [ 1 , 2 ] \n\t ");
p("inside_object", '{ "a" : 1 , "b" : 2 }');

// --- everything else JavaScript calls whitespace is refused ---
function ws(label: string, code: number): void {
  p(label, String.fromCharCode(code) + "1");
}
ws("vertical_tab", 0x0b);
ws("form_feed", 0x0c);
ws("nbsp", 0x00a0);
ws("bom", 0xfeff);
ws("line_separator", 0x2028);
ws("paragraph_separator", 0x2029);
ws("ideographic_space", 0x3000);
ws("null_char", 0x0000);

// --- whitespace inside a string literal is kept, not trimmed ---
p("string_with_spaces", '"  a  "');
p("string_only_space", '" "');

// --- the first argument goes through ToString ---
p("number_arg", 1);
p("float_arg", 1.5);
p("true_arg", true);
p("false_arg", false);
p("null_arg", null);
p("undefined_arg", undefined);
p("array_one_number", [1]);
p("array_empty", []);
p("array_two", [1, 2]);
p("nested_array_arg", [[1]]);
p("plain_object_arg", {});

// --- an object whose toString produces valid JSON ---
p("tostring_object", { toString() { return '{"from":"toString"}'; } } as any);
p("valueof_ignored", { valueOf() { return "1"; }, toString() { return "2"; } } as any);

// --- a Symbol.toPrimitive hook drives the coercion ---
p("toprimitive_hook", { [Symbol.toPrimitive](h: any) { return h === "string" ? '"str"' : '"other"'; } } as any);

// --- a symbol argument cannot be coerced ---
try { JSON.parse(Symbol("s") as any); console.log("symbol_arg=no_throw"); }
catch (e: any) { console.log("symbol_arg=" + e.constructor.name); }

// --- a String wrapper object works ---
p("string_object", new String("[1,2]") as any);

// --- the returned value is a fresh object each call ---
const a = JSON.parse("[1,2]");
const b = JSON.parse("[1,2]");
console.log("fresh=" + (a === b) + ":" + JSON.stringify(a));
console.log("proto_of_array=" + (Object.getPrototypeOf(a) === Array.prototype));
console.log("proto_of_object=" + (Object.getPrototypeOf(JSON.parse("{}")) === Object.prototype));

// --- shape of parse itself ---
console.log("parse_length=" + JSON.parse.length + ":" + JSON.parse.name);
console.log("json_typeof=" + (typeof (JSON as any)));
try { (JSON as any)(); console.log("call_json=no_throw"); }
catch (e: any) { console.log("call_json=" + e.constructor.name); }
try { new (JSON.parse as any)("1"); console.log("new_parse=no_throw"); }
catch (e: any) { console.log("new_parse=" + e.constructor.name); }
const jd: any = Object.getOwnPropertyDescriptor(JSON, "parse");
console.log("parse_flags=" + jd.writable + ":" + jd.enumerable + ":" + jd.configurable);
console.log("json_tag=" + Object.prototype.toString.call(JSON));
