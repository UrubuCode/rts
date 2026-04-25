// Regex find_at + match_count.
import { io, gc, regex } from "rts";

const word = /[0-9]+/;
const idx = regex.find_at(word, "abc 123 def 456");
const cnt = regex.match_count(word, "abc 123 def 456");
const h1 = gc.string_from_i64(idx);
io.print(h1); gc.string_free(h1); // 4
const h2 = gc.string_from_i64(cnt);
io.print(h2); gc.string_free(h2); // 2
regex.free(word);
