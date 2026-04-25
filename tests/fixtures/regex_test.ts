// Regex literal + test().
import { io, gc, regex } from "rts";

const re = /^[a-z]+@[a-z]+\.[a-z]+$/i;
const ok = regex.test(re, "USER@MAIL.COM") ? 1 : 0;
const bad = regex.test(re, "not-an-email") ? 1 : 0;
const h1 = gc.string_from_i64(ok);
io.print(h1); gc.string_free(h1); // 1
const h2 = gc.string_from_i64(bad);
io.print(h2); gc.string_free(h2); // 0
regex.free(re);
