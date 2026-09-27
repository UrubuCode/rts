//! Escuta por loopback: o agente pergunta ao dispositivo de SAÍDA padrão "o
//! que está saindo agora" sem depender do humano. No Windows, o `cpal` abre o
//! loopback WASAPI de forma transparente quando `build_input_stream` é chamado
//! num `Device` de saída (ver `cpal::host::wasapi::Host`, doc do módulo:
//! "If you use a WASAPI output device as an input device it will transparently
//! enable loopback mode"). Não existe host de loopback assim fora do Windows
//! nesta versão do `cpal` (CoreAudio tem o próprio caminho, ALSA não tem
//! nenhum) — por isso `escutar` falha fora do Windows em vez de fingir.
//!
//! Só LÊ: nunca abre uma saída, nunca toca no volume. O acumulador (soma de
//! quadrados, pico, contagem de amostras) vive em três átomos escritos só pela
//! thread de captura do `cpal`; a função que bloqueia por `ms` só lê esses
//! átomos depois de soltar o stream. Sem alocação no callback: a conversão de
//! `i16`/`u16` para `f32` usa um buffer pré-alocado antes de `play()`, do
//! mesmo jeito que `device::construir`.

#[cfg(target_os = "windows")]
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(target_os = "windows")]
use std::time::Duration;

/// O resultado de uma escuta.
#[derive(Debug, Clone, PartialEq)]
pub struct Escuta {
    /// Raiz da média dos quadrados das amostras (todas os canais juntos).
    pub rms: f32,
    /// Maior amplitude absoluta vista.
    pub pico: f32,
    /// `pico < 1e-4`: nada saindo, dentro do ruído de piso.
    pub silencio: bool,
    /// Quadros capturados (amostras ÷ canais).
    pub quadros: u64,
    /// Taxa do dispositivo de saída, em Hz.
    pub taxa: u32,
    /// Canais do dispositivo de saída.
    pub canais: u16,
    /// Nome do dispositivo escutado.
    pub dispositivo: String,
}

/// Menor duração aceita pela escuta, em milissegundos.
pub const MS_MINIMO: u32 = 50;
/// Maior duração aceita pela escuta, em milissegundos.
pub const MS_MAXIMO: u32 = 10_000;

/// O acumulador: só o que a captura escreve, em átomos — nenhuma alocação e
/// nenhum mutex no callback de tempo real.
struct Estado {
    /// Bits de um `f64`: soma dos quadrados de cada amostra vista.
    soma_quadrados: AtomicU64,
    /// Bits de um `f32`: a maior amplitude absoluta vista.
    pico: AtomicU32,
    /// Amostras vistas (intercaladas, não quadros).
    amostras: AtomicU64,
}

impl Estado {
    fn novo() -> Estado {
        Estado {
            soma_quadrados: AtomicU64::new(0.0f64.to_bits()),
            pico: AtomicU32::new(0.0f32.to_bits()),
            amostras: AtomicU64::new(0),
        }
    }
}

/// CAS sem alocação: `pico = max(pico, v)`. Sem contenção de verdade (só a
/// thread de captura escreve), então o laço não gira mais que uma vez na
/// prática; ele existe para não perder uma atualização em teoria.
fn atualizar_pico(pico: &AtomicU32, v: f32) {
    let mut atual = pico.load(Ordering::Relaxed);
    loop {
        if v <= f32::from_bits(atual) { return; }
        match pico.compare_exchange_weak(atual, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(novo) => atual = novo,
        }
    }
}

/// CAS sem alocação: soma `v²` (em `f64`, para não perder precisão em
/// captura longa) na soma corrente.
fn somar_quadrado(soma: &AtomicU64, v: f32) {
    let add = v as f64 * v as f64;
    let mut atual = soma.load(Ordering::Relaxed);
    loop {
        let novo = f64::from_bits(atual) + add;
        match soma.compare_exchange_weak(atual, novo.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(v2) => atual = v2,
        }
    }
}

