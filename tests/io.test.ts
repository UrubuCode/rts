import test from "rts:test";
import io from "rts:io";
import fs from "rts:fs";

test.describe("rts:io — print / stdout");

test.it("io.print emits a line");
io.print("hello from rts:io");
test.pass();

test.it("io.stdout_write emits raw text");
io.stdout_write("raw stdout\n");
test.pass();

test.it("io.stderr_write emits to stderr");
io.stderr_write("stderr line\n");
test.pass();

test.describe("rts:io — Result helpers via fs");

test.it("io.is_ok returns true for successful fs.write");
const write_res = fs.write("target/__io_test__.txt", "hello");
test.assert(io.is_ok(write_res), "fs.write should succeed");
test.pass();

test.it("io.is_err returns true for missing file");
const missing = fs.read_to_string("target/__nonexistent_1234__.txt");
test.assert(io.is_err(missing), "missing file should return Err");
test.pass();

test.it("io.unwrap_or returns inner value on ok");
const read_res = fs.read_to_string("target/__io_test__.txt");
const content = io.unwrap_or(read_res, "fallback");
test.assert_eq(content, "hello", "unwrap_or on ok should return value");
test.pass();

test.it("io.unwrap_or returns fallback on err");
const fallback = io.unwrap_or(missing, "fallback_value");
test.assert_eq(fallback, "fallback_value", "unwrap_or on err should return fallback");
test.pass();
