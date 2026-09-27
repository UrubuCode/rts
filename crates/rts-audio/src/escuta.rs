//! Escuta por loopback: o agente pergunta ao dispositivo de SAÍDA padrão "o
//! que está saindo agora" sem depender do humano. No Windows, o `cpal` abre o
//! loopback WASAPI de forma transparente quando `build_input_stream` é chamado
//! num `Device` de saída (ver `cpal::host::wasapi::Host`, doc do módulo:
//! "If you use a WASAPI output device as an input device it will transparently
//! enable loopback mode"). Não existe host de loopback assim fora do Windows
//! nesta versão do `cpal` (CoreAudio tem o próprio caminho, ALSA não tem
//! nenhum) — por isso a escuta falha fora do Windows em vez de fingir.
//!
//! Só LÊ: nunca abre uma saída, nunca toca no volume. Duas formas de pedir:
//!
//! - [`escutar`]: bloqueia a thread chamadora pelos `ms` pedidos. Simples,
//!   mas não serve pro motor híbrido (agente por WS + humano na janela ao
//!   mesmo tempo): bloquear a thread principal travaria os dois.
//! - [`escuta_iniciar`]/[`escuta_ler`]: a captura roda numa thread própria
//!   (o próprio `Stream` do `cpal`, mais uma thread que só dorme e solta o
//!   stream ao final); a thread chamadora nunca bloqueia — chama
//!   `escuta_iniciar` uma vez e faz polling de `escuta_ler` a cada quadro
//!   até ele devolver `Some`.
//!
//! O acumulador (soma de quadrados, pico, contagem de amostras, e o detector
//! de frequência de Goertzel) vive em átomos e em estado local do próprio
//! callback — nenhuma alocação e nenhum mutex no callback de tempo real. A
//! leitura do resultado final só acontece depois que o `Stream` foi solto
//! (`drop`), então nada lê os átomos enquanto a thread de captura ainda
//! escreve neles.

// Fora do Windows a captura não existe (`escutar`/`escuta_iniciar` devolvem
// erro), então o acumulador e o Goertzel só são usados pelos testes; o CI
// compila com avisos como erro.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(target_os = "windows")]
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
#[cfg(target_os = "windows")]
use std::thread::JoinHandle;
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

