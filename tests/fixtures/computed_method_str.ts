// Computed method name com literal string: `["foo"]() {}` ≡ `foo() {}`
import { io, gc } from "rts";

class C {
    ["greet"](): string {
        return "hello";
    }
    ["double"](n: number): number {
        return n * 2;
    }
}

const c = new C();
io.print(c.greet()); // hello
const h = gc.string_from_i64(c.double(7));
io.print(h); gc.string_free(h); // 14
