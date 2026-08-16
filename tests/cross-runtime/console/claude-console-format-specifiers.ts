// Cross-runtime: console.log's printf-style FORMAT specifiers. Only the ones
// Bun and Node agree on are pinned here — %s/%d/%i/%f/%j/%%, the leftover-args
// rule, and the fact that an unmatched specifier is left as written.
// This is the one fixture that calls console.log with more than one argument.
// console.error is written to stderr, so it must not appear in this stdout.

// %s stringifies a primitive.
console.log("s_string=%s|", "abc");
console.log("s_number=%s|", 42);
console.log("s_null=%s|", null);
console.log("s_undefined=%s|", undefined);
console.log("s_true=%s|", true);
console.log("s_empty=%s|", "");
console.log("s_via_toString=%s|", { toString() { return "TS"; } });

// %d and %i coerce with ToNumber; %i then truncates toward zero.
console.log("d_int=%d|", 42);
console.log("d_numeric_string=%d|", "12");
console.log("d_padded_string=%d|", " 12 ");
console.log("d_empty_string=%d|", "");
console.log("d_hex_string=%d|", "0x10");
console.log("d_garbage=%d|", "xy");
console.log("d_null=%d|", null);
console.log("d_true=%d|", true);
console.log("d_undefined=%d|", undefined);
console.log("d_nan=%d|", NaN);
console.log("d_valueOf=%d|", { valueOf() { return 7; } });

console.log("i_truncates=%i|", 4.7);
console.log("i_truncates_negative=%i|", -4.7);
console.log("i_string=%i|", "-12");
console.log("i_hex=%i|", "0x10");
console.log("i_garbage=%i|", "xy");

// %f keeps the fraction.
console.log("f_float=%f|", 4.75);
console.log("f_string=%f|", "3.5");
console.log("f_leading_dot=%f|", ".5");
console.log("f_exponent=%f|", "1e3");
console.log("f_repeating=%f|", 1 / 3);
console.log("f_garbage=%f|", "xy");

// %j is JSON.
console.log("j_object=%j|", { a: 1 });
console.log("j_array=%j|", [1, "a", null]);
console.log("j_string=%j|", "str");
console.log("j_number=%j|", 5);

// %% is a literal percent — but only when there IS an argument to substitute.
console.log("pct_no_args=%%|");
console.log("pct_with_arg=%%|", "a");
console.log("pct_before_letter=%%d|", 1);
console.log("trailing_pct=100%|", "a");

// Several specifiers consume arguments left to right.
console.log("two=%s and %s|", "1", "2");
console.log("mixed=%s/%d/%f|", "x", "9", "1.5");

// Arguments left over are appended, space-separated.
console.log("leftover=%s|", "a", "extra", 1);
console.log("no_specifier=", "a", 1);

// A specifier with no argument is left exactly as written.
console.log("missing_arg=%s|");
console.log("partly_missing=%s/%s|", "a");

// An unknown specifier is not a specifier.
console.log("unknown=%z|", "a");

// No arguments at all, and an empty string, each print one empty line.
console.log();
console.log("");

// A newline inside the format string still splits the output.
console.log("first\nsecond=%s|", "tail");

// console.error goes to stderr and must NOT reach this stdout.
console.error("this line belongs to stderr, not stdout");
console.error("stderr=%s|", "also-not-here");
console.log("after_error=reached");

// console.log returns undefined.
console.log("returns=" + String(console.log()));

// A specifier in a LATER argument is plain text, not a format.
console.log("literal=%s|", "%d", 5);
console.log("done");
