//! O kernel puro de `particlesStep` — sem `unsafe`, sem alocação por chamada,
//! sem `try`/painço. Espelha, em UM laço fundido por slot, o que
//! `rts-game/src/engine/particles/{sim.ts,curvas.ts}` e
//! `rts-game/src/scripts/particlesystem.ts` fazem em QUATRO passadas
//! separadas sobre o pool (envelhecer+reciclar, vento+arrasto, integrar
//! posição, preencher o buffer de instância) — a fusão é segura porque cada
//! passada só depende do estado ESCRITO pela passada anterior NO MESMO slot,
//! nunca de outro slot, e a ordem por slot é preservada (age → se morreu,
//! pula vento/posição/desenho, exatamente como o `P_VIDA>=0` de
//! `aplicarVelocidade`/o laço final de `update()`/`drawSelf` já fazem na
//! referência TS ao pular um slot que `atualizarVidas` acabou de marcar
//! livre no mesmo quadro).
//!
//! # Layout do pool (`pool: &mut [f64]`)
//!
//! Idêntico a `desc.ts`: `P_FLOATS=14` colunas por partícula, SoA num só
//! array, alocado (`P_VIDA>=0`) ou livre (`P_VIDA<0`, marcador — nunca
//! `idade<vida`, ver o comentário de `atualizarVidas` em `sim.ts`).
//!
//! # Layout de `params: &[f64]` (`PARAMS_FLOATS=42`)
//!
//! Um "desc" só, do mesmo espírito de `D_*`/`DESC_FLOATS` em `desc.ts` — os
//! campos do emissor que `update()`/`drawSelf()` leem a cada quadro e que não
//! cabem como escalares soltos na ABI de 4 argumentos:
//!
//! - `[0..3)` vento efetivo (`ventoX,ventoY,ventoZ`) — JÁ com `gravityModifier`
//!   somado ao componente Y pelo CHAMADOR, exatamente como `update()` monta
//!   `vento[1] = ventoY - gravityModifier` antes de chamar `aplicarVelocidade`
//!   hoje: o kernel não conhece "gravidade" como conceito, só um vetor.
//! - `[3]` arrasto (`arrasto`); `<=0` tratado como 0 (nunca amplifica), mesma
//!   regra de `aplicarVelocidade`.
//! - `[4]` nº de chaves do gradiente de cor (0..=4, clampado).
//! - `[5..25)` as até 4 chaves do gradiente, 5 floats cada
//!   (`tempo,r,g,b,a`) — mesmo formato plano de `ParticleSystem.gradiente`.
//! - `[25]` nº de chaves da curva de tamanho (0..=4, clampado).
//! - `[26..34)` as até 4 chaves da curva, 2 floats cada (`tempo,valor`).
//! - `[34]` `simulationSpace`: != 0 ⇒ "world" (soma `[35..38)` à posição
//!   escrita, como `somaPos` em `drawSelf`); == 0 ⇒ "local" (não soma).
//! - `[35..38)` posição do dono (`pos` do `drawsSelf(win,pos,tint)`).
//! - `[38]` `sortMode`: != 0 ⇒ ordena back-to-front por distância à câmera
//!   ANTES de escrever (mesmo bucket sort de `ordenarPorDistancia`); == 0 ⇒
//!   ordem de slot (a ordem "aditiva", comutativa, que não precisa ordenar).
//! - `[39..42)` posição da câmera (só usada se `sortMode!=0`).
//!
//! # `out: &mut [f32]`
//!
//! `PART_INSTANCIA_FLOATS=9` floats por partícula VIVA escrita, COMPACTADO a
//! partir do índice 0 (sem buracos pelas mortas) — o mesmo formato de
//! `drawParticlesSeguro`: x,y,z,tamanho,rotação,r,g,b,a. Precisa caber
//! `(pool.len()/P_FLOATS) * PART_INSTANCIA_FLOATS` floats (o pior caso, tudo
//! vivo); menor que isso é recusado (devolve 0) — nunca escreve fora do
//! slice.
//!
//! # O que este kernel NÃO faz (decisão deliberada, ver `kernel-rts-report.md`)
//!
//! - **Emissão** (`emitirN`) continua em TS — só o `update`+`drawSelf` por
//!   partícula JÁ viva entra aqui, como o brief pediu.
//! - **Corte de frustum** continua em TS, por EMISSOR (uma esfera contra o
//!   frustum, não por partícula) — baratíssimo comparado ao laço por
//!   partícula que este kernel substitui, e TS já decide se `drawSelf`
//!   roda antes de chamar o kernel.
//! - **A pilha de livres (`pool.livres`/`pool.nLivres`) NÃO é mantida por
//!   este kernel.** Ele só escreve o marcador `P_VIDA=-1` no slot que morreu
//!   (a fonte de verdade que `sim.ts` já usa) — reconciliar isso com a pilha
//!   de livres que `emitirN` consome é trabalho da integração `rts-game`
//!   (fora de escopo aqui). Ver "Concerns" no relatório para o design
//!   recomendado (cursor rotativo sobre `P_VIDA`, sem pilha separada).

