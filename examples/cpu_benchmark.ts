// CPU Benchmark — roda 60s e mede o desempenho do processador.
//
// Estratégia: cada worker executa uma workload mista (aritmética
// inteira + ponto-flutuante + branchy code) em loops apertados,
// contando "ops" via local counter. A cada ~100ms verifica o
// relógio compartilhado pra decidir se ainda continua.
//
// O loop de medição roda em N_WORKERS threads paralelas.
// Métricas finais:
//  - total ops executadas
//  - ops/s agregado (throughput)
//  - ops/s por core (paralelismo efetivo)
//  - π estimado (sanidade do trabalho — não é só busy loop)

import { thread, atomic, math, time, gc, io } from "rts";

const N_WORKERS = 8;
const DURATION_MS = 60_000;
const CHECK_EVERY = 1_000_000; // checa relógio a cada 1M ops

// estado compartilhado
const totalOps = atomic.i64_new(0);
const insideCount = atomic.i64_new(0);
const totalCount = atomic.i64_new(0);
// flag de parada — workers leem; main escreve quando 60s expiram
const stopFlag = atomic.i64_new(0);

function worker(seed: number): void {
    let local_ops = 0.0;
    let local_inside = 0.0;
    let local_total = 0.0;

    // workload mista: pi via Monte Carlo (FP) + acumulador (INT)
    while (true) {
        let i = 0;
        while (i < CHECK_EVERY) {
            // Monte Carlo step (4 FP ops + branch)
            const x = math.random_f64() * 2.0 - 1.0;
            const y = math.random_f64() * 2.0 - 1.0;
            if ((x * x + y * y) <= 1.0) {
                local_inside = local_inside + 1.0;
            }
            local_total = local_total + 1.0;

            // mais alguns FP ops pra encher o pipeline
            const s = math.sqrt(x * x + y * y + 1.0);
            local_ops = local_ops + s;

            i = i + 1;
        }

        // flush parcial pra main observar progresso (a cada 1M ops)
        atomic.i64_fetch_add(totalOps, CHECK_EVERY);

        // checa se main pediu pra parar
        if (atomic.i64_load(stopFlag) != 0) {
            break;
        }
    }

    atomic.i64_fetch_add(insideCount, local_inside as number);
    atomic.i64_fetch_add(totalCount, local_total as number);
}

const fp = worker as unknown as number;

io.print("=== RTS CPU Benchmark ===");
io.print("workers: 8 / duracao: 60s / workload: Monte Carlo + sqrt");
io.print("");
io.print("rodando...");

const t0 = time.now_ms();

const handles: number[] = [];
let i = 0;
while (i < N_WORKERS) {
    handles.push(thread.spawn(fp, i + 1));
    i = i + 1;
}

// loop de monitoramento: imprime progresso a cada 5s, sinaliza stop em 60s
let elapsed = 0;
let nextReport = 5000;
while (elapsed < DURATION_MS) {
    thread.sleep_ms(500);
    const now = time.now_ms();
    elapsed = (now - t0) as number;
    if (elapsed >= nextReport) {
        const ops = atomic.i64_load(totalOps);
        const opsF = (ops as number) * 1.0;
        const elapsedS = (elapsed as number) / 1000.0;
        const opsPerSec = opsF / elapsedS;
        const ho = gc.string_from_i64(ops);
        const hr = gc.string_from_f64(opsPerSec / 1_000_000.0);
        const ht = gc.string_from_i64(elapsed);
        io.print("[t=");
        io.print(ht);
        io.print("ms] ops=");
        io.print(ho);
        io.print("  M ops/s=");
        io.print(hr);
        gc.string_free(ho);
        gc.string_free(hr);
        gc.string_free(ht);
        nextReport = nextReport + 5000;
    }
}

// sinaliza fim e aguarda workers (eles terminam o batch atual + checam flag)
atomic.i64_store(stopFlag, 1);

i = 0;
while (i < N_WORKERS) {
    thread.join(handles[i]);
    i = i + 1;
}

const t1 = time.now_ms();
const realElapsed = (t1 - t0) as number;

const ops = atomic.i64_load(totalOps);
const inside = atomic.i64_load(insideCount);
const totalSamples = atomic.i64_load(totalCount);

const opsF = (ops as number) * 1.0;
const elapsedS = (realElapsed as number) / 1000.0;
const opsPerSec = opsF / elapsedS;
const opsPerCore = opsPerSec / (N_WORKERS as number);

const insideF = (inside as number) * 1.0;
const totalF = (totalSamples as number) * 1.0;
const pi = 4.0 * insideF / totalF;
const piErr = math.abs_f64(pi - math.PI);

io.print("");
io.print("=== Resultado ===");

const hms = gc.string_from_i64(realElapsed);
io.print("tempo (ms):"); io.print(hms); gc.string_free(hms);

const ho = gc.string_from_i64(ops);
io.print("total ops:"); io.print(ho); gc.string_free(ho);

const hr = gc.string_from_f64(opsPerSec / 1_000_000.0);
io.print("M ops/s (agregado):"); io.print(hr); gc.string_free(hr);

const hc = gc.string_from_f64(opsPerCore / 1_000_000.0);
io.print("M ops/s por core:"); io.print(hc); gc.string_free(hc);

const hs = gc.string_from_i64(totalSamples);
io.print("amostras Monte Carlo:"); io.print(hs); gc.string_free(hs);

const hp = gc.string_from_f64(pi);
io.print("π estimado:"); io.print(hp); gc.string_free(hp);

const he = gc.string_from_f64(piErr);
io.print("erro π:"); io.print(he); gc.string_free(he);