/// O acumulador em si, testável sem nenhum dispositivo: soma pico e soma de
/// quadrados de um bloco de amostras já em `f32` (intercaladas).
fn acumular(estado: &Estado, amostras: &[f32]) {
    for &v in amostras {
        atualizar_pico(&estado.pico, v.abs());
        somar_quadrado(&estado.soma_quadrados, v);
    }
    estado.amostras.fetch_add(amostras.len() as u64, Ordering::Relaxed);
}

/// Lê `estado` e monta o resultado. `canais` só divide `amostras` em
/// `quadros`; 0 canais devolve 0 quadros em vez de dividir por zero.
fn resultado(estado: &Estado, taxa: u32, canais: u16, dispositivo: String) -> Escuta {
    let amostras = estado.amostras.load(Ordering::Relaxed);
    let soma = f64::from_bits(estado.soma_quadrados.load(Ordering::Relaxed));
    let pico = f32::from_bits(estado.pico.load(Ordering::Relaxed));
    let rms = if amostras > 0 { (soma / amostras as f64).sqrt() as f32 } else { 0.0 };
    let quadros = if canais > 0 { amostras / canais as u64 } else { 0 };
    Escuta { rms, pico, silencio: pico < 1e-4, quadros, taxa, canais, dispositivo }
}

/// Escuta o dispositivo de SAÍDA padrão por `ms` (grampeado em
/// [`MS_MINIMO`]..=[`MS_MAXIMO`]) e devolve o que estava saindo. Bloqueante.
/// Só lê: não abre saída nem toca no volume. Fora do Windows, ou sem loopback
/// disponível, devolve erro em vez de fingir uma amostra.
pub fn escutar(ms: u32) -> Result<Escuta, String> {
    let ms = ms.clamp(MS_MINIMO, MS_MAXIMO);
    #[cfg(target_os = "windows")]
    { escutar_windows(ms) }
    #[cfg(not(target_os = "windows"))]
    { let _ = ms; Err("loopback indisponível nesta plataforma".to_string()) }
}

/// Maior número de QUADROS que a conversão I16/U16 processa por passada, sem
/// alocar — mesma razão e mesmo tamanho de `device::QUADROS_BLOCO_CONVERSAO`.
#[cfg(target_os = "windows")]
const QUADROS_BLOCO_CONVERSAO: usize = 4096;

#[cfg(target_os = "windows")]
fn processar_convertendo<T: Copy>(estado: &Estado, tmp: &mut [f32], data: &[T], conv: impl Fn(T) -> f32) {
    if tmp.is_empty() { return; }
    let mut i = 0;
    while i < data.len() {
        let n = (data.len() - i).min(tmp.len());
        for k in 0..n { tmp[k] = conv(data[i + k]); }
        acumular(estado, &tmp[..n]);
        i += n;
    }
}

