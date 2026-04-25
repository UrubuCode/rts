import { io, string } from "rts";

const a = "hello";
const b = "hello";
const c = "world";
io.print(`a == b: ${a == b}`);
io.print(`a == c: ${a == c}`);
io.print(`a != c: ${a != c}`);

// char_at retorna string handle
const s = "abc";
const ch = string.char_at(s, 1);
io.print(`ch == "b": ${ch == "b"}`);
io.print(`ch == "x": ${ch == "x"}`);

// Strings vazias
const e1 = "";
const e2 = "";
io.print(`empty == empty: ${e1 == e2}`);
