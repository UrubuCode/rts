// Threads constroem objetos, main recebe instâncias e usa.
// Padrão "factory pool" — útil quando inicialização é cara
// (parsing, conexões, caches preenchidos).
import { io, thread, atomic, gc } from "rts";

class Worker {
    id: i64;
    items: i64;
    constructor(id: i64) {
        this.id = atomic.i64_new(id);
        this.items = atomic.i64_new(0);
    }
    process(n: i64): void {
        atomic.i64_fetch_add(this.items, n);
    }
    summary(): i64 {
        return atomic.i64_load(this.id) * 1000 + atomic.i64_load(this.items);
    }
}

function buildWorker(id: i64): i64 {
    // Cada thread constrói seu próprio Worker e retorna o handle.
    const w = new Worker(id);
    // Inicialização cara: simulada com loops de processamento.
    let i: i64 = 0;
    while (i < 10000) {
        w.process(1);
        i = i + 1;
    }
    return w as unknown as i64;
}

const fp = buildWorker as unknown as number;
const handles: number[] = [];
let s: i64 = 1;
while (s <= 4) {
    handles.push(thread.spawn(fp, s));
    s = s + 1;
}

// Main recebe instâncias prontas e agrega.
let total: i64 = 0;
let i = 0;
while (i < 4) {
    const w_handle = thread.join(handles[i]);
    const w: Worker = w_handle as unknown as Worker;
    total = total + w.summary();
    i = i + 1;
}

// Cada Worker n contribui n*1000 + 10000:
//   1000+10000 + 2000+10000 + 3000+10000 + 4000+10000 = 50000
const hv = gc.string_from_i64(total);
io.print("agregado:");
io.print(hv);
gc.string_free(hv);
