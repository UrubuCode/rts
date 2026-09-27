//! `rts:particles` — a superfície `extern "C"` de `particlesStep`, no molde
//! de `rts-audio/src/surface.rs` (`mix_add`): views tipadas sobre memória do
//! runtime, um empréstimo curto por chamada, sem alocação.
//!
//! # `particlesStep(pool, params, dt, out)` — a assinatura TS exata
//!
//! ```ts
//! declare function particlesStep(
//!   pool: Float64Array,   // P_FLOATS (14) colunas por partícula, ver kernel.rs
//!   params: Float64Array, // PARAMS_FLOATS (42), ver kernel.rs
//!   dt: number,
//!   out: Float32Array,    // >= (pool.length/14) * 9 floats
//! ): number; // partículas vivas escritas em `out` (compactado)
//! ```
//!
//! Os 4 argumentos JS mapeiam 1:1 nos 4 slots `u64` que a ABI `Provided`
//! oferece (depois de `engine`/`task`, sempre implícitos) — não precisou de
//! empacotamento, ao contrário de `drawParticlesTex` (5 argumentos lógicos,
//! empacotados num objeto). `pool`/`out` são mutados in-place (o mesmo
//! `Float64Array`/`Float32Array` reaproveitado entre quadros que o chamador
//! já mantém — `ParticleSystem.pool.dados`/`saidaBuf`); `params` é só lido.
//!
//! `dt` chega como o valor bruto do argumento (`numero`, que decodifica o
//! `u64` da ABI de volta a `f64` e usa `0.0` como padrão para "não é um
//! número" — o kernel trata NaN/negativo à parte, ver `kernel.rs`).

use rts_core::entry::{self, Context, Provided};

use crate::kernel;

/// Os membros de `rts:particles`.
pub const MEMBERS: &[(&str, Provided)] = &[("particlesStep", particles_step)];

/// O objeto do módulo.
pub fn namespace(context: &mut Context) -> u64 {
    entry::make_namespace(context, MEMBERS)
}

/// Decodifica um argumento `u64` de volta a `f64`; `padrao` se não for um
/// número (mesmo helper de `rts-audio/src/surface.rs::numero`).
fn numero(v: u64, padrao: f64) -> f64 {
    match entry::number_of(v) {
        Some(n) if n.is_finite() => n,
        _ => padrao,
    }
}

fn resposta(n: f64) -> u64 {
    entry::make_number(n)
}

/// A janela de bytes de uma view, se o tamanho e o alinhamento servem a
/// `tam` (o mesmo `janela` de `rts-audio/src/surface.rs`).
fn janela(context: &mut Context, value: u64, tam: usize) -> Option<(*mut u8, usize)> {
    let (p, n) = entry::bytes_pointer(context, value)?;
    if n % tam != 0 || (p as usize) % tam != 0 {
        return None;
    }
    Some((p, n))
}

/// Nenhum par de janelas se sobrepõe; janela vazia não conta. Mesmo
/// `disjuntas` de `rts-audio/src/surface.rs`.
fn disjuntas(js: &[(usize, usize)]) -> bool {
    for (i, a) in js.iter().enumerate() {
        for b in js.iter().skip(i + 1) {
            if a.1 == 0 || b.1 == 0 {
                continue;
            }
            if a.0 < b.0 + b.1 && b.0 < a.0 + a.1 {
                return false;
            }
        }
    }
    true
}

/// # Safety
/// Janela viva, alinhada para `f64`, sem outra fatia sobre ela em escopo.
unsafe fn doubles_mut<'a>(j: (*mut u8, usize)) -> &'a mut [f64] {
    if j.1 == 0 {
        return &mut [];
    }
    unsafe { std::slice::from_raw_parts_mut(j.0.cast::<f64>(), j.1 / 8) }
}
/// # Safety
/// Como [`doubles_mut`], só leitura.
unsafe fn doubles<'a>(j: (*mut u8, usize)) -> &'a [f64] {
    if j.1 == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(j.0.cast::<f64>(), j.1 / 8) }
}
/// # Safety
/// Janela viva, alinhada para `f32`, sem outra fatia sobre ela em escopo.
unsafe fn floats_mut<'a>(j: (*mut u8, usize)) -> &'a mut [f32] {
    if j.1 == 0 {
        return &mut [];
    }
    unsafe { std::slice::from_raw_parts_mut(j.0.cast::<f32>(), j.1 / 4) }
}

/// `particlesStep(pool, params, dt, out)` → partículas vivas escritas,
/// compactadas em `out`. `0` se `pool`/`params`/`out` não servem (tamanho,
/// alinhamento, ou `pool`/`out` se sobrepõem) — nunca lê/escreve fora das
/// views, nunca panica.
extern "C" fn particles_step(_e: u64, _t: u64, pool: u64, params: u64, dt: u64, out: u64) -> u64 {
    let Some([jp, jx, jo]) = entry::with_runtime(|c| {
        Some([janela(c, pool, 8)?, janela(c, params, 8)?, janela(c, out, 4)?])
    }) else {
        return resposta(0.0);
    };
    if !disjuntas(&[(jp.0 as usize, jp.1), (jx.0 as usize, jx.1), (jo.0 as usize, jo.1)]) {
        return resposta(0.0);
    }
    let dt_val = numero(dt, 0.0);
    // SAFETY: três janelas vivas, alinhadas e disjuntas (checado acima); o
    // empréstimo acaba antes de qualquer coisa abaixo rodar código do
    // usuário ou alocar no motor — mesmo argumento de `mix_add`.
    let (p, x, o) = unsafe { (doubles_mut(jp), doubles(jx), floats_mut(jo)) };
    resposta(kernel::particles_step(p, x, dt_val, o) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjuntas_recusa_sobreposicao_mas_aceita_janela_vazia() {
        assert!(disjuntas(&[(0, 8), (8, 8)]));
        assert!(!disjuntas(&[(0, 16), (8, 8)]));
        assert!(disjuntas(&[(0, 0), (0, 100)]), "janela vazia nunca conta como sobreposição");
    }
}
