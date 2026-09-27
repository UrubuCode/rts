//! Testes de paridade: `kernel::particles_step` contra um PORTO INDEPENDENTE
//! da referência TS (`referencia` abaixo), escrito separadamente — em VÁRIAS
//! passadas sobre o pool, como `sim.ts`/`curvas.ts`/`particlesystem.ts` fazem
//! de verdade — para pegar um defeito que a FUSÃO num laço só (a otimização
//! do kernel) poderia introduzir sem que os dois lados compartilhem o mesmo
//! bug por acidente de copiar o mesmo código.
//!
//! Cobre: `sim.ts::atualizarVidas`, `curvas.ts::aplicarVelocidade`, o laço de
//! integração de posição no fim de `particlesystem.ts::update`, e o laço de
//! `drawSelf` (gradiente/curva/buffer de 9 floats, sem o corte de frustum —
//! fora de escopo do kernel). Tolerância `1e-5` (f32 vs f64 na escrita final).

use rts_particles::kernel::{self, *};

/// Um porto independente e DELIBERADAMENTE não-otimizado (várias passadas,
/// como a referência TS) das mesmas quatro funções — ver o header do
/// arquivo. `pool`/`out` mutados como em `kernel::particles_step`; devolve a
/// contagem de vivas escritas em `out` (não compactado por sort — quem quiser
/// comparar sort chama `referencia_ordenar` à parte, como o kernel faz).
fn referencia(pool: &mut [f64], params: &[f64], dt: f64, out: &mut [f32]) -> usize {
    let dt = if dt.is_finite() && dt >= 0.0 { dt } else { 0.0 };
    let max = pool.len() / P_FLOATS;

    // Passada 1: atualizarVidas.
    for slot in 0..max {
        let k = slot * P_FLOATS;
        if pool[k + P_VIDA] >= 0.0 {
            pool[k + P_IDADE] += dt;
            if pool[k + P_IDADE] >= pool[k + P_VIDA] {
                pool[k + P_VIDA] = -1.0;
            }
        }
    }

    // Passada 2: aplicarVelocidade (vento + arrasto exponencial).
    let arrasto = params[PARAMS_DRAG];
    let arrasto_efetivo = if arrasto > 0.0 { arrasto } else { 0.0 };
    let f_arrasto = (-arrasto_efetivo * dt).exp();
    for slot in 0..max {
        let k = slot * P_FLOATS;
        if pool[k + P_VIDA] >= 0.0 {
            pool[k + P_VX] = (pool[k + P_VX] + params[PARAMS_WIND_X] * dt) * f_arrasto;
            pool[k + P_VY] = (pool[k + P_VY] + params[PARAMS_WIND_Y] * dt) * f_arrasto;
            pool[k + P_VZ] = (pool[k + P_VZ] + params[PARAMS_WIND_Z] * dt) * f_arrasto;
        }
    }

    // Passada 3: integração de posição.
    for slot in 0..max {
        let k = slot * P_FLOATS;
        if pool[k + P_VIDA] >= 0.0 {
            pool[k + P_X] += pool[k + P_VX] * dt;
            pool[k + P_Y] += pool[k + P_VY] * dt;
            pool[k + P_Z] += pool[k + P_VZ] * dt;
        }
    }

    // Passada 4: drawSelf (gradiente/curva + buffer de 9 floats, compactado).
    let n_grad = params[PARAMS_N_GRAD].max(0.0) as usize;
    let n_size = params[PARAMS_N_SIZE].max(0.0) as usize;
    let gradiente = &params[PARAMS_GRAD..PARAMS_GRAD + 20];
    let curva = &params[PARAMS_SIZE_CURVE..PARAMS_SIZE_CURVE + 8];
    let soma_pos = params[PARAMS_SIM_WORLD] != 0.0;
    let (px, py, pz) = if soma_pos { (params[PARAMS_POS_X], params[PARAMS_POS_Y], params[PARAMS_POS_Z]) } else { (0.0, 0.0, 0.0) };

    let mut n = 0usize;
    for slot in 0..max {
        let k = slot * P_FLOATS;
        if pool[k + P_VIDA] < 0.0 {
            continue;
        }
        let vida = pool[k + P_VIDA];
        let idade = pool[k + P_IDADE];
        let t = if vida > 0.0 { idade / vida } else { 1.0 };
        let cor = referencia_gradiente(gradiente, n_grad, t);
        let escala = referencia_curva(curva, n_size, t);
        let o = n * PART_INSTANCIA_FLOATS;
        out[o] = (px + pool[k + P_X]) as f32;
        out[o + 1] = (py + pool[k + P_Y]) as f32;
        out[o + 2] = (pz + pool[k + P_Z]) as f32;
        out[o + 3] = (pool[k + P_TAM0] * escala) as f32;
        out[o + 4] = pool[k + P_ROT] as f32;
        out[o + 5] = cor[0];
        out[o + 6] = cor[1];
        out[o + 7] = cor[2];
        out[o + 8] = cor[3];
        n += 1;
    }
    n
}

