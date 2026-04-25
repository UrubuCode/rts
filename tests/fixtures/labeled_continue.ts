// Labeled continue: pula para a próxima iteração do loop externo.
import { io, gc } from "rts";

let count: number = 0;
outer: for (let i = 0; i < 3; i = i + 1) {
    for (let j = 0; j < 3; j = j + 1) {
        if (j == 1) {
            continue outer; // pula resto da iteração interna E externa
        }
        count = count + 1;
    }
    // este código nunca é executado por causa do continue outer
    count = count + 100;
}

const h = gc.string_from_i64(count);
io.print(h); gc.string_free(h); // 3 (1 por iteração externa, count++ só com j=0)
