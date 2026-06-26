//! DYNAMIC property access on a receiver whose SHAPE is not statically proven —
//! a param, a call return, a reassigned local. The proven-shape fast paths keep
//! their constant-slot loads; only when ALL of those fail does the lowering fall
//! back to the runtime `__rtsadp_obj_get`/`_set` trampolines, which read/write a
//! property by key from the object's slot-0 shape-id (`undefined` for an absent
//! key OR a non-object receiver — JS-correct, never a fault).

use super::assert_stdout;

#[test]
fn property_read_on_object_param() {
    assert_stdout(
        r#"function getX(o: any): number { return o.x; }
           console.log(getX({ x: 42, y: 1 }));"#,
        "42\n",
    );
}

#[test]
fn property_read_on_returned_object() {
    assert_stdout(
        r#"function make(): any { return { a: 10, b: 20 }; }
           let o = make();
           console.log(o.a, o.b);"#,
        "10 20\n",
    );
}

#[test]
fn property_write_on_object_param() {
    assert_stdout(
        r#"function bump(o: any): number { o.n = o.n + 1; return o.n; }
           console.log(bump({ n: 5 }));"#,
        "6\n",
    );
}

#[test]
fn absent_property_is_undefined() {
    assert_stdout(
        r#"function get(o: any): any { return o.missing; }
           console.log(get({ x: 1 }));"#,
        "undefined\n",
    );
}

#[test]
fn computed_string_key_dynamic() {
    assert_stdout(
        r#"function get(o: any, k: string): any { return o[k]; }
           console.log(get({ hello: 7 }, "hello"));"#,
        "7\n",
    );
}

#[test]
fn in_operator_own_property() {
    // `key in obj` — own-property membership via `__rtsadp_obj_has`. Present key →
    // true, absent → false.
    assert_stdout(
        r#"const o = { value: 1 }; console.log("value" in o, "other" in o);"#,
        "true false\n",
    );
}

#[test]
fn in_operator_in_arrow_callback() {
    // The headline #83 case: `in` inside an arrow callback where the receiver is a
    // param (unproven shape) — the dynamic has-key path handles it.
    assert_stdout(
        r#"const arr: any[] = [{ value: 1 }, { other: 2 }];
           console.log(arr.map((y) => ("value" in y ? "v" : "n")).join(","));"#,
        "v,n\n",
    );
}

#[test]
fn dyn_length_on_member_result() {
    // `obj.field.length` — the field read yields an opaque Tagged value; its
    // `.length` reads dynamically (array → element count).
    assert_stdout(
        "const o = { list: [1, 2, 3] }; console.log(o.list.length);",
        "3\n",
    );
}

#[test]
fn dyn_length_on_logical_result() {
    // `(s || "default").length` — the short-circuit yields one operand; its
    // `.length` reads dynamically. Empty → "default" (7).
    assert_stdout(
        r#"const s = ""; console.log((s || "default").length);"#,
        "7\n",
    );
}

#[test]
fn dyn_length_on_index_result() {
    // `rows[i].length` — an index result's `.length` reads dynamically.
    assert_stdout(
        "const rows = [[1, 2], [3, 4, 5]]; console.log(rows[1].length);",
        "3\n",
    );
}
