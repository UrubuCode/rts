import { io } from "rts";

// new RegExp(pattern)
const re = new RegExp("hello");
io.print(re.test("say hello world") ? "test_ok" : "test_fail");
io.print(re.test("goodbye") ? "test_fail" : "no_match_ok");

// new RegExp(pattern, flags)
const reI = new RegExp("HELLO", "i");
io.print(reI.test("say hello") ? "flags_ok" : "flags_fail");

// .exec — returns matched string
const m = re.exec("say hello world");
io.print(m === "hello" ? "exec_ok" : "exec_fail");

// .exec with no match — returns handle 0 (null sentinel), truthy check
const m2 = re.exec("goodbye");
io.print(!m2 ? "exec_null_ok" : "exec_null_fail");

// .source — pattern string
const src = re.source;
io.print(src === "hello" ? "source_ok" : "source_fail");
