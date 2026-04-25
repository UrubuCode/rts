// 3 níveis: break com label sobe direto pra um intermediário.
import { io, gc } from "rts";

let count: number = 0;
outer: for (let i = 0; i < 3; i = i + 1) {
    middle: for (let j = 0; j < 3; j = j + 1) {
        for (let k = 0; k < 3; k = k + 1) {
            if (k == 1) {
                break middle;
            }
            count = count + 1;
        }
        // não executa por causa do break middle
        count = count + 100;
    }
}

const h = gc.string_from_i64(count);
io.print(h); gc.string_free(h); // 3 iter externas × 1 inc = 3
