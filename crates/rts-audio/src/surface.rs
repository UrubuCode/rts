//! `rts:audio` — os doze membros, no molde de `rts-physics/src/surface.rs`:
//! views tipadas, ponteiros pegos num empréstimo curto e usados fora dele, e
//! views sobrepostas recusadas.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

use rts_core::entry::{self, Context, Provided};

use crate::device::{self, FLAG_NULO, Pedido, Saida};
use crate::mix::{self, DESC_FLOATS, NIVEL_FLOATS};
use crate::ogg;

/// `info` de `decode_ogg`: taxa, canais, quadros, erro.
pub const INFO_FLOATS: usize = 4;
/// `out` de `stats`: consumidos, faltas, enfileirados, taxa, canais, nulo.
pub const STATS_FLOATS: usize = 6;

/// O objeto do módulo.
pub fn namespace(context: &mut Context) -> u64 {
    entry::make_namespace(context, AUDIO)
}

const AUDIO: &[(&str, Provided)] = &[
    ("open_output", open_output),
    ("sample_rate", sample_rate),
    ("channels", channels),
    ("master_volume", master_volume),
    ("queued_frames", queued_frames),
    ("write", write),
    ("close", close),
    ("stats", stats),
    ("mix_add", mix_add),
    ("mix_level", mix_level),
    ("decode_ogg", decode_ogg),
    ("ogg_take", ogg_take),
];

thread_local! {
    /// Saídas abertas; o handle é o índice + 1 e nunca é reaproveitado.
    static SAIDAS: RefCell<Vec<Option<Saida>>> = const { RefCell::new(Vec::new()) };
    /// Clipes decodificados esperando `ogg_take`: (último handle, tabela).
    static DECODIFICADOS: RefCell<(u32, HashMap<u32, Vec<f32>>)> = RefCell::new((0, HashMap::new()));
}

/// Fecha todas as saídas. Chamado pelo host DEPOIS do programa e ANTES dos
/// destrutores de thread-local, pela mesma razão de `rts_ui::shutdown`: soltar o
/// stream WASAPI durante o descarregamento das DLLs é arriscar um `__fastfail`.
pub fn fechar_todas() {
    SAIDAS.with(|s| s.borrow_mut().clear());
    DECODIFICADOS.with(|d| d.borrow_mut().1.clear());
}

fn numero(v: u64, padrao: f64) -> f64 {
    match entry::number_of(v) { Some(n) if n.is_finite() => n, _ => padrao }
}

fn resposta(n: f64) -> u64 { entry::make_number(n) }

fn com_saida<R>(dev: u64, f: impl FnOnce(&Saida) -> R) -> Option<R> {
    let h = numero(dev, 0.0);
    if h < 1.0 { return None; }
    SAIDAS.with(|s| s.borrow().get(h as usize - 1).and_then(|o| o.as_ref()).map(f))
}

/// A janela de bytes de uma view, se o tamanho e o alinhamento servem a `tam`.
fn janela(context: &mut Context, value: u64, tam: usize) -> Option<(*mut u8, usize)> {
    let (p, n) = entry::bytes_pointer(context, value)?;
    if n % tam != 0 || (p as usize) % tam != 0 { return None; }
    Some((p, n))
}

/// Nenhum par de janelas (endereço, bytes) se sobrepõe; janela vazia não conta.
pub fn disjuntas(js: &[(usize, usize)]) -> bool {
    for (i, a) in js.iter().enumerate() {
        for b in js.iter().skip(i + 1) {
            if a.1 == 0 || b.1 == 0 { continue; }
            if a.0 < b.0 + b.1 && b.0 < a.0 + a.1 { return false; }
        }
    }
    true
}

// Uma view vazia pode vir com ponteiro nulo, e `from_raw_parts` exige ponteiro
// não nulo mesmo com comprimento 0: a janela vazia vira a fatia vazia.

/// # Safety
/// Janela viva, alinhada para `f32`, sem outra fatia sobre ela em escopo.
unsafe fn floats_mut<'a>(j: (*mut u8, usize)) -> &'a mut [f32] {
    if j.1 == 0 { return &mut []; }
    unsafe { std::slice::from_raw_parts_mut(j.0.cast::<f32>(), j.1 / 4) }
}
/// # Safety
/// Como [`floats_mut`], só leitura.
unsafe fn floats<'a>(j: (*mut u8, usize)) -> &'a [f32] {
    if j.1 == 0 { return &[]; }
    unsafe { std::slice::from_raw_parts(j.0.cast::<f32>(), j.1 / 4) }
}
/// # Safety
/// Janela viva, alinhada para `f64`, sem outra fatia sobre ela em escopo.
unsafe fn doubles_mut<'a>(j: (*mut u8, usize)) -> &'a mut [f64] {
    if j.1 == 0 { return &mut []; }
    unsafe { std::slice::from_raw_parts_mut(j.0.cast::<f64>(), j.1 / 8) }
}