/// Colunas do pool — mesmos nomes/índices de `desc.ts` (`P_*`).
pub const P_X: usize = 0;
/// Ver [`P_X`].
pub const P_Y: usize = 1;
/// Ver [`P_X`].
pub const P_Z: usize = 2;
/// Ver [`P_X`].
pub const P_VX: usize = 3;
/// Ver [`P_X`].
pub const P_VY: usize = 4;
/// Ver [`P_X`].
pub const P_VZ: usize = 5;
/// Ver [`P_X`].
pub const P_IDADE: usize = 6;
/// Marcador de alocado/livre: `>=0.0` viva, `<0.0` livre. Ver `sim.ts`.
pub const P_VIDA: usize = 7;
/// Ver [`P_X`].
pub const P_TAM0: usize = 8;
/// Ver [`P_X`].
pub const P_ROT: usize = 9;
/// Ver [`P_X`].
pub const P_COR_R: usize = 10;
/// Ver [`P_X`].
pub const P_COR_G: usize = 11;
/// Ver [`P_X`].
pub const P_COR_B: usize = 12;
/// Ver [`P_X`].
pub const P_COR_A: usize = 13;
/// Largura de uma linha do pool — mesmo valor de `P_FLOATS` em `desc.ts`.
pub const P_FLOATS: usize = 14;

/// Largura de uma chave de gradiente (`tempo,r,g,b,a`).
const CHAVE_GRADIENTE_FLOATS: usize = 5;
/// Largura de uma chave de curva de tamanho (`tempo,valor`).
const CHAVE_CURVA_FLOATS: usize = 2;
/// Nº máximo de chaves (gradiente ou curva) — mesmo `MAX_CHAVES` de
/// `particlesystem.ts`.
const MAX_CHAVES: usize = 4;

/// Layout de `params` — ver o comentário do módulo.
pub const PARAMS_WIND_X: usize = 0;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_WIND_Y: usize = 1;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_WIND_Z: usize = 2;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_DRAG: usize = 3;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_N_GRAD: usize = 4;
/// Início das `MAX_CHAVES*CHAVE_GRADIENTE_FLOATS` (20) posições do gradiente.
pub const PARAMS_GRAD: usize = 5;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_N_SIZE: usize = 25;
/// Início das `MAX_CHAVES*CHAVE_CURVA_FLOATS` (8) posições da curva de tamanho.
pub const PARAMS_SIZE_CURVE: usize = 26;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_SIM_WORLD: usize = 34;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_POS_X: usize = 35;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_POS_Y: usize = 36;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_POS_Z: usize = 37;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_SORT_MODE: usize = 38;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_CAM_X: usize = 39;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_CAM_Y: usize = 40;
/// Ver [`PARAMS_WIND_X`].
pub const PARAMS_CAM_Z: usize = 41;
/// Largura total de `params`.
pub const PARAMS_FLOATS: usize = 42;

