import { io } from "rts";

// new Error(message)
const e = new Error("something went wrong");
io.print(e.message === "something went wrong" ? "message_ok" : "message_fail");
io.print(e.name === "Error" ? "name_ok" : "name_fail");
io.print(e.toString() === "Error: something went wrong" ? "tostring_ok" : "tostring_fail");

// TypeError
const te = new TypeError("bad type");
io.print(te.name === "TypeError" ? "type_error_name_ok" : "type_error_name_fail");
io.print(te.message === "bad type" ? "type_error_msg_ok" : "type_error_msg_fail");

// RangeError
const re = new RangeError("out of range");
io.print(re.name === "RangeError" ? "range_error_ok" : "range_error_fail");

// SyntaxError
const se = new SyntaxError("unexpected token");
io.print(se.name === "SyntaxError" ? "syntax_error_ok" : "syntax_error_fail");

// toString with no message (name only)
const e2 = new Error("");
io.print(e2.toString() === "Error" ? "empty_msg_ok" : "empty_msg_fail");
