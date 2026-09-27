//! O dispositivo: `cpal` drenando o anel numa thread do SO, ou o NULO, que
//! drena no mesmo ritmo e descarta. Os dois chamam `ring::drenar`.
//!
//! O nulo existe para testes e CI sem placa de som, e só quando o CHAMADOR o
//! pede (`flags` bit 0). Uma falha ao abrir o real NÃO cai no nulo: devolve
//! erro, o nativo responde 0 e o jogo segue mudo — um mixer que gasta o quadro
//! para ninguém ouvir é o que `compat/audio.ts` recusava, com razão.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::ring::{Compartilhado, drenar};

/// Bit de `flags` que pede o dispositivo nulo.
pub const FLAG_NULO: i64 = 1;
/// Segundos de áudio que o anel comporta (o programa mantém ~100 ms).
pub const SEGUNDOS_ANEL: usize = 1;
/// Taxa e canais do nulo quando o pedido deixa em 0.
const TAXA_NULA: u32 = 48_000;
const CANAIS_NULOS: u16 = 2;
/// Intervalo da thread do nulo e o maior bloco que ela drena de uma vez.
const PASSO_NULO: Duration = Duration::from_millis(2);
const BLOCO_NULO: usize = 1024;

/// O que o programa pediu. 0 = o padrão do dispositivo.
pub struct Pedido {
    /// Taxa desejada, 0 = a do dispositivo.
    pub taxa: u32,
    /// Canais desejados, 0 = os do dispositivo.
    pub canais: u16,
    /// Dispositivo nulo em vez do real.
    pub nulo: bool,
}

/// Uma saída aberta. Fechar é soltá-la.
pub struct Saida {
    /// Taxa efetiva.
    pub taxa: u32,
    /// Canais efetivos.
    pub canais: u16,
    /// É o nulo.
    pub nulo: bool,
    /// O que a thread do dispositivo lê.
    pub comp: Arc<Compartilhado>,
    parar: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    stream: Option<cpal::Stream>,
}

impl Drop for Saida {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
        // O stream do cpal para no drop dele.
        self.stream.take();
    }
}

/// Abre a saída pedida.
pub fn abrir(p: &Pedido) -> Result<Saida, String> {
    if p.nulo {
        let taxa = if p.taxa > 0 { p.taxa } else { TAXA_NULA };
        let canais = if p.canais > 0 { p.canais } else { CANAIS_NULOS };
        return abrir_nula(taxa, canais);
    }
    abrir_cpal(p)
}