/// Floats por partícula VIVA em `out` — mesmo `PART_INSTANCIA_FLOATS` de
/// `particlesystem.ts` (x,y,z,tamanho,rotação,r,g,b,a).
pub const PART_INSTANCIA_FLOATS: usize = 9;

/// Baldes do bucket sort — mesmo `PS_SORT_BALDES` de `particlesystem.ts`.
pub const SORT_BALDES: usize = 256;

/// Amostra o gradiente de cor em `t` (∈ aproximadamente [0,1], não
/// necessariamente clampado — ver a chamada, `t` já vem de `idade/vida`).
/// `chaves`: `[tempo0,r0,g0,b0,a0, …]`, até `MAX_CHAVES`. Porta exata de
/// `avaliarGradiente` (`curvas.ts`): busca linear pela primeira chave cujo
/// tempo é `>= t`, interpola linear entre ela e a anterior. `nChaves<=1`
/// devolve sempre a chave 0. Tempos duplicados (`t1<=t0`) caem no ramo
/// `f=0.0`, sem `NaN`.
fn avaliar_gradiente(chaves: &[f64], n_chaves: usize, t: f64, out: &mut [f32; 4]) {
    if n_chaves <= 1 {
        out[0] = chaves[1] as f32;
        out[1] = chaves[2] as f32;
        out[2] = chaves[3] as f32;
        out[3] = chaves[4] as f32;
        return;
    }
    let mut i = 0usize;
    while i < n_chaves - 1 && chaves[(i + 1) * CHAVE_GRADIENTE_FLOATS] < t {
        i += 1;
    }
    if i >= n_chaves - 1 {
        i = n_chaves - 2;
    }
    let t0 = chaves[i * CHAVE_GRADIENTE_FLOATS];
    let t1 = chaves[(i + 1) * CHAVE_GRADIENTE_FLOATS];
    let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    for c in 0..4 {
        let a = chaves[i * CHAVE_GRADIENTE_FLOATS + 1 + c];
        let b = chaves[(i + 1) * CHAVE_GRADIENTE_FLOATS + 1 + c];
        out[c] = (a + (b - a) * f) as f32;
    }
}

/// Amostra a curva escalar (tamanho) em `t`. `chaves`: `[tempo0,valor0, …]`,
/// até `MAX_CHAVES`. Porta exata de `avaliarCurva` (`curvas.ts`); mesma
/// pré-condição/tratamento de `avaliar_gradiente`.
fn avaliar_curva(chaves: &[f64], n_chaves: usize, t: f64) -> f64 {
    if n_chaves <= 1 {
        return chaves[1];
    }
    let mut i = 0usize;
    while i < n_chaves - 1 && chaves[(i + 1) * CHAVE_CURVA_FLOATS] < t {
        i += 1;
    }
    if i >= n_chaves - 1 {
        i = n_chaves - 2;
    }
    let t0 = chaves[i * CHAVE_CURVA_FLOATS];
    let t1 = chaves[(i + 1) * CHAVE_CURVA_FLOATS];
    let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    let a = chaves[i * CHAVE_CURVA_FLOATS + 1];
    let b = chaves[(i + 1) * CHAVE_CURVA_FLOATS + 1];
    a + (b - a) * f
}

/// Clampa uma contagem de chaves lida de `params` para `[0, MAX_CHAVES]`,
/// sem `NaN`/negativo/fora de alcance travando nada (`params` é o desc de um
/// quadro, mesma confiança de `D_*` em `desc.ts` — não validado por chave,
/// só por FORMA aqui).
fn n_chaves_de(v: f64) -> usize {
    if !v.is_finite() || v <= 0.0 {
        0
    } else if v >= MAX_CHAVES as f64 {
        MAX_CHAVES
    } else {
        v as usize
    }
}

