//! `JSON.parse(text, reviver)` — the reviver walks the parsed tree bottom-up;
//! a transform replaces a value, a returned `undefined` removes the property.

use super::assert_stdout;

#[test]
fn reviver_transform() {
    assert_stdout(
        "const o=JSON.parse(\"{\\\"a\\\":1,\\\"b\\\":2}\",(k:string,v:any)=>typeof v===\"number\"?v*2:v); console.log(o.a,o.b);",
        "2 4\n",
    );
}

#[test]
fn reviver_delete_returns_undefined() {
    assert_stdout(
        "const o=JSON.parse(\"{\\\"a\\\":1,\\\"c\\\":2}\",(k:string,v:any)=>k===\"c\"?undefined:v); console.log(o.a,o.c===undefined);",
        "1 true\n",
    );
}
