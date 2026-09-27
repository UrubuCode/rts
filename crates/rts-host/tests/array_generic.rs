//! The `Array.prototype` methods applied to something that is NOT an array.
//!
//! ES2025 §23.1.3 defines them over `ToObject(this)`, `LengthOfArrayLike` and
//! `HasProperty`/`Get`/`Set`/`DeletePropertyOrThrow` per index, so an
//! array-like and a `Proxy` over an array are receivers the language answers
//! for. Each test pins one answer the runtime used to get wrong — most of them
//! `undefined` — and the last pins the ORDER of the conversion, which an
//! answer alone cannot show.

use rts_cranelift::tags;
use rts_host::compile;

fn number(source: &str) -> f64 {
    let mut program = compile(source).unwrap_or_else(|e| panic!("compile failed: {e:?}"));
    tags::decode_double(program.run())
}

fn holds(source: &str) -> bool {
    number(&format!("return ({source}) ? 1 : 0;")) == 1.0
}

#[test]
fn index_of_searches_an_array_like_and_skips_its_holes() {
    assert_eq!(
        number(r#"return Array.prototype.indexOf.call({ length: 3, 0: "a", 1: "b", 2: "c" }, "b");"#),
        1.0
    );
    assert_eq!(number(r#"return Array.prototype.indexOf.call({ length: 3, 2: undefined }, undefined);"#), 2.0);
    assert_eq!(number(r#"return Array.prototype.lastIndexOf.call({ length: 3, 0: 7, 2: 7 }, 7);"#), 2.0);
}

#[test]
fn includes_on_an_array_like_reads_a_hole_as_undefined() {
    assert!(holds(r#"Array.prototype.includes.call({ length: 2, 1: 1 }, undefined)"#));
}

#[test]
fn push_and_pop_write_an_array_like_and_its_length() {
    assert!(holds(
        r#"(() => { const o: any = { length: 2, 0: "a", 1: "b" };
            const n = Array.prototype.push.call(o, "c");
            return n === 3 && o.length === 3 && o[2] === "c"; })()"#
    ));
    assert!(holds(
        r#"(() => { const o: any = { length: 2, 0: "a", 1: "b" };
            const v = Array.prototype.pop.call(o);
            return v === "b" && o.length === 1 && !(1 in o); })()"#
    ));
}

#[test]
fn reverse_moves_an_array_like_hole_to_its_mirror_position() {
    assert!(holds(
        r#"(() => { const o: any = { length: 3, 0: "a", 2: "c" };
            Array.prototype.reverse.call(o);
            return o[0] === "c" && !(1 in o) && o[2] === "a"; })()"#
    ));
}

#[test]
fn fill_writes_an_array_like_from_the_start_it_was_given() {
    assert!(holds(
        r#"(() => { const o: any = { length: 3 };
            Array.prototype.fill.call(o, "z", 1);
            return !(0 in o) && o[1] === "z" && o[2] === "z"; })()"#
    ));
}

#[test]
fn slice_of_a_proxy_is_a_real_array_that_can_join() {
    assert!(holds(
        r#"(() => { const liar: any = new Proxy([1, 2, 3, 4, 5],
              { get(t, k, r) { return k === "length" ? 2 : Reflect.get(t, k, r); } });
            const s: any = Array.prototype.slice.call(liar);
            return Array.isArray(s) && s.join(",") === "1,2"
                && Array.prototype.join.call(liar, ",") === "1,2"; })()"#
    ));
}

#[test]
fn a_nan_from_index_makes_last_index_of_look_at_position_zero_only() {
    assert_eq!(number("return [1, 2, 3, 1].lastIndexOf(1, NaN);"), 0.0);
}

#[test]
fn splice_with_string_arguments_converts_them_instead_of_panicking() {
    assert!(holds(
        r#"(() => { const a = [0, 1, 2, 3, 4]; const r = (a.splice as any)("2", "1");
            return r.length === 1 && r[0] === 2 && a.join() === "0,1,3,4"; })()"#
    ));
}

#[test]
fn an_array_like_reads_length_before_converting_from_index() {
    // The order, pinned by a counter on the side effect rather than by the
    // answer — the answer is the same either way.
    assert!(holds(
        r#"(() => { const log: string[] = [];
            const o = { get length() { log.push("length"); return 1; }, 0: "x" };
            const from = { valueOf() { log.push("from"); return 0; } };
            Array.prototype.indexOf.call(o, "x", from);
            return log.join() === "length,from"; })()"#
    ));
}

#[test]
fn a_null_receiver_is_a_type_error() {
    assert!(holds(
        r#"(() => { try { Array.prototype.indexOf.call(null, 1); return false; }
            catch (e) { return e instanceof TypeError; } })()"#
    ));
}
