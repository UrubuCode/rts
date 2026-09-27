//! Os dois kernels de DSP de `rts:audio`: somar um clipe num bloco e medir o
//! bloco. Genéricos: não sabem o que é voz, grupo ou ouvinte (spec §3.1).
//!
//! # O descritor
//!
//! `mix_add` recebe TRÊS argumentos — `dst`, `src` e um `desc: Float64Array` de
//! [`DESC_FLOATS`] — porque um nativo deste motor vê quatro, e porque no RTS uma
//! função de 5+ parâmetros aloca por chamada. O `desc` é de entrada E de saída:
//! a posição, o estado do passa-baixa, quantos quadros saíram e se o clipe
//! acabou voltam nele, sem alocar nada.

/// Posição fracionária no clipe, em quadros (entrada e saída).
pub const D_POS: usize = 0;
/// Quadros do clipe por quadro de saída (pitch × razão de taxas).
pub const D_PASSO: usize = 1;
/// Canais do clipe: 1 ou 2.
pub const D_CANAIS_SRC: usize = 2;
/// Canais do bloco de saída: 1 a 8.
pub const D_CANAIS_DST: usize = 3;
/// Quadros a mixar neste bloco.
pub const D_QUADROS: usize = 4;
/// Ganho esquerdo no início do bloco.
pub const D_GL0: usize = 5;
/// Ganho direito no início do bloco.
pub const D_GR0: usize = 6;
/// Ganho esquerdo alcançado na última amostra do bloco.
pub const D_GL1: usize = 7;
/// Ganho direito alcançado na última amostra do bloco.
pub const D_GR1: usize = 8;
/// Coeficiente do passa-baixa de um polo, `a` em `y += a(x − y)`; ≥ 1 desliga.
pub const D_LP_COEF: usize = 9;
/// Estado do passa-baixa, canal esquerdo (entrada e saída).
pub const D_LP_L: usize = 10;
/// Estado do passa-baixa, canal direito (entrada e saída).
pub const D_LP_R: usize = 11;
/// Início do laço, em quadros do clipe.
pub const D_LACO_INI: usize = 12;
/// Fim do laço, em quadros do clipe; `≤ início` = sem laço.
pub const D_LACO_FIM: usize = 13;
/// SAÍDA: quadros efetivamente mixados.
pub const D_MIXADOS: usize = 14;
/// SAÍDA: 1 quando o clipe (sem laço) chegou ao fim.
pub const D_FIM: usize = 15;
/// Tamanho do descritor.
pub const DESC_FLOATS: usize = 16;

/// Maior número de canais de saída aceito (7.1).
const CANAIS_DST_MAX: usize = 8;

fn finito_ou(v: f64, padrao: f64) -> f64 { if v.is_finite() { v } else { padrao } }

