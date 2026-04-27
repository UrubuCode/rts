// Map-reduce paralelo: cada thread soma um shard de 0..N, main agrega.
// Demonstra retorno tipado de thread.join.
import { io, thread, time, gc } from "rts";

const N: i64 = 100_000_000;
const SHARDS: i64 = 8;
const SHARD_SIZE: i64 = N / SHARDS;

function shardSum(shard: i64): i64 {
    let sum: i64 = 0;
    let i: i64 = shard * SHARD_SIZE;
    const end: i64 = i + SHARD_SIZE;
    while (i < end) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

const fp = shardSum as unknown as number;
const t0 = time.now_ms();

const handles: number[] = [];
let s: i64 = 0;
while (s < SHARDS) {
    handles.push(thread.spawn(fp, s));
    s = s + 1;
}

let total: i64 = 0;
let i: i64 = 0;
while (i < SHARDS) {
    total = total + thread.join(handles[i]);
    i = i + 1;
}

const t1 = time.now_ms();

// soma de 0..(N-1) = N*(N-1)/2
const ht = gc.string_from_i64(total);
io.print("total:");
io.print(ht);
gc.string_free(ht);

const elapsed: i64 = t1 - t0;
const he = gc.string_from_i64(elapsed);
io.print("elapsed (ms):");
io.print(he);
gc.string_free(he);