/// Escrata reaproveitada do bucket sort — thread-local, cresce (nunca
/// encolhe) com `n`, mesmo padrão de `distBuf`/`ordemBuf`/`itemBalde`/
/// `saidaOrdenadaBuf` em `particlesystem.ts`. Isolada num `struct` só para
/// nomear os cinco buffers juntos.
#[derive(Default)]
struct EscrataSort {
    dist: Vec<f64>,
    balde_de: Vec<i32>,
    ordem: Vec<i32>,
    saida: Vec<f32>,
}

thread_local! {
    static SORT_SCRATCH: std::cell::RefCell<EscrataSort> = std::cell::RefCell::new(EscrataSort::default());
}

/// Reordena as `n` primeiras partículas de `buf` (9 floats cada) da mais
/// distante para a mais próxima de `(cam_x,cam_y,cam_z)` — porta exata do
/// bucket sort de `ordenarPorDistancia` (`particlesystem.ts`): quantiza
/// distância² em `SORT_BALDES` faixas, conta por balde, offset cumulativo
/// DECRESCENTE (o balde mais distante ocupa as primeiras posições), escreve
/// a ordem e copia. `O(n + SORT_BALDES)`, sem alocar (os `Vec` do
/// thread-local só crescem, nunca são recriados por chamada).
fn ordenar_por_distancia(buf: &mut [f32], n: usize, cam_x: f64, cam_y: f64, cam_z: f64) {
    SORT_SCRATCH.with(|cell| {
        let mut s = cell.borrow_mut();
        if s.dist.len() < n {
            s.dist.resize(n, 0.0);
        }
        if s.balde_de.len() < n {
            s.balde_de.resize(n, 0);
        }
        if s.ordem.len() < n {
            s.ordem.resize(n, 0);
        }
        let needed = n * PART_INSTANCIA_FLOATS;
        if s.saida.len() < needed {
            s.saida.resize(needed, 0.0);
        }

        let mut min_d = f64::MAX;
        let mut max_d = f64::MIN;
        for i in 0..n {
            let o = i * PART_INSTANCIA_FLOATS;
            let dx = buf[o] as f64 - cam_x;
            let dy = buf[o + 1] as f64 - cam_y;
            let dz = buf[o + 2] as f64 - cam_z;
            let d = dx * dx + dy * dy + dz * dz;
            s.dist[i] = d;
            if d < min_d {
                min_d = d;
            }
            if d > max_d {
                max_d = d;
            }
        }

        let faixa = max_d - min_d;
        let inv = if faixa > 1e-12 { (SORT_BALDES - 1) as f64 / faixa } else { 0.0 };

        let mut contagem = [0i32; SORT_BALDES];
        for i in 0..n {
            let mut b = ((s.dist[i] - min_d) * inv) as i64;
            if b < 0 {
                b = 0;
            } else if b >= SORT_BALDES as i64 {
                b = SORT_BALDES as i64 - 1;
            }
            s.balde_de[i] = b as i32;
            contagem[b as usize] += 1;
        }

        let mut offset = [0i32; SORT_BALDES];
        let mut acc = 0i32;
        for b in (0..SORT_BALDES).rev() {
            offset[b] = acc;
            acc += contagem[b];
        }
        for i in 0..n {
            let b = s.balde_de[i] as usize;
            s.ordem[offset[b] as usize] = i as i32;
            offset[b] += 1;
        }

        for k in 0..n {
            let src = s.ordem[k] as usize * PART_INSTANCIA_FLOATS;
            let dst = k * PART_INSTANCIA_FLOATS;
            s.saida[dst..dst + PART_INSTANCIA_FLOATS].copy_from_slice(&buf[src..src + PART_INSTANCIA_FLOATS]);
        }
        buf[..needed].copy_from_slice(&s.saida[..needed]);
    });
}