/// Soma um bloco de `src` (clipe intercalado, mono ou estéreo) em `dst` (bloco
/// de saída intercalado), com interpolação linear, rampa de ganho por amostra,
/// passa-baixa de um polo e laço. Devolve os quadros mixados (0 = recusa).
pub fn mix_add(dst: &mut [f32], src: &[f32], desc: &mut [f64]) -> usize {
    if desc.len() < DESC_FLOATS { return 0; }
    desc[D_MIXADOS] = 0.0;
    desc[D_FIM] = 0.0;
    let cs = finito_ou(desc[D_CANAIS_SRC], 0.0) as usize;
    let cd = finito_ou(desc[D_CANAIS_DST], 0.0) as usize;
    if !(cs == 1 || cs == 2) || cd == 0 || cd > CANAIS_DST_MAX { return 0; }
    let total = src.len() / cs;
    if total == 0 { desc[D_FIM] = 1.0; return 0; }
    let pedidos = finito_ou(desc[D_QUADROS], 0.0).max(0.0) as usize;
    let n = pedidos.min(dst.len() / cd);
    if n == 0 { return 0; }
    let passo = { let p = finito_ou(desc[D_PASSO], 1.0); if p > 0.0 { p } else { 1.0 } };
    let li = finito_ou(desc[D_LACO_INI], -1.0);
    let lf = finito_ou(desc[D_LACO_FIM], -1.0);
    let laco = li >= 0.0 && lf > li && lf <= total as f64;
    let limite = if laco { lf } else { total as f64 };
    let mut pos = finito_ou(desc[D_POS], 0.0).max(0.0);
    let a = { let c = finito_ou(desc[D_LP_COEF], 1.0); if c > 0.0 && c < 1.0 { c as f32 } else { 1.0 } };
    let mut yl = finito_ou(desc[D_LP_L], 0.0) as f32;
    let mut yr = finito_ou(desc[D_LP_R], 0.0) as f32;
    let gl0 = finito_ou(desc[D_GL0], 0.0) as f32;
    let gr0 = finito_ou(desc[D_GR0], 0.0) as f32;
    let dgl = (finito_ou(desc[D_GL1], 0.0) as f32 - gl0) / n as f32;
    let dgr = (finito_ou(desc[D_GR1], 0.0) as f32 - gr0) / n as f32;
    let mut i = 0usize;
    while i < n {
        if pos >= limite {
            if laco { pos = li + (pos - lf) % (lf - li); } else { desc[D_FIM] = 1.0; break; }
        }
        let i0 = pos as usize;
        let frac = (pos - i0 as f64) as f32;
        let mut i1 = i0 + 1;
        if i1 as f64 >= limite { i1 = if laco { li as usize } else { i0 }; }
        let (xl, xr) = if cs == 1 {
            let s0 = src[i0];
            let s = s0 + (src[i1] - s0) * frac;
            (s, s)
        } else {
            let l0 = src[i0 * 2];
            let r0 = src[i0 * 2 + 1];
            (l0 + (src[i1 * 2] - l0) * frac, r0 + (src[i1 * 2 + 1] - r0) * frac)
        };
        yl += a * (xl - yl);
        yr += a * (xr - yr);
        let k = (i + 1) as f32;
        let l = yl * (gl0 + dgl * k);
        let r = yr * (gr0 + dgr * k);
        let base = i * cd;
        if cd == 1 {
            dst[base] += (l + r) * 0.5;
        } else {
            dst[base] += l;
            dst[base + 1] += r;
            for c in 2..cd { dst[base + c] += (l + r) * 0.5; }
        }
        pos += passo;
        i += 1;
    }
    if !laco && pos >= limite { desc[D_FIM] = 1.0; }
    desc[D_POS] = pos;
    desc[D_LP_L] = yl as f64;
    desc[D_LP_R] = yr as f64;
    desc[D_MIXADOS] = i as f64;
    i
}

/// Canais do bloco (entrada).
pub const N_CANAIS: usize = 0;
/// Quadros a medir (entrada).
pub const N_QUADROS: usize = 1;
/// SAÍDA: pico do canal esquerdo, depois do corte.
pub const N_PICO_L: usize = 2;
/// SAÍDA: pico do canal direito (= esquerdo em mono).
pub const N_PICO_R: usize = 3;
/// SAÍDA: RMS do canal esquerdo.
pub const N_RMS_L: usize = 4;
/// SAÍDA: RMS do canal direito.
pub const N_RMS_R: usize = 5;
/// SAÍDA: amostras que passaram de ±1 e foram cortadas.
pub const N_CORTADAS: usize = 6;
/// Tamanho do vetor de nível.
pub const NIVEL_FLOATS: usize = 8;

