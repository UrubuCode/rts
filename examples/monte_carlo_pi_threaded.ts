// Monte Carlo π paralelo via thread.spawn (#206 corrigido).
//
// Cada worker computa uma fatia de POINTS_TOTAL e faz UM fetch_add
// no final (minimiza contention). Usa math.random_f64() — o RNG
// global é compartilhado, mas isso é OK pra Monte Carlo: a
// concorrência adiciona um pouco mais de "aleatoriedade" benigna.

import { thread, atomic, math, time, gc, io } from "rts";

const N_WORKERS = 8;
const POINTS_PER_WORKER = 1_250_000;

const totalCounter = atomic.i64_new(0);
const insideCounter = atomic.i64_new(0);

function worker(arg: number): void {
    let local_inside = 0.0;
    let i = 0;
    while (i < POINTS_PER_WORKER) {
        const x = math.random_f64() * 2.0 - 1.0;
        const y = math.random_f64() * 2.0 - 1.0;
        if ((x * x + y * y) <= 1.0) {
            local_inside = local_inside + 1.0;
        }
        i = i + 1;
    }

    atomic.i64_fetch_add(insideCounter, local_inside as number);
    atomic.i64_fetch_add(totalCounter, POINTS_PER_WORKER);
}

const fp = worker as unknown as number;

io.print("Monte Carlo π paralelo (RTS)");
io.print("workers: 8 / pontos/worker: 1250000 / total: 10000000");

const t0 = time.now_ms();

const handles: number[] = [];
let i = 0;
while (i < N_WORKERS) {
    handles.push(thread.spawn(fp, i));
    i = i + 1;
}

i = 0;
while (i < N_WORKERS) {
    thread.join(handles[i]);
    i = i + 1;
}

const t1 = time.now_ms();
const elapsed = (t1 - t0) as number;

const total = atomic.i64_load(totalCounter);
const inside = atomic.i64_load(insideCounter);
const totalF = (total as number) * 1.0;
const insideF = (inside as number) * 1.0;
const pi = 4.0 * insideF / totalF;
const err = math.abs_f64(pi - math.PI);

const ht = gc.string_from_i64(total);
io.print("total:"); io.print(ht); gc.string_free(ht);

const hi = gc.string_from_i64(inside);
io.print("inside:"); io.print(hi); gc.string_free(hi);

const hp = gc.string_from_f64(pi);
io.print("π ≈"); io.print(hp); gc.string_free(hp);

const he = gc.string_from_f64(err);
io.print("erro:"); io.print(he); gc.string_free(he);

const hms = gc.string_from_i64(elapsed);
io.print("tempo (ms):"); io.print(hms); gc.string_free(hms);
