//! Builtin behavior regression tests (part 2): Array.toSorted(cmp), String methods
//! on string variables, indexOf/lastIndexOf fromIndex, Array.join/slice/toString
//! defaults, sort(cmp) + Array.from(mapper). Normal path + a variation each.

use super::assert_stdout;

// ── Array.toSorted(cmp) — non-mutating sort with a comparator (#1692) ────────────

#[test]
fn to_sorted_with_comparator_keeps_receiver() {
    // Non-mutating: `s` is sorted descending; `a` keeps its ORIGINAL order.
    assert_stdout(
        "let a=[3,1,2]; let s=a.toSorted((x:number,y:number)=>y-x); console.log(s.join(),a.join());",
        "3,2,1 3,1,2\n",
    );
}

#[test]
fn to_sorted_default_and_chained() {
    assert_stdout("console.log([3,1,2].toSorted().join());", "1,2,3\n");
    // Chained on the (array) result.
    assert_stdout("console.log([2,1].toReversed().toSorted().join());", "1,2\n");
}

// ── String methods dispatch on a VARIABLE (proven string), not only literals (#1639)

#[test]
fn string_methods_on_string_variable() {
    assert_stdout(
        r#"let s = "Hello World";
           console.log(s.toUpperCase());
           console.log(s.toLowerCase());
           console.log(s.length);"#,
        "HELLO WORLD\nhello world\n11\n",
    );
}

#[test]
fn string_var_method_chained_and_in_call() {
    // A string var flowing through a method whose result is itself a string.
    assert_stdout(
        r#"let s = "  trim me  ";
           console.log(s.trim().toUpperCase());"#,
        "TRIM ME\n",
    );
    // A string var used as a method receiver inside another call.
    assert_stdout(
        r#"let s = "abc";
           console.log("[" + s.slice(1) + "]");"#,
        "[bc]\n",
    );
}

// ── String.indexOf / lastIndexOf with a fromIndex (#1645 / #1651) ────────────────

#[test]
fn string_index_of_from_index() {
    assert_stdout(r#"console.log("abcabc".indexOf("a", 1));"#, "3\n");
    assert_stdout(r#"console.log("abcabc".lastIndexOf("a", 2));"#, "0\n");
}

// ── Array.join() 0-arg default, slice() 0-arg, toString, toLocaleString (#1645/#1651)

#[test]
fn array_join_default_slice_tostring() {
    assert_stdout("console.log([1,2,3].join());", "1,2,3\n");
    assert_stdout("console.log([1,2,3].slice().join());", "1,2,3\n");
    assert_stdout("console.log([1,2,3].toString());", "1,2,3\n");
    assert_stdout("console.log([1,2,3].toLocaleString());", "1,2,3\n");
}

// ── Array.sort(cmp) descending + Array.from(source, mapper) (#1657 / #1688) ──────

#[test]
fn sort_descending_and_from_mapper() {
    assert_stdout(
        "let a=[1,3,2]; a.sort((x:number,y:number)=>y-x); console.log(a.join());",
        "3,2,1\n",
    );
    assert_stdout(
        "console.log(Array.from([1,2,3],(x:number)=>x*2).join());",
        "2,4,6\n",
    );
    // String source through Array.from's mapper.
    assert_stdout(
        r#"console.log(Array.from("ab",(c:string)=>c.toUpperCase()).join());"#,
        "A,B\n",
    );
}