fn abrir_nula(taxa: u32, canais: u16) -> Result<Saida, String> {
    let comp = Arc::new(Compartilhado::new(taxa as usize * canais as usize * SEGUNDOS_ANEL));
    let parar = Arc::new(AtomicBool::new(false));
    let (c, p) = (comp.clone(), parar.clone());
    let ch = canais as usize;
    let thread = std::thread::Builder::new()
        .name("rts-audio-nulo".to_string())
        .spawn(move || {
            let inicio = Instant::now();
            let mut entregues: u64 = 0;
            let mut rascunho = vec![0.0f32; BLOCO_NULO * ch];
            while !p.load(Ordering::Relaxed) {
                std::thread::sleep(PASSO_NULO);
                let devidos = (inicio.elapsed().as_secs_f64() * taxa as f64) as u64;
                while entregues < devidos {
                    let q = ((devidos - entregues) as usize).min(BLOCO_NULO);
                    drenar(&c, &mut rascunho[..q * ch], ch);
                    entregues += q as u64;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Saida { taxa, canais, nulo: true, comp, parar, thread: Some(thread), stream: None })
}

fn abrir_cpal(p: &Pedido) -> Result<Saida, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let disp = host.default_output_device().ok_or_else(|| "nenhum dispositivo de saída".to_string())?;
    let padrao = disp.default_output_config().map_err(|e| e.to_string())?;
    let fmt = padrao.sample_format();
    let mut cfg = padrao.config();
    if p.canais > 0 { cfg.channels = p.canais; }
    if p.taxa > 0 { cfg.sample_rate = p.taxa; }
    // Pedido que o dispositivo recusa (taxa ou canais) cai no padrão dele: quem
    // chamou lê `sample_rate`/`channels` e reamostra, que é o contrato.
    let (stream, cfg, comp) = match construir(&disp, cfg.clone(), fmt) {
        Ok((s, c)) => (s, cfg, c),
        Err(_) => {
            let base = padrao.config();
            let (s, c) = construir(&disp, base.clone(), fmt)?;
            (s, base, c)
        }
    };
    stream.play().map_err(|e| e.to_string())?;
    Ok(Saida {
        taxa: cfg.sample_rate,
        canais: cfg.channels,
        nulo: false,
        comp,
        parar: Arc::new(AtomicBool::new(false)),
        thread: None,
        stream: Some(stream),
    })
}

fn construir(disp: &cpal::Device, cfg: cpal::StreamConfig, fmt: cpal::SampleFormat)
    -> Result<(cpal::Stream, Arc<Compartilhado>), String> {
    use cpal::traits::DeviceTrait;
    let canais = cfg.channels as usize;
    let comp = Arc::new(Compartilhado::new(cfg.sample_rate as usize * canais * SEGUNDOS_ANEL));
    let c = comp.clone();
    let erro = |e: cpal::Error| eprintln!("[rts:audio] erro no dispositivo: {e}");
    let stream = match fmt {
        cpal::SampleFormat::F32 => disp.build_output_stream(
            cfg,
            move |d: &mut [f32], _: &cpal::OutputCallbackInfo| drenar(&c, d, canais),
            erro,
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut tmp: Vec<f32> = Vec::new();
            disp.build_output_stream(
                cfg,
                move |d: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    tmp.resize(d.len(), 0.0);
                    drenar(&c, &mut tmp, canais);
                    for (o, v) in d.iter_mut().zip(tmp.iter()) { *o = (v.clamp(-1.0, 1.0) * 32767.0) as i16; }
                },
                erro,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let mut tmp: Vec<f32> = Vec::new();
            disp.build_output_stream(
                cfg,
                move |d: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    tmp.resize(d.len(), 0.0);
                    drenar(&c, &mut tmp, canais);
                    for (o, v) in d.iter_mut().zip(tmp.iter()) { *o = (v.clamp(-1.0, 1.0) * 32767.0 + 32768.0) as u16; }
                },
                erro,
                None,
            )
        }
        outro => return Err(format!("formato de amostra {outro:?} sem conversão")),
    }
    .map_err(|e| e.to_string())?;
    Ok((stream, comp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    fn nulo_abre_com_o_padrao() {
        let s = abrir(&Pedido { taxa: 0, canais: 0, nulo: true }).expect("nulo sempre abre");
        assert_eq!((s.taxa, s.canais, s.nulo), (48_000, 2, true));
        assert!(s.comp.anel.capacidade() >= 48_000 * 2);
    }

    #[test]
    fn nulo_consome_em_tempo_real() {
        let s = abrir(&Pedido { taxa: 48_000, canais: 2, nulo: true }).unwrap();
        assert_eq!(s.comp.anel.escrever_quadros(&vec![0.25f32; 9600], 2), 9600, "4800 quadros = 100 ms");
        std::thread::sleep(Duration::from_millis(60));
        let consumidos = s.comp.consumidos.load(Ordering::Relaxed);
        // 60 ms a 48 kHz = 2880 quadros; folga para o agendador do SO
        assert!(consumidos >= 1500 && consumidos <= 4800, "consumidos = {consumidos}");
        let restantes = s.comp.anel.enfileiradas() / 2;
        assert!(restantes < 4800 && restantes > 0, "o nulo drena, mas não instantaneamente: {restantes}");
    }

    #[test]
    fn nulo_para_ao_fechar() {
        let s = abrir(&Pedido { taxa: 48_000, canais: 2, nulo: true }).unwrap();
        let comp = s.comp.clone();
        std::thread::sleep(Duration::from_millis(20));
        drop(s);
        let depois = comp.consumidos.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(comp.consumidos.load(Ordering::Relaxed), depois, "a thread parou no drop");
    }
}