/// `open_output(taxa, canais, flags)` → handle, 0 sem dispositivo.
extern "C" fn open_output(_e: u64, _t: u64, rate: u64, ch: u64, flags: u64, _c: u64) -> u64 {
    let pedido = Pedido {
        taxa: numero(rate, 0.0).clamp(0.0, 384_000.0) as u32,
        canais: numero(ch, 0.0).clamp(0.0, 8.0) as u16,
        nulo: (numero(flags, 0.0) as i64) & FLAG_NULO != 0,
    };
    match device::abrir(&pedido) {
        Ok(saida) => SAIDAS.with(|s| {
            let mut s = s.borrow_mut();
            s.push(Some(saida));
            resposta(s.len() as f64)
        }),
        Err(motivo) => {
            eprintln!("[rts:audio] sem saída de som: {motivo}");
            resposta(0.0)
        }
    }
}

/// `sample_rate(dev)` → taxa, 0 handle inválido.
extern "C" fn sample_rate(_e: u64, _t: u64, dev: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    resposta(com_saida(dev, |s| s.taxa as f64).unwrap_or(0.0))
}

/// `channels(dev)` → canais, 0 handle inválido.
extern "C" fn channels(_e: u64, _t: u64, dev: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    resposta(com_saida(dev, |s| s.canais as f64).unwrap_or(0.0))
}

/// `master_volume(dev, v)` — 0..4, aplicado na thread do dispositivo.
extern "C" fn master_volume(_e: u64, _t: u64, dev: u64, v: u64, _b: u64, _c: u64) -> u64 {
    let vol = numero(v, 1.0).clamp(0.0, 4.0) as f32;
    com_saida(dev, |s| s.comp.volume.store(vol.to_bits(), Ordering::Relaxed));
    entry::undefined_value()
}

/// `queued_frames(dev)` → quadros no anel, −1 handle inválido.
extern "C" fn queued_frames(_e: u64, _t: u64, dev: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    resposta(com_saida(dev, |s| (s.comp.anel.enfileiradas() / s.canais.max(1) as usize) as f64).unwrap_or(-1.0))
}

/// `write(dev, buf: Float32Array, amostras)` → amostras que entraram (quadros inteiros).
extern "C" fn write(_e: u64, _t: u64, dev: u64, buf: u64, samples: u64, _c: u64) -> u64 {
    let Some(j) = entry::with_runtime(|c| janela(c, buf, 4)) else { return resposta(0.0) };
    // SAFETY: janela checada em `janela`; o empréstimo acabou e nada abaixo roda
    // código do usuário nem aloca no motor.
    let amostras = unsafe { floats(j) };
    let n = (numero(samples, 0.0).max(0.0) as usize).min(amostras.len());
    let escritas = com_saida(dev, |s| s.comp.anel.escrever_quadros(&amostras[..n], s.canais as usize)).unwrap_or(0);
    resposta(escritas as f64)
}

/// `close(dev)`.
extern "C" fn close(_e: u64, _t: u64, dev: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let h = numero(dev, 0.0);
    if h >= 1.0 {
        let saida = SAIDAS.with(|s| s.borrow_mut().get_mut(h as usize - 1).and_then(|o| o.take()));
        drop(saida);
    }
    entry::undefined_value()
}

/// `stats(dev, out: Float64Array)` → 1, ou 0 se o handle ou `out` não servem.
extern "C" fn stats(_e: u64, _t: u64, dev: u64, out: u64, _b: u64, _c: u64) -> u64 {
    let Some(j) = entry::with_runtime(|c| janela(c, out, 8)) else { return resposta(0.0) };
    // SAFETY: como em `write`.
    let o = unsafe { doubles_mut(j) };
    if o.len() < STATS_FLOATS { return resposta(0.0); }
    let ok = com_saida(dev, |s| {
        o[0] = s.comp.consumidos.load(Ordering::Relaxed) as f64;
        o[1] = s.comp.faltas.load(Ordering::Relaxed) as f64;
        o[2] = (s.comp.anel.enfileiradas() / s.canais.max(1) as usize) as f64;
        o[3] = s.taxa as f64;
        o[4] = s.canais as f64;
        o[5] = if s.nulo { 1.0 } else { 0.0 };
    });
    resposta(if ok.is_some() { 1.0 } else { 0.0 })
}