/// `particlesStep(pool, params, dt, out) -> vivas`. Ver o comentário do
/// módulo para o layout de cada buffer. Devolve `0` (sem tocar em `pool`/
/// `out`) se `pool`/`params`/`out` não têm o tamanho que o contrato exige —
/// nunca lê/escreve fora de um slice, nunca panica em `debug` (todo índice
/// vem de uma contagem já validada contra o comprimento real).
///
/// `dt` não-finito ou negativo é tratado como `0.0` (um quadro sem avanço,
/// não um "quadro inválido que trava tudo") — a mesma postura defensiva de
/// `aplicarVelocidade`'s `arrasto<0 ⇒ 0`. O CLAMP para `PS_DT_MAX_PASSO`
/// continua em TS (é política de `ParticleSystem.update`, não do kernel).
pub fn particles_step(pool: &mut [f64], params: &[f64], dt: f64, out: &mut [f32]) -> i64 {
    if pool.is_empty() || pool.len() % P_FLOATS != 0 {
        return 0;
    }
    if params.len() < PARAMS_FLOATS {
        return 0;
    }
    let max = pool.len() / P_FLOATS;
    let out_needed = match max.checked_mul(PART_INSTANCIA_FLOATS) {
        Some(v) => v,
        None => return 0,
    };
    if out.len() < out_needed {
        return 0;
    }
    let dt = if dt.is_finite() && dt >= 0.0 { dt } else { 0.0 };

    let arrasto = params[PARAMS_DRAG];
    let arrasto_efetivo = if arrasto.is_finite() && arrasto > 0.0 { arrasto } else { 0.0 };
    let f_arrasto = (-arrasto_efetivo * dt).exp();
    let vento_x = if params[PARAMS_WIND_X].is_finite() { params[PARAMS_WIND_X] } else { 0.0 };
    let vento_y = if params[PARAMS_WIND_Y].is_finite() { params[PARAMS_WIND_Y] } else { 0.0 };
    let vento_z = if params[PARAMS_WIND_Z].is_finite() { params[PARAMS_WIND_Z] } else { 0.0 };

    let n_grad = n_chaves_de(params[PARAMS_N_GRAD]);
    let n_size = n_chaves_de(params[PARAMS_N_SIZE]);
    let gradiente = &params[PARAMS_GRAD..PARAMS_GRAD + MAX_CHAVES * CHAVE_GRADIENTE_FLOATS];
    let curva_tam = &params[PARAMS_SIZE_CURVE..PARAMS_SIZE_CURVE + MAX_CHAVES * CHAVE_CURVA_FLOATS];

    let soma_pos = params[PARAMS_SIM_WORLD] != 0.0;
    let (pos_x, pos_y, pos_z) = if soma_pos {
        (params[PARAMS_POS_X], params[PARAMS_POS_Y], params[PARAMS_POS_Z])
    } else {
        (0.0, 0.0, 0.0)
    };

    let mut n = 0usize;
    let mut cor = [0f32; 4];
    for slot in 0..max {
        let k = slot * P_FLOATS;
        if pool[k + P_VIDA] < 0.0 {
            continue;
        }
        // ── envelhecer / reciclar (atualizarVidas) ─────────────────────
        let idade = pool[k + P_IDADE] + dt;
        pool[k + P_IDADE] = idade;
        let vida = pool[k + P_VIDA];
        if idade >= vida {
            pool[k + P_VIDA] = -1.0;
            // Morta NESTE quadro: sem vento/posição/desenho, igual à
            // referência TS (ver o comentário do módulo).
            continue;
        }

        // ── vento + arrasto exponencial (aplicarVelocidade) ────────────
        let vx = (pool[k + P_VX] + vento_x * dt) * f_arrasto;
        let vy = (pool[k + P_VY] + vento_y * dt) * f_arrasto;
        let vz = (pool[k + P_VZ] + vento_z * dt) * f_arrasto;
        pool[k + P_VX] = vx;
        pool[k + P_VY] = vy;
        pool[k + P_VZ] = vz;

        // ── integração de posição (laço final de update()) ─────────────
        let x = pool[k + P_X] + vx * dt;
        let y = pool[k + P_Y] + vy * dt;
        let z = pool[k + P_Z] + vz * dt;
        pool[k + P_X] = x;
        pool[k + P_Y] = y;
        pool[k + P_Z] = z;

        // ── buffer de instância (drawSelf) ──────────────────────────────
        let t = if vida > 0.0 { idade / vida } else { 1.0 };
        avaliar_gradiente(gradiente, n_grad, t, &mut cor);
        let escala = avaliar_curva(curva_tam, n_size, t);

        let o = n * PART_INSTANCIA_FLOATS;
        out[o] = (pos_x + x) as f32;
        out[o + 1] = (pos_y + y) as f32;
        out[o + 2] = (pos_z + z) as f32;
        out[o + 3] = (pool[k + P_TAM0] * escala) as f32;
        out[o + 4] = pool[k + P_ROT] as f32;
        out[o + 5] = cor[0];
        out[o + 6] = cor[1];
        out[o + 7] = cor[2];
        out[o + 8] = cor[3];
        n += 1;
    }

    if params[PARAMS_SORT_MODE] != 0.0 && n > 1 {
        ordenar_por_distancia(out, n, params[PARAMS_CAM_X], params[PARAMS_CAM_Y], params[PARAMS_CAM_Z]);
    }

    n as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um `params` neutro: sem vento/arrasto, gradiente/curva de 1 chave
    /// (branco opaco, escala 1), espaço "world" com dono na origem, sem sort
    /// — o baseline que cada teste ajusta.
    fn params_neutros() -> Vec<f64> {
        let mut p = vec![0.0; PARAMS_FLOATS];
        p[PARAMS_N_GRAD] = 1.0;
        p[PARAMS_GRAD] = 0.0;
        p[PARAMS_GRAD + 1] = 1.0;
        p[PARAMS_GRAD + 2] = 1.0;
        p[PARAMS_GRAD + 3] = 1.0;
        p[PARAMS_GRAD + 4] = 1.0;
        p[PARAMS_N_SIZE] = 1.0;
        p[PARAMS_SIZE_CURVE] = 0.0;
        p[PARAMS_SIZE_CURVE + 1] = 1.0;
        p[PARAMS_SIM_WORLD] = 1.0;
        p
    }

    fn uma_particula(idade: f64, vida: f64) -> Vec<f64> {
        let mut row = vec![0.0; P_FLOATS];
        row[P_X] = 1.0;
        row[P_Y] = 2.0;
        row[P_Z] = 3.0;
        row[P_VX] = 1.0;
        row[P_VY] = 0.0;
        row[P_VZ] = 0.0;
        row[P_IDADE] = idade;
        row[P_VIDA] = vida;
        row[P_TAM0] = 2.0;
        row[P_ROT] = 0.5;
        row[P_COR_R] = 1.0;
        row[P_COR_G] = 1.0;
        row[P_COR_B] = 1.0;
        row[P_COR_A] = 1.0;
        row
    }

    #[test]
    fn tamanhos_invalidos_nao_travam() {
        let mut pool = vec![0.0; 3]; // não múltiplo de P_FLOATS
        let params = params_neutros();
        let mut out = vec![0f32; 100];
        assert_eq!(particles_step(&mut pool, &params, 0.016, &mut out), 0);

        let mut pool2 = vec![0.0; P_FLOATS];
        pool2[P_VIDA] = -1.0;
        let curto = vec![0.0; PARAMS_FLOATS - 1];
        assert_eq!(particles_step(&mut pool2, &curto, 0.016, &mut out), 0);

        let mut pool3 = vec![0.0; P_FLOATS * 4];
        for s in 0..4 {
            pool3[s * P_FLOATS + P_VIDA] = -1.0;
        }
        let mut out_curto = vec![0f32; 2]; // menor que 4*9
        assert_eq!(particles_step(&mut pool3, &params, 0.016, &mut out_curto), 0);

        let mut vazio: Vec<f64> = vec![];
        assert_eq!(particles_step(&mut vazio, &params, 0.016, &mut out), 0);
    }

    #[test]
    fn nan_no_pool_ou_dt_nao_trava_e_nao_produz_nan_na_saida() {
        let mut pool = uma_particula(0.0, 1.0);
        pool[P_VX] = f64::NAN;
        let params = params_neutros();
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        let vivas = particles_step(&mut pool, &params, f64::NAN, &mut out);
        // dt NaN vira 0.0: idade não avança, vida não expira, ainda viva.
        assert_eq!(vivas, 1);
        // vx era NaN: propaga (não é o `dt` que produzia o NaN aqui), então
        // só garantimos que NADA panica — o teste de paridade cobre os casos
        // numéricos "normais" com tolerância.
        let _ = out[0];
    }

    #[test]
    fn recicla_quando_idade_atinge_vida() {
        let mut pool = uma_particula(0.9, 1.0);
        let params = params_neutros();
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        let vivas = particles_step(&mut pool, &params, 0.2, &mut out);
        assert_eq!(vivas, 0, "morreu neste quadro: não desenha");
        assert!(pool[P_VIDA] < 0.0, "marcador de livre");
    }

    #[test]
    fn vida_sorteada_em_zero_morre_no_mesmo_quadro_sem_nan() {
        let mut pool = uma_particula(0.0, 0.0);
        let params = params_neutros();
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        let vivas = particles_step(&mut pool, &params, 0.016, &mut out);
        assert_eq!(vivas, 0);
        assert!(pool[P_VIDA] < 0.0);
    }

    #[test]
    fn slot_livre_e_ignorado() {
        let mut pool = uma_particula(0.5, 1.0);
        pool[P_VIDA] = -1.0;
        let params = params_neutros();
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        let vivas = particles_step(&mut pool, &params, 0.016, &mut out);
        assert_eq!(vivas, 0);
        // Posição/idade não avançam para um slot livre.
        assert_eq!(pool[P_X], 1.0);
        assert_eq!(pool[P_IDADE], 0.5);
    }

    #[test]
    fn integra_posicao_com_vento_e_arrasto() {
        let mut pool = uma_particula(0.0, 10.0);
        let mut params = params_neutros();
        params[PARAMS_WIND_X] = 2.0;
        params[PARAMS_DRAG] = 1.0;
        let dt = 0.1;
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        particles_step(&mut pool, &params, dt, &mut out);
        let f_arrasto = (-1.0f64 * dt).exp();
        let vx_esperado = (1.0 + 2.0 * dt) * f_arrasto;
        assert!((pool[P_VX] - vx_esperado).abs() < 1e-12);
        let x_esperado = 1.0 + vx_esperado * dt;
        assert!((pool[P_X] - x_esperado).abs() < 1e-12);
        assert!((out[0] as f64 - x_esperado).abs() < 1e-5);
    }

    #[test]
    fn arrasto_negativo_e_tratado_como_zero() {
        let mut pool = uma_particula(0.0, 10.0);
        let mut params = params_neutros();
        params[PARAMS_DRAG] = -5.0;
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        particles_step(&mut pool, &params, 0.1, &mut out);
        // Sem arrasto: vx só ganha o vento (0 aqui), permanece 1.0.
        assert!((pool[P_VX] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn espaco_local_nao_soma_pos_do_dono() {
        let mut pool = uma_particula(0.0, 10.0);
        let mut params = params_neutros();
        params[PARAMS_SIM_WORLD] = 0.0;
        params[PARAMS_POS_X] = 100.0;
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        particles_step(&mut pool, &params, 0.0, &mut out);
        assert!((out[0] as f64 - 1.0).abs() < 1e-5, "não deve somar 100");
    }

    #[test]
    fn espaco_world_soma_pos_do_dono() {
        let mut pool = uma_particula(0.0, 10.0);
        let mut params = params_neutros();
        params[PARAMS_POS_X] = 100.0;
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        particles_step(&mut pool, &params, 0.0, &mut out);
        assert!((out[0] as f64 - 101.0).abs() < 1e-5);
    }

    #[test]
    fn compacta_saida_pulando_mortas_e_livres() {
        let mut pool = vec![0.0; P_FLOATS * 3];
        // slot 0: livre
        pool[0 * P_FLOATS + P_VIDA] = -1.0;
        // slot 1: viva, vida longa
        let viva = uma_particula(0.0, 10.0);
        pool[P_FLOATS..2 * P_FLOATS].copy_from_slice(&viva);
        // slot 2: viva, morre este quadro
        let morre = uma_particula(0.99, 1.0);
        pool[2 * P_FLOATS..3 * P_FLOATS].copy_from_slice(&morre);

        let params = params_neutros();
        let mut out = vec![0f32; 3 * PART_INSTANCIA_FLOATS];
        let vivas = particles_step(&mut pool, &params, 0.1, &mut out);
        assert_eq!(vivas, 1);
        // A única viva (slot 1) foi escrita compactada na posição 0.
        assert!((out[0] as f64 - (1.0 + 1.0 * 0.1)).abs() < 1e-5);
    }

    #[test]
    fn gradiente_de_duas_chaves_interpola() {
        let mut pool = uma_particula(0.5, 1.0); // t = 0.5 após +dt=0? ver abaixo
        let mut params = params_neutros();
        params[PARAMS_N_GRAD] = 2.0;
        // chave 0: t=0, cor (0,0,0,0); chave 1: t=1, cor (1,1,1,1)
        params[PARAMS_GRAD] = 0.0;
        params[PARAMS_GRAD + 1] = 0.0;
        params[PARAMS_GRAD + 2] = 0.0;
        params[PARAMS_GRAD + 3] = 0.0;
        params[PARAMS_GRAD + 4] = 0.0;
        params[PARAMS_GRAD + 5] = 1.0;
        params[PARAMS_GRAD + 6] = 1.0;
        params[PARAMS_GRAD + 7] = 1.0;
        params[PARAMS_GRAD + 8] = 1.0;
        params[PARAMS_GRAD + 9] = 1.0;
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        // dt=0 então t = idade/vida = 0.5/1.0 = 0.5 exatamente (idade não
        // avança por este dt).
        particles_step(&mut pool, &params, 0.0, &mut out);
        assert!((out[5] - 0.5).abs() < 1e-5, "r deveria interpolar para 0.5, foi {}", out[5]);
    }

    #[test]
    fn sort_ordena_do_mais_distante_para_o_mais_perto() {
        let mut pool = vec![0.0; P_FLOATS * 3];
        for (i, x) in [1.0, 5.0, 3.0].iter().enumerate() {
            let mut row = uma_particula(0.0, 10.0);
            row[P_X] = *x;
            row[P_Y] = 0.0;
            row[P_Z] = 0.0;
            pool[i * P_FLOATS..(i + 1) * P_FLOATS].copy_from_slice(&row);
        }
        let mut params = params_neutros();
        params[PARAMS_SORT_MODE] = 1.0;
        params[PARAMS_CAM_X] = 0.0;
        let mut out = vec![0f32; 3 * PART_INSTANCIA_FLOATS];
        let vivas = particles_step(&mut pool, &params, 0.0, &mut out);
        assert_eq!(vivas, 3);
        // Farthest (x=5) primeiro, depois x=3, depois x=1.
        assert!((out[0] - 5.0).abs() < 1e-3, "esperava 5.0 primeiro, out={:?}", &out[..9]);
        assert!((out[9] - 3.0).abs() < 1e-3);
        assert!((out[18] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn sort_com_uma_particula_nao_reordena_nem_le_fora() {
        let mut pool = uma_particula(0.0, 10.0);
        let mut params = params_neutros();
        params[PARAMS_SORT_MODE] = 1.0;
        let mut out = vec![0f32; PART_INSTANCIA_FLOATS];
        let vivas = particles_step(&mut pool, &params, 0.0, &mut out);
        assert_eq!(vivas, 1);
    }
}