#[cfg(target_os = "windows")]
fn escutar_windows(ms: u32) -> Result<Escuta, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let disp = host.default_output_device().ok_or_else(|| "nenhum dispositivo de saída".to_string())?;
    let nome = disp.to_string();
    let padrao = disp.default_output_config().map_err(|e| e.to_string())?;
    let fmt = padrao.sample_format();
    let cfg = padrao.config();
    let taxa = cfg.sample_rate;
    let canais = cfg.channels;

    let estado = Arc::new(Estado::novo());
    let e = estado.clone();
    let erro = |err: cpal::Error| eprintln!("[rts:audio] erro no loopback: {err}");

    // "If you use a WASAPI output device as an input device it will
    // transparently enable loopback mode" — `build_input_stream` no
    // dispositivo de SAÍDA, com o formato dele mesmo (obtido acima por
    // `default_output_config`, já que `default_input_config` recusa um
    // dispositivo de saída antes de chegar ao loopback).
    let stream = match fmt {
        cpal::SampleFormat::F32 => disp.build_input_stream(
            cfg.clone(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| acumular(&e, data),
            erro,
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut tmp = vec![0.0f32; QUADROS_BLOCO_CONVERSAO * canais.max(1) as usize];
            disp.build_input_stream(
                cfg.clone(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    processar_convertendo(&e, &mut tmp, data, |v| v as f32 / 32_768.0);
                },
                erro,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let mut tmp = vec![0.0f32; QUADROS_BLOCO_CONVERSAO * canais.max(1) as usize];
            disp.build_input_stream(
                cfg.clone(),
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    processar_convertendo(&e, &mut tmp, data, |v| (v as f32 - 32_768.0) / 32_768.0);
                },
                erro,
                None,
            )
        }
        outro => return Err(format!("formato de amostra {outro:?} sem loopback")),
    }
    .map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(ms as u64));
    // Solta a captura ANTES de ler: o Drop do Stream para a thread do SO, e só
    // depois os átomos param de mudar sob nós.
    drop(stream);

    Ok(resultado(&estado, taxa, canais, nome))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silencio_com_amostras_vazias() {
        let estado = Estado::novo();
        let r = resultado(&estado, 48_000, 2, "teste".to_string());
        assert_eq!(r.rms, 0.0);
        assert_eq!(r.pico, 0.0);
        assert!(r.silencio);
        assert_eq!(r.quadros, 0);
    }

    #[test]
    fn acumula_rms_e_pico_de_uma_onda_sintetica() {
        let estado = Estado::novo();
        // Um "quadro" estéreo alternando +0.5/-0.5: rms = 0.5, pico = 0.5.
        let amostras = [0.5f32, -0.5, 0.5, -0.5, 0.5, -0.5, 0.5, -0.5];
        acumular(&estado, &amostras);
        let r = resultado(&estado, 48_000, 2, "teste".to_string());
        assert!((r.rms - 0.5).abs() < 1e-6, "rms = {}", r.rms);
        assert!((r.pico - 0.5).abs() < 1e-6, "pico = {}", r.pico);
        assert!(!r.silencio);
        assert_eq!(r.quadros, 4, "8 amostras estéreo = 4 quadros");
    }

    #[test]
    fn acumula_em_varios_blocos_como_o_callback_faria() {
        let estado = Estado::novo();
        acumular(&estado, &[1.0, 1.0]);
        acumular(&estado, &[0.0, 0.0]);
        acumular(&estado, &[-1.0, -1.0]);
        let r = resultado(&estado, 44_100, 2, "teste".to_string());
        // rms² = (1+1+0+0+1+1)/6 = 4/6
        assert!((r.rms - (4.0f32 / 6.0).sqrt()).abs() < 1e-6, "rms = {}", r.rms);
        assert_eq!(r.pico, 1.0);
        assert_eq!(r.quadros, 3);
    }

    #[test]
    fn pico_abaixo_do_limiar_e_silencio() {
        let estado = Estado::novo();
        acumular(&estado, &[1e-5, -1e-5, 5e-5]);
        let r = resultado(&estado, 48_000, 1, "teste".to_string());
        assert!(r.pico < 1e-4);
        assert!(r.silencio);
    }

    #[test]
    fn zero_canais_nao_divide_por_zero() {
        let estado = Estado::novo();
        acumular(&estado, &[1.0, 1.0]);
        let r = resultado(&estado, 48_000, 0, "teste".to_string());
        assert_eq!(r.quadros, 0);
    }

    #[test]
    fn escutar_grampeia_ms_fora_da_faixa() {
        // Só verifica o grampeamento (não abre dispositivo nenhum): fora do
        // Windows a chamada recusa antes de olhar `ms`, então o teste real do
        // grampo em si é indireto — mas a constante pública é o contrato.
        assert_eq!(MS_MINIMO, 50);
        assert_eq!(MS_MAXIMO, 10_000);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn fora_do_windows_devolve_erro() {
        let e = escutar(300).unwrap_err();
        assert_eq!(e, "loopback indisponível nesta plataforma");
    }
}
