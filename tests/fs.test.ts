import test from "rts:test";
import fs from "rts:fs";
import io from "rts:io";

const TMP_PATH = "target/test_fs_tmp.txt";
const CONTENT = "rts:fs test content\n";

test.describe("rts:fs — write / read_to_string");

test.it("fs.write creates a file");
const write_result = fs.write(TMP_PATH, CONTENT);
test.assert(io.is_ok(write_result), "fs.write should succeed");
test.pass();

test.it("fs.read_to_string reads back written content");
const read_result = fs.read_to_string(TMP_PATH);
test.assert(io.is_ok(read_result), "fs.read_to_string should succeed");
const content = io.unwrap_or(read_result, "");
test.assert_eq(content, CONTENT, "read content should match written content");
test.pass();

test.it("fs.read_to_string fails for missing file");
const missing = fs.read_to_string("target/__nonexistent__.txt");
test.assert(io.is_err(missing), "missing file should return Err");
test.pass();
