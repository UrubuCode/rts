import test from "rts:test";
import process from "rts:process";

test.describe("rts:process — platform / arch");

test.it("process.platform returns a string");
const platform = process.platform();
test.assert_ne(platform, "", "platform should be non-empty");
test.pass();

test.it("process.arch returns a string");
const arch = process.arch();
test.assert_ne(arch, "", "arch should be non-empty");
test.pass();

test.it("process.pid returns a positive number");
const pid = process.pid();
test.assert(pid > 0, "pid should be positive");
test.pass();

test.it("process.cwd returns a non-empty path");
const cwd = process.cwd();
test.assert_ne(cwd, "", "cwd should be non-empty");
test.pass();

test.describe("rts:process — env_set / env_get");

test.it("env round-trip works");
process.env_set("RTS_TEST_VAR", "rts_test_value");
const val = process.env_get("RTS_TEST_VAR");
test.assert_eq(val, "rts_test_value", "env round-trip should work");
test.pass();
