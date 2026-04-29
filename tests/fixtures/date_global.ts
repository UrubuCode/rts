import { io } from "rts";

// Date.now() — static method
const ms = Date.now();
io.print(ms > 0 ? "now_ok" : "now_fail");

// new Date() — no-arg constructor
const d = new Date();
io.print(d.getTime() > 0 ? "getTime_ok" : "getTime_fail");

// new Date(ms) — from milliseconds
const d2 = new Date(1_000_000_000_000);
io.print(d2.getFullYear() === 2001 ? "getFullYear_ok" : "getFullYear_fail");
io.print(d2.getMonth() === 8 ? "getMonth_ok" : "getMonth_fail");

// Date.parse()
const parsed = Date.parse("2001-09-09T01:46:40.000Z");
io.print(parsed === 1_000_000_000_000 ? "parse_ok" : "parse_fail");

// toISOString
const iso = d2.toISOString();
io.print(iso === "2001-09-09T01:46:40.000Z" ? "toISOString_ok" : "toISOString_fail");
