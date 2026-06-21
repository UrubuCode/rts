//! Array mutation / variadic / copy methods: splice, toSpliced, fill range,
//! copyWithin, push/unshift/concat (variadic), toSorted(cmp), sort(cmp),
//! join/slice/toString defaults, Array.from(source, mapper).

use super::assert_stdout;

#[test]
fn splice_remove_insert_replace() {
    assert_stdout(
        "let a=[1,2,3,4,5]; let r=a.splice(2,2); console.log(a.join(),r.join());",
        "1,2,5 3,4\n",
    );
    assert_stdout("let a=[1,2,5]; a.splice(2,0,3,4); console.log(a.join());", "1,2,3,4,5\n");
    assert_stdout("let a=[1,2,3,4]; a.splice(1,2,8,9); console.log(a.join());", "1,8,9,4\n");
}

#[test]
fn splice_chained_result_is_array() {
    assert_stdout("console.log([1,2,3,4].splice(1,2).join());", "2,3\n");
}

#[test]
fn to_spliced_keeps_receiver() {
    assert_stdout(
        "let a=[1,2,3,4]; let b=a.toSpliced(1,2,9); console.log(b.join(),a.join());",
        "1,9,4 1,2,3,4\n",
    );
}

#[test]
fn fill_range_and_copy_within() {
    assert_stdout("let a=[1,1,1,1]; a.fill(9,2); console.log(a.join());", "1,1,9,9\n");
    assert_stdout("let a=[1,1,1,1]; a.fill(7,1,3); console.log(a.join());", "1,7,7,1\n");
    assert_stdout("let a=[1,2,3,4,5]; a.copyWithin(-2); console.log(a.join());", "1,2,3,1,2\n");
}

#[test]
fn variadic_push_unshift_concat() {
    assert_stdout("let a=[1]; a.push(2,3,4); console.log(a.join());", "1,2,3,4\n");
    assert_stdout("let a=[3,4]; a.unshift(1,2); console.log(a.join());", "1,2,3,4\n");
    assert_stdout("console.log([1,2].concat([3],[4,5]).join());", "1,2,3,4,5\n");
    assert_stdout("console.log([1].concat(2,3).join());", "1,2,3\n");
}

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
    assert_stdout("console.log([2,1].toReversed().toSorted().join());", "1,2\n");
}

#[test]
fn join_default_slice_tostring() {
    assert_stdout("console.log([1,2,3].join());", "1,2,3\n");
    assert_stdout("console.log([1,2,3].slice().join());", "1,2,3\n");
    assert_stdout("console.log([1,2,3].toString());", "1,2,3\n");
    assert_stdout("console.log([1,2,3].toLocaleString());", "1,2,3\n");
}

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
    assert_stdout(
        r#"console.log(Array.from("ab",(c:string)=>c.toUpperCase()).join());"#,
        "A,B\n",
    );
}
