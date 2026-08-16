// Cross-runtime: JSON.rawJSON — a box holding already-formed JSON text that
// stringify embeds VERBATIM, so a value no JavaScript number can hold survives
// a round trip. JSON.isRawJSON is the only way to recognise one, because the
// box is defined by an internal slot rather than by its shape.

// --- the shape of the box ---
const raw: any = (JSON as any).rawJSON("123");
console.log("typeof=" + typeof raw);
console.log("is_raw=" + (JSON as any).isRawJSON(raw));
console.log("proto=" + String(Object.getPrototypeOf(raw)));
console.log("own_keys=" + Reflect.ownKeys(raw).map(String).join(","));
console.log("rawJSON_value=" + JSON.stringify(raw.rawJSON));
console.log("frozen=" + Object.isFrozen(raw) + ":sealed=" + Object.isSealed(raw));
console.log("extensible=" + Object.isExtensible(raw));
const d: any = Object.getOwnPropertyDescriptor(raw, "rawJSON");
console.log("prop_flags=" + d.writable + ":" + d.enumerable + ":" + d.configurable);
console.log("write_refused=" + Reflect.set(raw, "rawJSON", "999") + ":still=" + raw.rawJSON);
console.log("add_refused=" + Reflect.defineProperty(raw, "extra", { value: 1 }));

// --- isRawJSON needs the slot, not the shape ---
console.log("lookalike=" + (JSON as any).isRawJSON({ rawJSON: "123" }));
console.log("frozen_lookalike=" + (JSON as any).isRawJSON(Object.freeze(Object.assign(Object.create(null), { rawJSON: "123" }))));
console.log("proxy_of_raw=" + (JSON as any).isRawJSON(new Proxy(raw, {})));
console.log("string=" + (JSON as any).isRawJSON("123"));
console.log("number=" + (JSON as any).isRawJSON(123));
console.log("null=" + (JSON as any).isRawJSON(null));
console.log("undefined=" + (JSON as any).isRawJSON(undefined));
console.log("no_arg=" + (JSON as any).isRawJSON());
console.log("array=" + (JSON as any).isRawJSON([]));

// --- the API metadata ---
console.log("rawJSON_fn=" + typeof (JSON as any).rawJSON + ":" + (JSON as any).rawJSON.name + ":" + (JSON as any).rawJSON.length);
console.log("isRawJSON_fn=" + typeof (JSON as any).isRawJSON + ":" + (JSON as any).isRawJSON.name + ":" + (JSON as any).isRawJSON.length);

// --- stringify embeds the text with no quoting and no re-parsing ---
console.log("top_level=" + JSON.stringify(raw));
console.log("in_object=" + JSON.stringify({ n: raw }));
console.log("in_array=" + JSON.stringify([raw, raw]));
console.log("nested=" + JSON.stringify({ a: { b: [raw] } }));
console.log("with_indent=" + JSON.stringify(JSON.stringify({ n: raw }, null, 2)));

// --- a precision-preserving big number, which a JS number would round ---
const big = "12345678901234567890";
console.log("bigint_via_raw=" + JSON.stringify({ id: (JSON as any).rawJSON(big) }));
console.log("bigint_via_number=" + JSON.stringify({ id: Number(big) }));
console.log("round_trip_text=" + (JSON.stringify({ id: (JSON as any).rawJSON(big) }) === '{"id":' + big + "}"));

// --- every primitive JSON form is accepted ---
function make(text: string): string {
  try { return "ok:" + JSON.stringify({ v: (JSON as any).rawJSON(text) }); }
  catch (e: any) { return e.constructor.name; }
}
console.log("raw_number=" + make("1"));
console.log("raw_negative=" + make("-1.5e10"));
console.log("raw_exponent=" + make("1E1000"));
console.log("raw_string=" + make('"abc"'));
console.log("raw_escaped_string=" + make('"a\\u0041b"'));
console.log("raw_true=" + make("true"));
console.log("raw_false=" + make("false"));
console.log("raw_null=" + make("null"));

// --- but structures and malformed text are refused ---
console.log("raw_object=" + make('{"a":1}'));
console.log("raw_array=" + make("[1]"));
console.log("raw_empty=" + make(""));
console.log("raw_space_before=" + make(" 1"));
console.log("raw_space_after=" + make("1 "));
console.log("raw_newline=" + make("\n1"));
console.log("raw_tab_only=" + make("\t"));
console.log("raw_undefined_text=" + make("undefined"));
console.log("raw_NaN=" + make("NaN"));
console.log("raw_Infinity=" + make("Infinity"));
console.log("raw_leading_plus=" + make("+1"));
console.log("raw_leading_zero=" + make("01"));
console.log("raw_hex=" + make("0x1"));
console.log("raw_single_quotes=" + make("'a'"));
console.log("raw_trailing_comma=" + make("1,"));
console.log("raw_two_values=" + make("1 2"));

// --- the argument is coerced to a string first ---
console.log("raw_from_number=" + (function () { try { return JSON.stringify({ v: (JSON as any).rawJSON(1) }); } catch (e: any) { return e.constructor.name; } })());
console.log("raw_from_bigint=" + (function () { try { return JSON.stringify({ v: (JSON as any).rawJSON(10n) }); } catch (e: any) { return e.constructor.name; } })());
console.log("raw_from_toString=" + (function () { try { return JSON.stringify({ v: (JSON as any).rawJSON({ toString() { return "7"; } }) }); } catch (e: any) { return e.constructor.name; } })());
console.log("raw_from_undefined=" + (function () { try { return JSON.stringify({ v: (JSON as any).rawJSON(undefined) }); } catch (e: any) { return e.constructor.name; } })());
console.log("raw_from_symbol=" + (function () { try { return JSON.stringify({ v: (JSON as any).rawJSON(Symbol("s")) }); } catch (e: any) { return e.constructor.name; } })());
console.log("raw_no_arg=" + (function () { try { return String((JSON as any).rawJSON()); } catch (e: any) { return e.constructor.name; } })());

// --- each call makes a distinct box ---
console.log("distinct=" + ((JSON as any).rawJSON("1") === (JSON as any).rawJSON("1")));
console.log("as_map_key=" + new Set([(JSON as any).rawJSON("1"), (JSON as any).rawJSON("1")]).size);

// --- a replacer may hand one back, and toJSON may return one ---
console.log("from_replacer=" + JSON.stringify({ a: 1 }, (k, v) => (k === "a" ? (JSON as any).rawJSON("42") : v)));
console.log("from_toJSON=" + JSON.stringify({ a: { toJSON() { return (JSON as any).rawJSON("[1,2]".length === 5 ? "5" : "0"); } } }));

// --- a box is not otherwise special: it has no toJSON, and is not iterable ---
console.log("has_toJSON=" + ("toJSON" in raw));
console.log("string_coercion=" + (function () { try { return String(raw); } catch (e: any) { return e.constructor.name; } })());
console.log("json_of_lookalike=" + JSON.stringify({ v: { rawJSON: "123" } }));

// --- and parse never produces one ---
console.log("parse_makes_raw=" + (JSON as any).isRawJSON(JSON.parse("123")));
console.log("parse_object_raw=" + (JSON as any).isRawJSON(JSON.parse('{"a":1}')));