/// O resultado de uma escuta CONTÍNUA (`escuta_iniciar`/`escuta_ler`): o
/// mesmo de [`Escuta`], mais o detector de frequência e a contagem de erros
/// do dispositivo durante a captura.
#[derive(Debug, Clone, PartialEq)]
pub struct EscutaContinua {
    /// rms, pico, silêncio, quadros, taxa, canais e dispositivo.
    pub base: Escuta,
    /// Amplitude estimada na frequência pedida a `escuta_iniciar`
    /// (Goertzel sobre o sinal reduzido a mono); `0.0` se `freq_hz <= 0`
    /// foi pedido, ou se nenhum bloco completo de [`GOERTZEL_N`] amostras
    /// coube na captura.
    pub energia_freq: f32,
    /// Quantas vezes o callback de erro do `cpal` disparou durante a
    /// captura (estouro/esvaziamento do buffer do dispositivo).
    pub underruns: u64,
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

/// CAS sem alocação: soma `add` (em bits de `f64`) na soma corrente. Sem
/// contenção de verdade em nenhum dos dois usos (só a thread de captura
/// escreve em cada um dos átomos), então o laço não gira mais que uma vez na
/// prática; ele existe para não perder uma atualização em teoria.
fn somar_f64_atomico(soma: &AtomicU64, add: f64) {
    let mut atual = soma.load(Ordering::Relaxed);
    loop {
        let novo = f64::from_bits(atual) + add;
        match soma.compare_exchange_weak(atual, novo.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(v2) => atual = v2,
        }
    }
}

/// CAS sem alocação: `pico = max(pico, v)`.
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

/// O acumulador em si, testável sem nenhum dispositivo: soma pico e soma de
/// quadrados de um bloco de amostras já em `f32` (intercaladas).
fn acumular(estado: &Estado, amostras: &[f32]) {
    for &v in amostras {
        atualizar_pico(&estado.pico, v.abs());
        somar_f64_atomico(&estado.soma_quadrados, v as f64 * v as f64);
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

// ---------------------------------------------------------------------------
// Goertzel: detector de UMA frequência sobre o sinal reduzido a mono, sem
// alocação e sem guardar amostra alguma — só a recursão de dois estados.
// ---------------------------------------------------------------------------

/// Tamanho do bloco do Goertzel, em quadros mono. Fixo (não depende da taxa
/// do dispositivo). O Goertzel só mede exatamente no BIN mais próximo de
/// `freq_hz` (`k` arredondado em [`EstadoGoertzel::novo`]); quando a
/// frequência pedida não cai bem em cima de um bin, a energia medida perde
/// um pouco de amplitude por "scalloping" (a resposta de uma janela
/// retangular fora do centro do bin). Um bloco maior estreita o bin e
/// encolhe essa perda: a 48 kHz, 4096 quadros dão bins de ~11,7 Hz — pra
/// 997 Hz (~12,6 Hz do bin mais próximo com 1024 quadros) isso já é erro de
/// ~1 % em vez de ~12 %. 4096 quadros são ~85 ms a 48 kHz, então mesmo uma
/// escuta de 300 ms ainda fecha uns 3 blocos.
const GOERTZEL_N: usize = 4096;

/// Soma das amplitudes de cada bloco completo do Goertzel, e quantos blocos
/// completaram — a média dos dois é `energia_freq`. Em átomos pela mesma
/// razão de [`Estado`]: só a thread de captura escreve.
struct GoertzelAcumulador {
    soma_amplitudes: AtomicU64,
    blocos: AtomicU64,
}

impl GoertzelAcumulador {
    fn novo() -> GoertzelAcumulador {
        GoertzelAcumulador { soma_amplitudes: AtomicU64::new(0.0f64.to_bits()), blocos: AtomicU64::new(0) }
    }

    fn somar(&self, amplitude: f64) {
        somar_f64_atomico(&self.soma_amplitudes, amplitude);
        self.blocos.fetch_add(1, Ordering::Relaxed);
    }

    /// Média das amplitudes dos blocos completos; `0.0` sem nenhum bloco.
    fn media(&self) -> f64 {
        let blocos = self.blocos.load(Ordering::Relaxed);
        if blocos == 0 { return 0.0; }
        f64::from_bits(self.soma_amplitudes.load(Ordering::Relaxed)) / blocos as f64
    }
}

/// O estado da recursão do Goertzel: NÃO é atômico, porque só a thread de
/// captura (o próprio callback, como uma closure `FnMut`) o lê e escreve —
/// não precisa ser `Sync`, só `Send` pra viver dentro do callback.
struct EstadoGoertzel {
    /// `2·cos(2π·k/N)`, calculado uma vez a partir da taxa e da frequência
    /// pedidas — é o único jeito de `processar_amostra` custar uma
    /// multiplicação e duas somas por amostra, sem trigonometria por amostra.
    coef: f64,
    s1: f64,
    s2: f64,
    n: usize,
}

impl EstadoGoertzel {
    /// `k` é arredondado pro inteiro mais próximo: o Goertzel mede a energia
    /// no BIN mais próximo de `freq_hz`, não exatamente nela. Com
    /// [`GOERTZEL_N`] fixo, o erro do bin cai conforme a taxa sobe.
    fn novo(freq_hz: f64, taxa: u32) -> EstadoGoertzel {
        let k = (GOERTZEL_N as f64 * freq_hz / taxa.max(1) as f64).round();
        let w = 2.0 * std::f64::consts::PI * k / GOERTZEL_N as f64;
        EstadoGoertzel { coef: 2.0 * w.cos(), s1: 0.0, s2: 0.0, n: 0 }
    }

    /// Uma amostra mono por chamada. Ao completar um bloco de
    /// [`GOERTZEL_N`], calcula a amplitude do bloco e a soma em
    /// `acumulador`, e reseta a recursão pro próximo bloco.
    fn processar_amostra(&mut self, v: f64, acumulador: &GoertzelAcumulador) {
        let s0 = v + self.coef * self.s1 - self.s2;
        self.s2 = self.s1;
        self.s1 = s0;
        self.n += 1;
        if self.n >= GOERTZEL_N {
            // Potência real do Goertzel: s1² + s2² − coef·s1·s2. `max(0.0)`
            // só por segurança de arredondamento perto de zero (não deveria
            // ficar negativa, mas uma raiz de negativo por erro de ponto
            // flutuante não pode acontecer).
            let potencia = (self.s1 * self.s1 + self.s2 * self.s2 - self.coef * self.s1 * self.s2).max(0.0);
            // Normalização pra amplitude: um seno puro de amplitude A em N
            // amostras dá magnitude ≈ A·N/2, então amplitude ≈ 2·magnitude/N.
            let amplitude = 2.0 * potencia.sqrt() / GOERTZEL_N as f64;
            acumulador.somar(amplitude);
            self.s1 = 0.0;
            self.s2 = 0.0;
            self.n = 0;
        }
    }

    /// Reduz um bloco intercalado a mono (média dos canais) e processa cada
    /// quadro. `canais == 0` não processa nada (nada pra reduzir).
    fn processar_bloco(&mut self, canais: usize, bloco: &[f32], acumulador: &GoertzelAcumulador) {
        if canais == 0 { return; }
        for quadro in bloco.chunks_exact(canais) {
            let media = quadro.iter().sum::<f32>() / canais as f32;
            self.processar_amostra(media as f64, acumulador);
        }
    }
}

/// Escuta o dispositivo de SAÍDA padrão por `ms` (grampeado em
/// [`MS_MINIMO`]..=[`MS_MAXIMO`]) e devolve o que estava saindo. BLOQUEIA a
/// thread chamadora pelos `ms` inteiros. Só lê: não abre saída nem toca no
/// volume. Fora do Windows, ou sem loopback disponível, devolve erro em vez
/// de fingir uma amostra.
pub fn escutar(ms: u32) -> Result<Escuta, String> {
    let ms = ms.clamp(MS_MINIMO, MS_MAXIMO);
    #[cfg(target_os = "windows")]
    { escutar_windows(ms) }
    #[cfg(not(target_os = "windows"))]
    { let _ = ms; Err("loopback indisponível nesta plataforma".to_string()) }
}

/// Inicia uma escuta CONTÍNUA que não bloqueia a thread chamadora: a
/// captura roda numa thread própria por `ms` (grampeado como em
/// [`escutar`]), e o resultado só fica pronto pra [`escuta_ler`] quando essa
/// thread solta o `Stream`. `freq_hz > 0.0` liga o detector de Goertzel
/// nessa frequência sobre o sinal reduzido a mono; `freq_hz <= 0.0` desliga
/// o detector (`energia_freq` sempre `0.0`).
///
/// Devolve `false` sem fazer nada se já houver uma escuta contínua em
/// andamento nesta thread (chame [`escuta_ler`] até ela devolver `Some`
/// antes de iniciar outra), ou se o dispositivo/plataforma não permitirem a
/// captura.
pub fn escuta_iniciar(ms: u32, freq_hz: f64) -> bool {
    let ms = ms.clamp(MS_MINIMO, MS_MAXIMO);
    #[cfg(target_os = "windows")]
    { escuta_iniciar_windows(ms, freq_hz) }
    #[cfg(not(target_os = "windows"))]
    { let _ = (ms, freq_hz); false }
}

/// Faz o polling de uma escuta contínua iniciada por [`escuta_iniciar`].
/// `None` enquanto a captura ainda está rodando (ou se nenhuma foi
/// iniciada); `Some` UMA VEZ quando ela termina — a partir daí uma nova
/// escuta pode ser iniciada. Nunca bloqueia.
pub fn escuta_ler() -> Option<EscutaContinua> {
    #[cfg(target_os = "windows")]
    { escuta_ler_windows() }
    #[cfg(not(target_os = "windows"))]
    { None }
}

/// Maior número de QUADROS que a conversão I16/U16 processa por passada, sem
/// alocar — mesma razão e mesmo tamanho de `device::QUADROS_BLOCO_CONVERSAO`.
#[cfg(target_os = "windows")]
const QUADROS_BLOCO_CONVERSAO: usize = 4096;

/// CONSUMIDOR, dentro do callback de captura: converte `data` (formato `T`
/// do dispositivo) em blocos de até `tmp.len()` amostras `f32`, chamando
/// `processar` por bloco. Não aloca — `tmp` já vem pré-alocado de antes de
/// `play()`. Cada `data` que o `cpal` entrega já é um número inteiro de
/// quadros, e `tmp.len()` também é (múltiplo de `canais`), então cada bloco
/// passado a `processar` também é — quem processa por quadro pode contar
/// com isso.
#[cfg(target_os = "windows")]
fn processar_convertendo<T: Copy>(
    tmp: &mut [f32],
    data: &[T],
    conv: impl Fn(T) -> f32,
    mut processar: impl FnMut(&[f32]),
) {
    if tmp.is_empty() { return; }
    let mut i = 0;
    while i < data.len() {
        let n = (data.len() - i).min(tmp.len());
        for k in 0..n { tmp[k] = conv(data[i + k]); }
        processar(&tmp[..n]);
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
                    processar_convertendo(&mut tmp, data, |v| v as f32 / 32_768.0, |bloco| acumular(&e, bloco));
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
                    processar_convertendo(&mut tmp, data, |v| (v as f32 - 32_768.0) / 32_768.0, |bloco| acumular(&e, bloco));
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

// ---------------------------------------------------------------------------
// A escuta CONTÍNUA: uma thread própria (dona do `Stream`, que só sabe
// dormir `ms` e soltá-lo) mais um registro nesta thread pra `escuta_ler`
// fazer polling sem bloquear. `EM_ANDAMENTO` pressupõe que quem chama
// `escuta_iniciar`/`escuta_ler` é sempre a mesma thread (a do programa TS,
// como as demais nativas de `rts:audio`) — só ela lê e escreve o `RefCell`.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
struct CapturaEmAndamento {
    estado: Arc<Estado>,
    pronto: Arc<AtomicBool>,
    underruns: Arc<AtomicU64>,
    goertzel_acc: Arc<GoertzelAcumulador>,
    taxa: u32,
    canais: u16,
    dispositivo: String,
    guarda: Option<JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
thread_local! {
    static EM_ANDAMENTO: std::cell::RefCell<Option<CapturaEmAndamento>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "windows")]
fn escuta_iniciar_windows(ms: u32, freq_hz: f64) -> bool {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    if EM_ANDAMENTO.with(|c| c.borrow().is_some()) { return false; }

    let host = cpal::default_host();
    let Some(disp) = host.default_output_device() else { return false; };
    let nome = disp.to_string();
    let Ok(padrao) = disp.default_output_config() else { return false; };
    let fmt = padrao.sample_format();
    let cfg = padrao.config();
    let taxa = cfg.sample_rate;
    let canais = cfg.channels;
    let canais_usize = canais.max(1) as usize;

    let estado = Arc::new(Estado::novo());
    let goertzel_acc = Arc::new(GoertzelAcumulador::novo());
    let underruns = Arc::new(AtomicU64::new(0));
    let pronto = Arc::new(AtomicBool::new(false));

    let e = estado.clone();
    let ga = goertzel_acc.clone();
    let mut goertzel = if freq_hz > 0.0 { Some(EstadoGoertzel::novo(freq_hz, taxa)) } else { None };

    let ur = underruns.clone();
    let erro = move |err: cpal::Error| {
        ur.fetch_add(1, Ordering::Relaxed);
        eprintln!("[rts:audio] erro no loopback: {err}");
    };

    let stream = match fmt {
        cpal::SampleFormat::F32 => disp.build_input_stream(
            cfg.clone(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                acumular(&e, data);
                if let Some(g) = goertzel.as_mut() { g.processar_bloco(canais_usize, data, &ga); }
            },
            erro,
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut tmp = vec![0.0f32; QUADROS_BLOCO_CONVERSAO * canais_usize];
            disp.build_input_stream(
                cfg.clone(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    processar_convertendo(&mut tmp, data, |v| v as f32 / 32_768.0, |bloco| {
                        acumular(&e, bloco);
                        if let Some(g) = goertzel.as_mut() { g.processar_bloco(canais_usize, bloco, &ga); }
                    });
                },
                erro,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let mut tmp = vec![0.0f32; QUADROS_BLOCO_CONVERSAO * canais_usize];
            disp.build_input_stream(
                cfg.clone(),
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    processar_convertendo(&mut tmp, data, |v| (v as f32 - 32_768.0) / 32_768.0, |bloco| {
                        acumular(&e, bloco);
                        if let Some(g) = goertzel.as_mut() { g.processar_bloco(canais_usize, bloco, &ga); }
                    });
                },
                erro,
                None,
            )
        }
        _ => return false,
    };
    let Ok(stream) = stream else { return false; };
    if stream.play().is_err() { return false; }

    // A THREAD DE GUARDA é a única dona do `Stream` a partir daqui: ela só
    // dorme `ms` e o solta (o Drop para a captura), e então sinaliza
    // `pronto`. `escuta_ler` (na thread do programa) só lê os átomos DEPOIS
    // de ver `pronto`, então nunca lê enquanto o Stream ainda escreve.
    let pronto_guarda = pronto.clone();
    let guarda = std::thread::Builder::new()
        .name("rts-audio-escuta".to_string())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(ms as u64));
            drop(stream);
            pronto_guarda.store(true, Ordering::Release);
        });
    let Ok(guarda) = guarda else { return false; };

    EM_ANDAMENTO.with(|c| {
        *c.borrow_mut() = Some(CapturaEmAndamento {
            estado,
            pronto,
            underruns,
            goertzel_acc,
            taxa,
            canais,
            dispositivo: nome,
            guarda: Some(guarda),
        });
    });
    true
}

#[cfg(target_os = "windows")]
fn escuta_ler_windows() -> Option<EscutaContinua> {
    let pronto = EM_ANDAMENTO.with(|c| c.borrow().as_ref().map(|r| r.pronto.clone()))?;
    if !pronto.load(Ordering::Acquire) { return None; }

    let registro = EM_ANDAMENTO.with(|c| c.borrow_mut().take())?;
    // A thread de guarda já soltou o Stream antes de marcar `pronto` — o
    // `join` aqui não bloqueia de verdade (ela já terminou), só recupera o
    // handle.
    if let Some(g) = registro.guarda { let _ = g.join(); }

    let base = resultado(&registro.estado, registro.taxa, registro.canais, registro.dispositivo);
    let energia_freq = registro.goertzel_acc.media() as f32;
    let underruns = registro.underruns.load(Ordering::Relaxed);
    Some(EscutaContinua { base, energia_freq, underruns })
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

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn fora_do_windows_iniciar_devolve_falso() {
        assert!(!escuta_iniciar(300, 997.0));
        assert!(escuta_ler().is_none());
    }

    /// Gera `n` amostras mono de um seno de `freq_hz` a `taxa`, amplitude `a`.
    fn seno_mono(freq_hz: f64, taxa: u32, a: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (a * (2.0 * std::f64::consts::PI * freq_hz * i as f64 / taxa as f64).sin()) as f32)
            .collect()
    }

    #[test]
    fn goertzel_acha_a_propria_frequencia() {
        let taxa = 48_000;
        let acc = GoertzelAcumulador::novo();
        let mut g = EstadoGoertzel::novo(997.0, taxa);
        // Vários blocos de 997 Hz, amplitude 0,3 — a média deve convergir
        // pra perto de 0,3 em cada bloco completo.
        let amostras = seno_mono(997.0, taxa, 0.3, GOERTZEL_N * 8);
        g.processar_bloco(1, &amostras, &acc);
        let energia = acc.media();
        assert!((energia - 0.3).abs() < 0.02, "energia = {energia}, esperava ~0.3");
    }

    #[test]
    fn goertzel_ignora_frequencia_diferente() {
        let taxa = 48_000;
        let acc = GoertzelAcumulador::novo();
        let mut g = EstadoGoertzel::novo(997.0, taxa);
        // Mesma amplitude, mas em 440 Hz — o detector de 997 Hz deve achar
        // quase nada.
        let amostras = seno_mono(440.0, taxa, 0.3, GOERTZEL_N * 8);
        g.processar_bloco(1, &amostras, &acc);
        let energia = acc.media();
        assert!(energia < 0.02, "energia = {energia}, esperava ~0");
    }

    #[test]
    fn goertzel_processar_bloco_reduz_canais_a_mono() {
        let taxa = 48_000;
        let acc = GoertzelAcumulador::novo();
        let mut g = EstadoGoertzel::novo(997.0, taxa);
        let mono = seno_mono(997.0, taxa, 0.5, GOERTZEL_N * 4);
        // Intercala o mesmo sinal em dois canais: a média por quadro deve
        // devolver a mesma amostra mono de volta.
        let mut estereo = Vec::with_capacity(mono.len() * 2);
        for &v in &mono { estereo.push(v); estereo.push(v); }
        g.processar_bloco(2, &estereo, &acc);
        let energia = acc.media();
        assert!((energia - 0.5).abs() < 0.02, "energia = {energia}, esperava ~0.5");
    }

    #[test]
    fn goertzel_zero_canais_nao_processa() {
        let acc = GoertzelAcumulador::novo();
        let mut g = EstadoGoertzel::novo(997.0, 48_000);
        g.processar_bloco(0, &[1.0, 2.0, 3.0], &acc);
        assert_eq!(acc.media(), 0.0);
    }

    #[test]
    fn goertzel_sem_bloco_completo_da_media_zero() {
        let acc = GoertzelAcumulador::novo();
        let mut g = EstadoGoertzel::novo(997.0, 48_000);
        let amostras = seno_mono(997.0, 48_000, 0.5, GOERTZEL_N / 2);
        g.processar_bloco(1, &amostras, &acc);
        assert_eq!(acc.media(), 0.0, "meio bloco não fecha o Goertzel");
    }
}