/// Corta o bloco em [−1, 1] e mede pico e RMS por canal numa passada. Devolve os
/// quadros medidos (0 = recusa).
pub fn mix_level(buf: &mut [f32], nivel: &mut [f64]) -> usize {
    if nivel.len() < NIVEL_FLOATS { return 0; }
    let ch = finito_ou(nivel[N_CANAIS], 0.0) as usize;
    if ch == 0 || ch > CANAIS_DST_MAX { return 0; }
    let n = (finito_ou(nivel[N_QUADROS], 0.0).max(0.0) as usize).min(buf.len() / ch);
    let (mut pl, mut pr, mut sl, mut sr, mut cortadas) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0u64);
    for q in 0..n {
        for c in 0..ch {
            let at = q * ch + c;
            let mut v = buf[at];
            if v > 1.0 { v = 1.0; cortadas += 1; } else if v < -1.0 { v = -1.0; cortadas += 1; }
            buf[at] = v;
            let a = v.abs() as f64;
            if c == 0 { pl = pl.max(a); sl += a * a; } else if c == 1 { pr = pr.max(a); sr += a * a; }
        }
    }
    if ch == 1 { pr = pl; sr = sl; }
    let div = if n > 0 { n as f64 } else { 1.0 };
    nivel[N_PICO_L] = pl;
    nivel[N_PICO_R] = pr;
    nivel[N_RMS_L] = (sl / div).sqrt();
    nivel[N_RMS_R] = (sr / div).sqrt();
    nivel[N_CORTADAS] = cortadas as f64;
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(cs: f64, cd: f64, quadros: f64) -> [f64; DESC_FLOATS] {
        let mut d = [0.0f64; DESC_FLOATS];
        d[D_PASSO] = 1.0; d[D_CANAIS_SRC] = cs; d[D_CANAIS_DST] = cd; d[D_QUADROS] = quadros;
        d[D_GL0] = 1.0; d[D_GR0] = 1.0; d[D_GL1] = 1.0; d[D_GR1] = 1.0;
        d[D_LP_COEF] = 1.0; d[D_LACO_FIM] = -1.0;
        d
    }

    #[test]
    fn mix_soma_mono_nos_dois_canais() {
        let src = [0.5f32; 8];
        let mut dst = [0.25f32; 8];
        let mut d = desc(1.0, 2.0, 4.0);
        assert_eq!(mix_add(&mut dst, &src, &mut d), 4);
        assert_eq!(dst, [0.75; 8], "SOMA, não escreve por cima");
        assert_eq!(d[D_POS], 4.0);
        assert_eq!(d[D_MIXADOS], 4.0);
        assert_eq!(d[D_FIM], 0.0);
    }

    #[test]
    fn mix_estereo_mantem_a_ordem_dos_canais() {
        let src = [1.0f32, -1.0, 1.0, -1.0];
        let mut dst = [0.0f32; 4];
        let mut d = desc(2.0, 2.0, 2.0);
        d[D_GL0] = 0.5; d[D_GL1] = 0.5; d[D_GR0] = 0.25; d[D_GR1] = 0.25;
        mix_add(&mut dst, &src, &mut d);
        assert_eq!(dst, [0.5, -0.25, 0.5, -0.25]);
    }

    #[test]
    fn mix_rampa_de_ganho_linear_sem_degrau() {
        let src = [1.0f32; 4];
        let mut dst = [0.0f32; 8];
        let mut d = desc(1.0, 2.0, 4.0);
        d[D_GL0] = 0.0; d[D_GL1] = 1.0; d[D_GR0] = 1.0; d[D_GR1] = 1.0;
        mix_add(&mut dst, &src, &mut d);
        let esq: Vec<f32> = dst.iter().step_by(2).copied().collect();
        assert_eq!(esq, vec![0.25, 0.5, 0.75, 1.0], "chega exatamente ao alvo na última amostra");
        for k in 1..4 { assert!((esq[k] - esq[k - 1] - 0.25).abs() < 1e-6, "passo constante"); }
    }

    #[test]
    fn mix_passo_dois_dobra_a_frequencia_e_termina_na_metade() {
        let src: Vec<f32> = (0..8).map(|k| k as f32).collect();
        let mut dst = [0.0f32; 16];
        let mut d = desc(1.0, 2.0, 8.0);
        d[D_PASSO] = 2.0;
        assert_eq!(mix_add(&mut dst, &src, &mut d), 4, "8 quadros a passo 2 acabam em 4");
        assert_eq!(d[D_FIM], 1.0);
        assert_eq!(dst[0], 0.0); assert_eq!(dst[2], 2.0); assert_eq!(dst[4], 4.0); assert_eq!(dst[6], 6.0);
    }

    #[test]
    fn mix_interpola_linear_no_passo_fracionario() {
        let src = [0.0f32, 1.0, 0.0, 1.0];
        let mut dst = [0.0f32; 2];
        let mut d = desc(1.0, 1.0, 2.0);
        d[D_PASSO] = 0.5;
        mix_add(&mut dst, &src, &mut d);
        assert_eq!(dst, [0.0, 0.5], "posição 0,5 entre 0 e 1");
        assert_eq!(d[D_POS], 1.0);
    }

    #[test]
    fn mix_laco_da_a_volta_sem_descontinuidade() {
        let src: Vec<f32> = (0..4).map(|k| k as f32).collect();
        let mut dst = [0.0f32; 10];
        let mut d = desc(1.0, 1.0, 10.0);
        d[D_LACO_INI] = 0.0; d[D_LACO_FIM] = 4.0;
        assert_eq!(mix_add(&mut dst, &src, &mut d), 10);
        assert_eq!(dst, [0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0, 0.0, 1.0]);
        assert_eq!(d[D_FIM], 0.0);
        assert_eq!(d[D_POS], 2.0, "posição devolvida já dentro do laço");
    }

    #[test]
    fn mix_passa_baixa_de_um_polo_guarda_estado() {
        let src = [1.0f32; 3];
        let mut dst = [0.0f32; 3];
        let mut d = desc(1.0, 1.0, 3.0);
        d[D_LP_COEF] = 0.5;
        mix_add(&mut dst, &src, &mut d);
        assert_eq!(dst, [0.5, 0.75, 0.875], "y += a(x - y)");
        assert_eq!(d[D_LP_L], 0.875);
        assert_eq!(d[D_LP_R], 0.875);
    }

    #[test]
    fn mix_canais_extras_recebem_a_media() {
        let src = [1.0f32];
        let mut dst = [0.0f32; 6];
        let mut d = desc(1.0, 6.0, 1.0);
        d[D_GL0] = 1.0; d[D_GL1] = 1.0; d[D_GR0] = 0.0; d[D_GR1] = 0.0;
        mix_add(&mut dst, &src, &mut d);
        assert_eq!(dst, [1.0, 0.0, 0.5, 0.5, 0.5, 0.5]);
    }

    #[test]
    fn mix_recusa_argumentos_ruins() {
        let src = [1.0f32; 4];
        let mut dst = [0.0f32; 4];
        let mut curto = [0.0f64; 4];
        assert_eq!(mix_add(&mut dst, &src, &mut curto), 0, "desc curto");
        let mut d = desc(3.0, 2.0, 2.0);
        assert_eq!(mix_add(&mut dst, &src, &mut d), 0, "3 canais de origem");
        let mut d = desc(1.0, 0.0, 2.0);
        assert_eq!(mix_add(&mut dst, &src, &mut d), 0, "0 canais de destino");
        let mut d = desc(1.0, 2.0, f64::NAN);
        assert_eq!(mix_add(&mut dst, &src, &mut d), 0, "quadros NaN");
        let mut d = desc(1.0, 2.0, 100.0);
        assert_eq!(mix_add(&mut dst, &src, &mut d), 2, "preso ao tamanho de dst");
        let mut d = desc(1.0, 2.0, 2.0);
        d[D_PASSO] = f64::NAN;
        assert_eq!(mix_add(&mut dst, &src, &mut d), 2);
        assert_eq!(d[D_POS], 2.0, "passo NaN vale 1");
        let mut d = desc(1.0, 2.0, 2.0);
        assert_eq!(mix_add(&mut dst, &[], &mut d), 0);
        assert_eq!(d[D_FIM], 1.0, "clipe vazio termina");
    }

    #[test]
    fn nivel_corta_e_mede() {
        let mut buf = [2.0f32, -0.5, -3.0, 0.5];
        let mut n = [0.0f64; NIVEL_FLOATS];
        n[N_CANAIS] = 2.0; n[N_QUADROS] = 2.0;
        assert_eq!(mix_level(&mut buf, &mut n), 2);
        assert_eq!(buf, [1.0, -0.5, -1.0, 0.5], "cortado em [-1, 1]");
        assert_eq!(n[N_PICO_L], 1.0);
        assert_eq!(n[N_PICO_R], 0.5);
        assert!((n[N_RMS_L] - 1.0).abs() < 1e-9);
        assert!((n[N_RMS_R] - 0.5).abs() < 1e-9);
        assert_eq!(n[N_CORTADAS], 2.0);
        let mut mono = [0.5f32, -0.5];
        n[N_CANAIS] = 1.0; n[N_QUADROS] = 2.0;
        mix_level(&mut mono, &mut n);
        assert_eq!(n[N_PICO_R], n[N_PICO_L], "mono: R = L");
    }
}
