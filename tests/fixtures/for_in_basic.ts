// for-in itera chaves de objeto.
import { io } from "rts";

const obj = { foo: 1, bar: 2, baz: 3 };

for (const key in obj) {
    io.print(key);
}
