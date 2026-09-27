//! A dead `async` frame is not a string, and the collector must not free one
//! as if it were.
//!
//! The compiler numbered frame types from zero in its own registry, and the
//! running context numbers its string type zero in its registry. So the sweep
//! took every dead frame of the first async body for a string and freed the
//! text-slab slot its field 0 named — a resume label, i.e. a small integer,
//! i.e. the slot of one of the program's first literals. Nothing failed at that
//! moment; the next string made reused the slot, and a later `new Function`,
//! whose literals are appended to the running table, read strings belonging to
//! somebody else. Found as twelve `fetch` calls followed by a `new Function`
//! whose `"g"` came back as that Function's own source text.
//! `rts_core::entry::generator::numbering` is the fix and says why.

use rts_cranelift::tags;
use rts_host::compile;

fn number(source: &str) -> f64 {
    let mut program =
        compile(source).unwrap_or_else(|error| panic!("compiling `{source}` failed: {error:?}"));
    tags::decode_double(program.run())
}

#[test]
fn literals_compiled_after_many_parked_frames_die_read_as_written() {
    // Enough rounds that the region fills and collections run while dead
    // frames lie in it; three awaits so the label a dead frame keeps in field 0
    // reaches the slots the host's own literals were given.
    let answer = number(
        r#"const first = "alpha", second = "beta", third = "gamma", fourth = "delta";
           async function body(n) {
             await null; await null; await null;
             return ("x".repeat(64) + n).length;
           }
           for (let n = 0; n < 100000; n++) { await body(n); }
           const made = [];
           for (let n = 0; n < 1000; n++) made.push("y" + n);
           const back = new Function('return ["g", "h", "zz", "text"]')();
           return back.join(",") === "g,h,zz,text" ? 1 : 0;"#,
    );
    assert_eq!(
        answer, 1.0,
        "a `new Function` compiled after the collections must read its own \
         literals, not strings that reused a slot a dead frame freed"
    );
}
