// throw dentro de fn deve propagar até o try/catch do caller.
// Nota: fase 1 não tem unwind real — o body continua até o ponto de
// observação (try/catch ou call site). Este teste exercita o caso onde
// o body da fn imprime ANTES do throw, e o caller observa o erro.
import { io } from "rts";

function inner(): void {
    io.print("inner-before");
    throw "boom";
    // statements após throw na fase 1 ainda executam — mas evitamos
    // testar isso; aqui o body é puro até o throw.
}

try {
    inner();
} catch (e) {
    io.print(`caught: ${e}`);
}

io.print("end");
