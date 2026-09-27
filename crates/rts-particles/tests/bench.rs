//! Micro-benchmark de `particles_step`, `#[ignore]` — roda só sob pedido:
//!
//! ```text
//! cargo test --release -p rts-particles -- --ignored --nocapture
//! ```
//!
//! Sem `criterion` (não é dependência deste workspace): `Instant` + várias
//! repetições sobre um pool pré-populado, reportando ns/partícula — o mesmo
//! número que `kernel-report.md` mediu em ms/quadro para a referência TS,
//! só que aqui já dividido pela contagem para comparar direto com "quanto
//! custa UMA partícula" em vez de "quanto custa UM quadro a N partículas".

use rts_particles::kernel::{self, *};

fn pool_cheio(n: usize) -> Vec<f64> {
    let mut pool = vec![0.0; n * P_FLOATS];
    for slot in 0..n {
        let k = slot * P_FLOATS;
        pool[k + P_X] = (slot % 100) as f64;
        pool[k + P_Y] = 0.0;
        pool[k + P_Z] = 0.0;
        pool[k + P_VX] = 1.0;
        pool[k + P_VY] = 2.0;
        pool[k + P_VZ] = -1.0;
        pool[k + P_IDADE] = 0.0;
        pool[k + P_VIDA] = 5.0; // vida longa: nenhuma morre durante o bench
        pool[k + P_TAM0] = 1.0;
        pool[k + P_ROT] = 0.0;
        pool[k + P_COR_R] = 1.0;
        pool[k + P_COR_G] = 1.0;
        pool[k + P_COR_B] = 1.0;
        pool[k + P_COR_A] = 1.0;
    }
    pool
}

fn params_4_chaves(sort: bool) -> Vec<f64> {
    let mut p = vec![0.0; PARAMS_FLOATS];
    p[PARAMS_WIND_X] = 0.1;
    p[PARAMS_WIND_Y] = -0.2;
    p[PARAMS_DRAG] = 0.3;
    p[PARAMS_N_GRAD] = 4.0;
    for i in 0..4 {
        p[PARAMS_GRAD + i * 5] = i as f64 / 3.0;
        p[PARAMS_GRAD + i * 5 + 1] = 1.0;
        p[PARAMS_GRAD + i * 5 + 2] = 1.0;
        p[PARAMS_GRAD + i * 5 + 3] = 1.0;
        p[PARAMS_GRAD + i * 5 + 4] = 1.0 - i as f64 / 4.0;
    }
    p[PARAMS_N_SIZE] = 4.0;
    for i in 0..4 {
        p[PARAMS_SIZE_CURVE + i * 2] = i as f64 / 3.0;
        p[PARAMS_SIZE_CURVE + i * 2 + 1] = 1.0 + i as f64 * 0.2;
    }
    p[PARAMS_SIM_WORLD] = 1.0;
    p[PARAMS_SORT_MODE] = if sort { 1.0 } else { 0.0 };
    p[PARAMS_CAM_X] = 500.0;
    p
}

fn bench_um(n: usize, sort: bool, reps: u32) -> f64 {
    let mut pool = pool_cheio(n);
    let params = params_4_chaves(sort);
    let mut out = vec![0f32; n * PART_INSTANCIA_FLOATS];
    // Aquecimento.
    for _ in 0..10 {
        kernel::particles_step(&mut pool, &params, 0.0, &mut out);
    }
    let inicio = std::time::Instant::now();
    for _ in 0..reps {
        kernel::particles_step(&mut pool, &params, 0.0, &mut out);
    }
    let decorrido = inicio.elapsed();
    decorrido.as_nanos() as f64 / (reps as f64) / (n as f64)
}

#[test]
#[ignore = "micro-benchmark; rodar com --ignored --nocapture em --release"]
fn bench_particles_step_ns_por_particula() {
    for &n in &[1_000usize, 5_000, 10_000] {
        for &sort in &[false, true] {
            let reps = 200u32;
            let ns = bench_um(n, sort, reps);
            println!("n={n:<6} sort={} ns/particula={ns:.2}  ms/quadro={:.4}", sort as u32, ns * n as f64 / 1e6);
        }
    }
}
