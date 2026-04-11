// JavaScript variant — same tests, no type annotations.
import test from "rts:test";
import io from "rts:io";
import fs from "rts:fs";

test.describe("rts:io (JS) — print");

test.it("io.print works from .js");
io.print("hello from js");
test.pass();

test.describe("rts:io (JS) — Result helpers");

test.it("is_ok / is_err via real fs results");
const write_ok = fs.write("target/__js_io_test__.txt", "js content");
const missing = fs.read_to_string("target/__nonexistent_js__.txt");
test.assert(io.is_ok(write_ok), "is_ok on ok result");
test.assert(io.is_err(missing), "is_err on err result");
test.pass();

test.it("unwrap_or works from .js");
const read_ok = fs.read_to_string("target/__js_io_test__.txt");
const val = io.unwrap_or(read_ok, "fallback");
test.assert_eq(val, "js content", "unwrap_or returns value");
const fallback = io.unwrap_or(missing, "fb");
test.assert_eq(fallback, "fb", "unwrap_or returns fallback");
test.pass();
