//! Luzes, céu e neblina do scene pass: o formato que o TS envia
//! (`setLights`/`setSky`/`setFog`) e o que o uniform `Env` do shader carrega.
//!
//! Aritmética pura, testável sem GPU. `attenuation`, `spot_factor` e
//! `fog_factor` são o CONTRATO do shader: `shader.rs` repete a mesma conta em
//! WGSL, e os testes fixam os números.

pub const MAX_LIGHTS: usize = 8;
/// Floats por luz no buffer do TS: tipo, pos(3), dir(3), cor(3), intensidade,
/// alcance, cos interno, cos externo, sombra, reserva.
pub const LIGHT_IN: usize = 16;
/// Floats por luz no uniform: a (pos, tipo), b (dir, alcance), c (cor,
/// intensidade), d (cos interno, cos externo, sombra, 0).
pub const LIGHT_GPU: usize = 16;
/// Floats do `setSky`: modo, topo(3), horizonte(3), chão(3), sol(3), tamanho
/// do disco, estrelas, exposição, id da textura, modo do ambiente, cor do
/// ambiente(3), intensidade do ambiente.
pub const SKY_IN: usize = 22;
/// Floats do uniform `Env`: 8 vec4 de cabeçalho + 8 luzes de 16.
pub const ENV_FLOATS: usize = 160;
pub const ENV_BYTES: u64 = (ENV_FLOATS * 4) as u64;
const ENV_LIGHTS_AT: usize = 32;

/// Direção padrão em que a luz do sol viaja (normalizada).
const SOL_PADRAO: [f32; 3] = [-0.30151135, -0.90453410, -0.30151135];
/// Faixa aceita para o tamanho angular do disco do sol, em radianos.
const SOL_MIN: f32 = 0.001;
const SOL_MAX: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedLights {
    pub gpu: [f32; MAX_LIGHTS * LIGHT_GPU],
    pub n: u32,
    /// Índice da primeira direcional com sombra (-1 = nenhuma).
    pub shadow: i32,
}

impl PackedLights {
    pub fn empty() -> PackedLights {
        PackedLights { gpu: [0.0; MAX_LIGHTS * LIGHT_GPU], n: 0, shadow: -1 }
    }
}

fn num(v: f64, padrao: f32) -> f32 {
    if v.is_finite() { v as f32 } else { padrao }
}

fn unit(x: f32, y: f32, z: f32, padrao: [f32; 3]) -> [f32; 3] {
    let l = (x * x + y * y + z * z).sqrt();
    if l.is_finite() && l > 1e-6 { [x / l, y / l, z / l] } else { padrao }
}

/// Converte o buffer do TS no uniform. `n` fica preso a MAX_LIGHTS e ao que o
/// buffer realmente carrega; NaN vira o padrão de cada campo.
pub fn pack_lights(src: &[f64], n: usize) -> PackedLights {
    let mut out = PackedLights::empty();
    let n = n.min(MAX_LIGHTS).min(src.len() / LIGHT_IN);
    for i in 0..n {
        let s = &src[i * LIGHT_IN..(i + 1) * LIGHT_IN];
        let tipo = num(s[0], 0.0).round().clamp(0.0, 2.0);
        let dir = unit(num(s[4], 0.0), num(s[5], 0.0), num(s[6], 1.0), [0.0, 0.0, 1.0]);
        let mut c_in = num(s[12], 1.0).clamp(-1.0, 1.0);
        let mut c_out = num(s[13], 1.0).clamp(-1.0, 1.0);
        if c_in < c_out { std::mem::swap(&mut c_in, &mut c_out); }
        let sombra = num(s[14], 0.0) > 0.5;
        let g = &mut out.gpu[i * LIGHT_GPU..(i + 1) * LIGHT_GPU];
        g[0] = num(s[1], 0.0); g[1] = num(s[2], 0.0); g[2] = num(s[3], 0.0); g[3] = tipo;
        g[4] = dir[0]; g[5] = dir[1]; g[6] = dir[2]; g[7] = num(s[11], 0.0).max(0.0);
        g[8] = num(s[7], 1.0).max(0.0); g[9] = num(s[8], 1.0).max(0.0); g[10] = num(s[9], 1.0).max(0.0);
        g[11] = num(s[10], 1.0).max(0.0);
        g[12] = c_in; g[13] = c_out; g[14] = if sombra { 1.0 } else { 0.0 }; g[15] = 0.0;
        if out.shadow < 0 && tipo == 0.0 && sombra { out.shadow = i as i32; }
    }
    out.n = n as u32;
    out
}

// `attenuation`, `spot_factor` e `fog_factor` só rodam nos testes: o render
// usa as cópias em WGSL (`atenuacao`, `cone`, `neblina` em shader.rs).

/// `(1 - (d/alcance)²)²`, cortado em zero. Alcance <= 0 apaga a luz.
#[cfg(test)]
pub fn attenuation(d: f32, range: f32) -> f32 {
    if range <= 0.0 { return 0.0; }
    let r = d / range;
    let x = (1.0 - r * r).clamp(0.0, 1.0);
    x * x
}