fn referencia_gradiente(chaves: &[f64], n_chaves: usize, t: f64) -> [f32; 4] {
    if n_chaves <= 1 {
        return [chaves[1] as f32, chaves[2] as f32, chaves[3] as f32, chaves[4] as f32];
    }
    let mut i = 0usize;
    while i < n_chaves - 1 && chaves[(i + 1) * 5] < t {
        i += 1;
    }
    if i >= n_chaves - 1 {
        i = n_chaves - 2;
    }
    let t0 = chaves[i * 5];
    let t1 = chaves[(i + 1) * 5];
    let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    let mut out = [0f32; 4];
    for c in 0..4 {
        let a = chaves[i * 5 + 1 + c];
        let b = chaves[(i + 1) * 5 + 1 + c];
        out[c] = (a + (b - a) * f) as f32;
    }
    out
}

fn referencia_curva(chaves: &[f64], n_chaves: usize, t: f64) -> f64 {
    if n_chaves <= 1 {
        return chaves[1];
    }
    let mut i = 0usize;
    while i < n_chaves - 1 && chaves[(i + 1) * 2] < t {
        i += 1;
    }
    if i >= n_chaves - 1 {
        i = n_chaves - 2;
    }
    let t0 = chaves[i * 2];
    let t1 = chaves[(i + 1) * 2];
    let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    let a = chaves[i * 2 + 1];
    let b = chaves[(i + 1) * 2 + 1];
    a + (b - a) * f
}

/// Um gerador congruente linear simples e determinístico — sem depender de
/// `rand` (não é dependência deste crate), reprodutível entre corridas.
struct Lcg(u64);
impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }
}

fn pool_aleatorio(n: usize, seed: u64) -> Vec<f64> {
    let mut rng = Lcg(seed);
    let mut pool = vec![0.0; n * P_FLOATS];
    for slot in 0..n {
        let k = slot * P_FLOATS;
        // ~85% vivas, com idade/vida variados (incluindo perto de morrer).
        if rng.next_f64() < 0.85 {
            let vida = rng.range(0.1, 3.0);
            pool[k + P_X] = rng.range(-50.0, 50.0);
            pool[k + P_Y] = rng.range(-50.0, 50.0);
            pool[k + P_Z] = rng.range(-50.0, 50.0);
            pool[k + P_VX] = rng.range(-5.0, 5.0);
            pool[k + P_VY] = rng.range(-5.0, 5.0);
            pool[k + P_VZ] = rng.range(-5.0, 5.0);
            pool[k + P_IDADE] = rng.range(0.0, vida * 1.2); // pode já estar "morta"
            pool[k + P_VIDA] = vida;
            pool[k + P_TAM0] = rng.range(0.05, 2.0);
            pool[k + P_ROT] = rng.range(0.0, 6.28);
            pool[k + P_COR_R] = 1.0;
            pool[k + P_COR_G] = 1.0;
            pool[k + P_COR_B] = 1.0;
            pool[k + P_COR_A] = 1.0;
        } else {
            pool[k + P_VIDA] = -1.0;
        }
    }
    pool
}