/// `mix_add(dst, src, desc)` → quadros mixados; 0 = recusa.
extern "C" fn mix_add(_e: u64, _t: u64, dst: u64, src: u64, desc: u64, _c: u64) -> u64 {
    let Some([jd, js, jx]) = entry::with_runtime(|c| {
        Some([janela(c, dst, 4)?, janela(c, src, 4)?, janela(c, desc, 8)?])
    }) else { return resposta(0.0) };
    if !disjuntas(&[(jd.0 as usize, jd.1), (js.0 as usize, js.1), (jx.0 as usize, jx.1)]) { return resposta(0.0); }
    // SAFETY: três janelas vivas, alinhadas e disjuntas; o empréstimo acabou.
    let (d, s, x) = unsafe { (floats_mut(jd), floats(js), doubles_mut(jx)) };
    if x.len() < DESC_FLOATS { return resposta(0.0); }
    resposta(mix::mix_add(d, s, x) as f64)
}

/// `mix_level(buf, nivel)` → quadros medidos.
extern "C" fn mix_level(_e: u64, _t: u64, buf: u64, nivel: u64, _b: u64, _c: u64) -> u64 {
    let Some([jb, jn]) = entry::with_runtime(|c| Some([janela(c, buf, 4)?, janela(c, nivel, 8)?])) else {
        return resposta(0.0);
    };
    if !disjuntas(&[(jb.0 as usize, jb.1), (jn.0 as usize, jn.1)]) { return resposta(0.0); }
    // SAFETY: como em `mix_add`.
    let (b, n) = unsafe { (floats_mut(jb), doubles_mut(jn)) };
    if n.len() < NIVEL_FLOATS { return resposta(0.0); }
    resposta(mix::mix_level(b, n) as f64)
}

/// `decode_ogg(bytes, info)` → handle para `ogg_take`, 0 = recusa (`info[3]` diz por quê).
extern "C" fn decode_ogg(_e: u64, _t: u64, bytes: u64, info: u64, _b: u64, _c: u64) -> u64 {
    let Some(dados) = entry::with_runtime(|c| entry::bytes_of(c, bytes)) else { return resposta(0.0) };
    // Decodifica ANTES de pegar o ponteiro de `info`: nada do motor roda aqui,
    // mas um ponteiro não precisa viver mais que o necessário.
    let resultado = ogg::decodificar(&dados);
    let Some(j) = entry::with_runtime(|c| janela(c, info, 8)) else { return resposta(0.0) };
    // SAFETY: como em `write`.
    let i = unsafe { doubles_mut(j) };
    if i.len() < INFO_FLOATS { return resposta(0.0); }
    match resultado {
        Ok(d) => {
            i[0] = d.taxa as f64;
            i[1] = d.canais as f64;
            i[2] = (d.amostras.len() / d.canais.max(1) as usize) as f64;
            i[3] = 0.0;
            let h = DECODIFICADOS.with(|t| {
                let mut t = t.borrow_mut();
                t.0 += 1;
                let h = t.0;
                t.1.insert(h, d.amostras);
                h
            });
            resposta(h as f64)
        }
        Err(e) => {
            i[0] = 0.0; i[1] = 0.0; i[2] = 0.0; i[3] = e.codigo();
            resposta(0.0)
        }
    }
}

/// `ogg_take(handle, dst)` → amostras copiadas; libera o handle.
extern "C" fn ogg_take(_e: u64, _t: u64, handle: u64, dst: u64, _b: u64, _c: u64) -> u64 {
    let h = numero(handle, 0.0);
    if h < 1.0 { return resposta(0.0); }
    let Some(amostras) = DECODIFICADOS.with(|t| t.borrow_mut().1.remove(&(h as u32))) else { return resposta(0.0) };
    let Some(j) = entry::with_runtime(|c| janela(c, dst, 4)) else { return resposta(0.0) };
    // SAFETY: como em `write`.
    let d = unsafe { floats_mut(j) };
    let n = d.len().min(amostras.len());
    d[..n].copy_from_slice(&amostras[..n]);
    resposta(n as f64)
}

#[cfg(test)]
mod tests {
    use super::disjuntas;

    #[test]
    fn janelas_sobrepostas_sao_recusadas() {
        assert!(disjuntas(&[(1000, 16), (1016, 16), (2000, 8)]));
        assert!(!disjuntas(&[(1000, 16), (1008, 16)]), "sobreposição parcial");
        assert!(!disjuntas(&[(1000, 16), (1000, 16)]), "a mesma view");
        assert!(!disjuntas(&[(1000, 64), (1016, 8)]), "uma dentro da outra");
        assert!(disjuntas(&[(1000, 0), (1000, 16)]), "janela vazia não sobrepõe");
    }
}
