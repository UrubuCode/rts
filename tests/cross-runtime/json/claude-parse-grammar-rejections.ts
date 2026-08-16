// Cross-runtime: the JSON GRAMMAR is stricter than JavaScript's — this pins the
// forms JSON.parse REFUSES (all SyntaxError) against the few surprising ones it
// accepts. Only e.constructor.name is printed; messages differ by engine.

function p(label: string, text: string): void {
  try {
    const v = JSON.parse(text);
    console.log(label + "=ok:" + (typeof v) + ":" + JSON.stringify(v));
  } catch (e: any) {
    console.log(label + "=" + e.constructor.name);
  }
}

// --- trailing and stray commas ---
p("trailing_comma_obj", '{"a":1,}');
p("trailing_comma_arr", "[1,2,]");
p("leading_comma_arr", "[,1]");
p("double_comma", "[1,,2]");

// --- quoting ---
p("single_quoted_string", "'x'");
p("single_quoted_key", "{'a':1}");
p("unquoted_key", "{a:1}");

// --- number grammar ---
p("plus_one", "+1");
p("leading_dot", ".5");
p("trailing_dot", "1.");
p("leading_zero", "01");
p("negative_leading_zero", "-01");
p("zero_dot_five", "0.5");
p("hex", "0x10");
p("octal", "0o10");
p("underscore", "1_000");
p("exponent_plus", "1e+2");
p("exponent_bare", "1e2");
p("exponent_no_digits", "1e");
p("minus_alone", "-");
p("negative_zero", "-0");

// --- named values JavaScript has but JSON does not ---
p("NaN", "NaN");
p("Infinity", "Infinity");
p("neg_Infinity", "-Infinity");
p("undefined", "undefined");
p("null_ok", "null");
p("true_ok", "true");

// --- strings: raw control characters are illegal, escapes are not ---
p("raw_newline", '"a' + String.fromCharCode(10) + 'b"');
p("raw_bell", '"a' + String.fromCharCode(7) + 'b"');
p("raw_del_is_ok", '"a' + String.fromCharCode(127) + 'b"');
p("escaped_newline", '"a\\nb"');
p("escaped_solidus", '"a\\/b"');
p("bad_escape_v", '"a\\vb"');
p("bad_escape_x", '"a\\x41b"');
p("bad_escape_zero", '"a\\0b"');
p("unterminated", '"abc');

// --- \u escapes, including a LONE surrogate, which JSON does allow ---
p("unicode_escape", '"\\u0041"');
p("lone_high_surrogate", '"\\ud800"');
p("lone_low_surrogate", '"\\udfff"');
p("surrogate_pair", '"\\ud83d\\ude00"');
p("short_unicode", '"\\u41"');
p("bad_unicode_digits", '"\\uZZZZ"');

// --- structure ---
p("comment_line", "1 // x");
p("comment_block", "/* x */ 1");
p("two_values", "1 2");
p("empty_input", "");
p("duplicate_key", '{"a":1,"a":2}');
p("colon_missing", '{"a" 1}');
p("nested_ok", '{"a":[1,{"b":null}]}');
p("empty_object", "{}");
p("bom_prefix", "﻿1");
