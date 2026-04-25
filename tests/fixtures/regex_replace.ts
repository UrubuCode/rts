// Regex replace + replace_all.
import { io, gc, regex } from "rts";

const foo = /foo/;
const h1 = regex.replace_all(foo, "foo bar foo baz", "X");
io.print(h1); gc.string_free(h1); // X bar X baz
const h2 = regex.replace(foo, "foo and foo", "Y");
io.print(h2); gc.string_free(h2); // Y and foo
regex.free(foo);