/// Smoothstep entre o cosseno externo (0) e o interno (1); cones iguais = degrau.
#[cfg(test)]
pub fn spot_factor(cos_ang: f32, cos_in: f32, cos_out: f32) -> f32 {
    if cos_in - cos_out <= 1e-4 { return if cos_ang >= cos_out { 1.0 } else { 0.0 }; }
    let t = ((cos_ang - cos_out) / (cos_in - cos_out)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fração da cor do objeto que sobra a `dist` com neblina exponencial.
#[cfg(test)]
pub fn fog_factor(dist: f32, density: f32) -> f32 {
    if density <= 0.0 { 1.0 } else { (-density * dist).exp() }
}

pub fn fog_params(r: f64, g: f64, b: f64, d: f64) -> [f32; 4] {
    [num(r, 0.0).max(0.0), num(g, 0.0).max(0.0), num(b, 0.0).max(0.0), num(d, 0.0).max(0.0)]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyParams {
    /// 0 estrelas (o de hoje), 1 procedural, 2 cor, 3 panorama.
    pub modo: f32,
    pub topo: [f32; 3],
    pub horizonte: [f32; 3],
    pub chao: [f32; 3],
    /// Direção em que a luz do sol viaja (normalizada); o disco fica em -sol.
    pub sol: [f32; 3],
    pub tamanho_sol: f32,
    pub estrelas: f32,
    pub exposicao: f32,
    /// Id de `textureUpload` (>= 2) do panorama equirretangular; 0 = nenhum.
    pub textura: u64,
    /// 0 = escalar do `setLight` (legado), 1 = cor, 2 = céu (hemisférico).
    pub amb_modo: f32,
    pub amb_cor: [f32; 3],
    pub amb_intensidade: f32,
}

impl SkyParams {
    pub fn padrao() -> SkyParams {
        SkyParams {
            modo: 0.0, topo: [0.25, 0.45, 0.80], horizonte: [0.70, 0.80, 0.90], chao: [0.25, 0.23, 0.20],
            sol: SOL_PADRAO, tamanho_sol: 0.04, estrelas: 0.0, exposicao: 1.0, textura: 0,
            amb_modo: 0.0, amb_cor: [1.0, 1.0, 1.0], amb_intensidade: 0.25,
        }
    }

    /// Lê o buffer do `setSky`; o que faltar ou for NaN fica no padrão.
    pub fn from_f64(s: &[f64]) -> SkyParams {
        let p = SkyParams::padrao();
        let s = &s[..s.len().min(SKY_IN)]; // floats a mais são ignorados
        let at = |i: usize, d: f32| -> f32 { if i < s.len() { num(s[i], d) } else { d } };
        let rgb = |i: usize, d: [f32; 3]| [at(i, d[0]).max(0.0), at(i + 1, d[1]).max(0.0), at(i + 2, d[2]).max(0.0)];
        let tex = at(16, 0.0);
        SkyParams {
            modo: at(0, p.modo).round().clamp(0.0, 3.0),
            topo: rgb(1, p.topo),
            horizonte: rgb(4, p.horizonte),
            chao: rgb(7, p.chao),
            sol: unit(at(10, p.sol[0]), at(11, p.sol[1]), at(12, p.sol[2]), SOL_PADRAO),
            tamanho_sol: at(13, p.tamanho_sol).clamp(SOL_MIN, SOL_MAX),
            estrelas: at(14, p.estrelas).max(0.0),
            exposicao: at(15, p.exposicao).max(0.0),
            textura: if tex >= 2.0 { tex as u64 } else { 0 },
            amb_modo: at(17, p.amb_modo).round().clamp(0.0, 2.0),
            amb_cor: rgb(18, p.amb_cor),
            amb_intensidade: at(21, p.amb_intensidade).max(0.0),
        }
    }
}

/// O uniform `Env` inteiro, na ordem do struct WGSL (ver o teste de layout).
pub fn env_floats(l: &PackedLights, sky: &SkyParams, has_pano: bool, fog: [f32; 4]) -> [f32; ENV_FLOATS] {
    let mut e = [0f32; ENV_FLOATS];
    e[0] = l.n as f32; e[1] = l.shadow as f32; e[2] = sky.amb_modo; e[3] = sky.amb_intensidade;
    e[4..7].copy_from_slice(&sky.amb_cor);
    e[8] = sky.modo; e[9] = sky.exposicao; e[10] = sky.estrelas; e[11] = sky.tamanho_sol;
    e[12..15].copy_from_slice(&sky.topo);
    e[16..19].copy_from_slice(&sky.horizonte);
    e[20..23].copy_from_slice(&sky.chao);
    e[24..27].copy_from_slice(&sky.sol);
    e[27] = if has_pano { 1.0 } else { 0.0 };
    e[28..32].copy_from_slice(&fog);
    e[ENV_LIGHTS_AT..].copy_from_slice(&l.gpu);
    e
}