fn params_aleatorios(seed: u64, n_grad: usize, n_size: usize, world: bool) -> Vec<f64> {
    let mut rng = Lcg(seed);
    let mut p = vec![0.0; PARAMS_FLOATS];
    p[PARAMS_WIND_X] = rng.range(-3.0, 3.0);
    p[PARAMS_WIND_Y] = rng.range(-3.0, 3.0);
    p[PARAMS_WIND_Z] = rng.range(-3.0, 3.0);
    p[PARAMS_DRAG] = rng.range(0.0, 2.0);
    p[PARAMS_N_GRAD] = n_grad as f64;
    for i in 0..n_grad.max(1) {
        let t = i as f64 / (n_grad.max(1).max(2) - 1) as f64;
        p[PARAMS_GRAD + i * 5] = t;
        p[PARAMS_GRAD + i * 5 + 1] = rng.range(0.0, 1.0);
        p[PARAMS_GRAD + i * 5 + 2] = rng.range(0.0, 1.0);
        p[PARAMS_GRAD + i * 5 + 3] = rng.range(0.0, 1.0);
        p[PARAMS_GRAD + i * 5 + 4] = rng.range(0.0, 1.0);
    }
    p[PARAMS_N_SIZE] = n_size as f64;
    for i in 0..n_size.max(1) {
        let t = i as f64 / (n_size.max(1).max(2) - 1) as f64;
        p[PARAMS_SIZE_CURVE + i * 2] = t;
        p[PARAMS_SIZE_CURVE + i * 2 + 1] = rng.range(0.0, 3.0);
    }
    p[PARAMS_SIM_WORLD] = if world { 1.0 } else { 0.0 };
    p[PARAMS_POS_X] = rng.range(-20.0, 20.0);
    p[PARAMS_POS_Y] = rng.range(-20.0, 20.0);
    p[PARAMS_POS_Z] = rng.range(-20.0, 20.0);
    p
}

const TOL: f64 = 1e-5;

fn assert_perto(a: f32, b: f32, ctx: &str) {
    assert!((a as f64 - b as f64).abs() < TOL, "{ctx}: kernel={a} referencia={b}");
}

#[test]
fn paridade_pools_aleatorios_varias_configuracoes() {
    for &n in &[1usize, 2, 7, 50, 400] {
        for &(n_grad, n_size, world) in &[(1usize, 1usize, true), (2, 2, false), (3, 4, true), (4, 3, false)] {
            for seed in [1u64, 42, 999, 123456] {
                let mut pool_k = pool_aleatorio(n, seed);
                let mut pool_r = pool_k.clone();
                let params = params_aleatorios(seed ^ 0xABCD, n_grad, n_size, world);
                let dt = 0.016 + (seed % 7) as f64 * 0.01;

                let mut out_k = vec![0f32; n * PART_INSTANCIA_FLOATS];
                let mut out_r = vec![0f32; n * PART_INSTANCIA_FLOATS];
                let vivas_k = kernel::particles_step(&mut pool_k, &params, dt, &mut out_k);
                let vivas_r = referencia(&mut pool_r, &params, dt, &mut out_r);

                assert_eq!(vivas_k as usize, vivas_r, "n={n} grad={n_grad} size={n_size} world={world} seed={seed}: contagem de vivas diverge");
                for i in 0..vivas_r * PART_INSTANCIA_FLOATS {
                    assert_perto(out_k[i], out_r[i], &format!("n={n} grad={n_grad} size={n_size} world={world} seed={seed} idx={i}"));
                }
                // Estado do pool (posição/idade/vida) também deve bater —
                // não só o buffer de desenho.
                for slot in 0..n {
                    let k = slot * P_FLOATS;
                    for campo in [P_X, P_Y, P_Z, P_VX, P_VY, P_VZ, P_IDADE, P_VIDA] {
                        assert!(
                            (pool_k[k + campo] - pool_r[k + campo]).abs() < TOL,
                            "n={n} slot={slot} campo={campo}: kernel={} referencia={}",
                            pool_k[k + campo],
                            pool_r[k + campo]
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn paridade_dt_zero_nao_muda_posicao_nem_idade() {
    let mut pool_k = pool_aleatorio(30, 7);
    let mut pool_r = pool_k.clone();
    let params = params_aleatorios(7, 2, 2, true);
    let mut out_k = vec![0f32; 30 * PART_INSTANCIA_FLOATS];
    let mut out_r = vec![0f32; 30 * PART_INSTANCIA_FLOATS];
    let vk = kernel::particles_step(&mut pool_k, &params, 0.0, &mut out_k);
    let vr = referencia(&mut pool_r, &params, 0.0, &mut out_r);
    assert_eq!(vk as usize, vr);
    for i in 0..vr * PART_INSTANCIA_FLOATS {
        assert_perto(out_k[i], out_r[i], &format!("dt=0 idx={i}"));
    }
}

#[test]
fn paridade_todas_mortas_devolve_zero() {
    let n = 20;
    let mut pool = vec![0.0; n * P_FLOATS];
    for s in 0..n {
        pool[s * P_FLOATS + P_VIDA] = -1.0;
    }
    let params = params_aleatorios(1, 2, 2, true);
    let mut out = vec![0f32; n * PART_INSTANCIA_FLOATS];
    assert_eq!(kernel::particles_step(&mut pool, &params, 0.016, &mut out), 0);
}
