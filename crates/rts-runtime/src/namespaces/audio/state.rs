//! Estado global do namespace `audio` — gerência dos streams de saída.
//!
//! ## Por que um shard-map próprio e não `Entry` global?
//!
//! `cpal::Stream` é `!Send` (não pode cruzar threads). O `Entry` do
//! HandleTable precisa ser `Send` (atravessa shards protegidos por Mutex),
//! então o Stream não cabe lá. O modelo correto — validado no spike
//! `examples/audio_spike.rs` — é manter o `cpal::Stream` preso numa thread de
//! áudio dedicada e expor ao resto do programa apenas o que É `Send + Sync`:
//! o ring buffer (`Arc<Mutex<VecDeque<f32>>>`) e contadores atômicos.
//!
//! O que cruza o boundary extern "C" é só um `u64` opaco (o handle). Os tipos
//! Rust-rich vivem aqui, indexados por esse handle — mesmo padrão de
//! `http_server`/`tokio_ctx` descrito em `.claude/rules/02-runtime.md`.
//!
//! ## Modelo de fornecimento de samples (pull)
//!
//! O código TS controla o loop: gera samples, consulta `available_frames` e
//! empurra via `write`. A thread de áudio nunca chama TS de volta — ela só
//! drena o ring no callback real-time. Zero risco de rodar GC/alocação na
//! thread RT.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

/// Ring buffer de samples f32 intercalados (interleaved por canal).
/// Produtor: TS (via `write`). Consumidor: callback de áudio (thread RT).
pub type Ring = Arc<Mutex<VecDeque<f32>>>;

/// Handle de um stream de saída ativo. Tudo aqui é `Send + Sync`; o
/// `cpal::Stream` (`!Send`) fica na thread de áudio, não neste struct.
pub struct OutputHandle {
    pub ring: Ring,
    pub sample_rate: u32,
    pub channels: u16,
    /// Capacidade do ring em FRAMES (samples = frames * channels). `write`
    /// nunca deixa o ring passar disso → backpressure.
    pub capacity_frames: usize,
    /// Ganho master aplicado no callback (bits f64, lidos como f32).
    pub master_gain: Arc<Mutex<f32>>,
    /// Contagem de underruns (samples pedidos pelo device sem dado no ring).
    pub underruns: Arc<AtomicU64>,
    /// Sinaliza a thread de áudio para encerrar (drop do Stream).
    stop: Arc<AtomicBool>,
    /// Thread dona do `cpal::Stream`. `join` no `close`.
    thread: Option<std::thread::JoinHandle<()>>,
    /// Sinaliza que a thread terminou de inicializar (sucesso ou falha).
    ready: Arc<(Mutex<Option<bool>>, std::sync::Condvar)>,
}

static STREAMS: OnceLock<Mutex<HashMap<u64, OutputHandle>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn streams() -> &'static Mutex<HashMap<u64, OutputHandle>> {
    STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Config do device de saída default (sample_rate, channels). 0,0 se não há
/// device ou não foi possível consultar.
pub fn default_output_config() -> (u32, u16) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        return (0, 0);
    };
    match device.default_output_config() {
        Ok(cfg) => (cfg.sample_rate(), cfg.channels()),
        Err(_) => (0, 0),
    }
}

/// Abre um stream de saída. Retorna o handle (>0) ou 0 em falha.
///
/// IMPORTANTE: o device é SEMPRE aberto com a config NATIVA do default
/// (`default_output_config`). Os parâmetros `sample_rate`/`channels` pedidos
/// NÃO forçam o device — no WASAPI shared o device é preso ao formato do mixer,
/// e forçar valores != nativo faz o callback do cpal rodar fora de tempo-real
/// (o ring nunca acumula, `available_frames` mente, e o áudio fica picotado —
/// bug diagnosticado 2026-06). O TS descobre o rate/canais reais via
/// `sample_rate()`/`channels()` e adapta (resample/upmix), como o player de WAV
/// já faz. `capacity_frames` limita o ring (backpressure); 0 → ~500ms.
pub fn open_output(_sample_rate: u32, _channels: u16, capacity_frames: usize) -> u64 {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        return 0;
    };
    let Ok(default_cfg) = device.default_output_config() else {
        return 0;
    };

    // Hoje só F32 (formato mais comum no WASAPI/CoreAudio). Outros formatos
    // exigiriam conversão no callback — fica para uma fase posterior.
    if default_cfg.sample_format() != SampleFormat::F32 {
        return 0;
    }

    // Config NATIVA — garante callback em tempo-real. `config()` carrega o
    // buffer_size correto do device, não BufferSize::Default arbitrário.
    let stream_config = default_cfg.config();
    let real_sr = default_cfg.sample_rate();
    let real_ch = default_cfg.channels();
    let real_cap = if capacity_frames == 0 {
        (real_sr as usize / 2).max(1) // ~500ms
    } else {
        capacity_frames
    };

    let ring: Ring = Arc::new(Mutex::new(VecDeque::with_capacity(real_cap * real_ch as usize)));
    let master_gain = Arc::new(Mutex::new(1.0_f32));
    let underruns = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new((Mutex::new(None::<bool>), std::sync::Condvar::new()));

    // A thread de áudio é dona do `cpal::Stream` (!Send). Ela monta o stream
    // com a config nativa, dá play, e dorme até `stop`. O callback só drena.
    let ring_t = ring.clone();
    let gain_t = master_gain.clone();
    let under_t = underruns.clone();
    let stop_t = stop.clone();
    let ready_t = ready.clone();

    let thread = std::thread::spawn(move || {
        let signal_ready = |ok: bool| {
            let (lock, cv) = &*ready_t;
            *lock.lock().unwrap() = Some(ok);
            cv.notify_all();
        };

        let ring_cb = ring_t.clone();
        let gain_cb = gain_t.clone();
        let under_cb = under_t.clone();
        let err_fn = |e| eprintln!("[audio] stream error: {e}");
        let stream = device.build_output_stream::<f32, _, _>(
            stream_config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let g = *gain_cb.lock().unwrap();
                let mut r = ring_cb.lock().unwrap();
                for s in out.iter_mut() {
                    match r.pop_front() {
                        Some(v) => *s = v * g,
                        None => {
                            under_cb.fetch_add(1, Ordering::Relaxed);
                            *s = 0.0;
                        }
                    }
                }
            },
            err_fn,
            None,
        );

        let stream = match stream {
            Ok(s) => s,
            Err(_) => {
                signal_ready(false);
                return;
            }
        };
        if stream.play().is_err() {
            signal_ready(false);
            return;
        }
        signal_ready(true);

        // Mantém o Stream vivo até close(). Polling leve — o áudio roda no
        // callback do cpal, esta thread só segura o Stream e observa `stop`.
        while !stop_t.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Drop do `stream` aqui para o device de forma limpa.
        drop(stream);
    });

    // Espera a thread confirmar que o stream subiu (ou falhou).
    let ok = {
        let (lock, cv) = &*ready;
        let mut guard = lock.lock().unwrap();
        while guard.is_none() {
            guard = cv.wait(guard).unwrap();
        }
        guard.unwrap()
    };
    if !ok {
        stop.store(true, Ordering::Release);
        let _ = thread.join();
        return 0;
    }

    let handle = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let out = OutputHandle {
        ring,
        sample_rate: real_sr,
        channels: real_ch,
        capacity_frames: real_cap,
        master_gain,
        underruns,
        stop,
        thread: Some(thread),
        ready,
    };
    streams().lock().unwrap().insert(handle, out);
    handle
}

/// Executa `f` com o handle do stream, se existir.
pub fn with_stream<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&OutputHandle) -> R,
{
    let map = streams().lock().unwrap();
    match map.get(&handle) {
        Some(s) => f(s),
        None => default,
    }
}

/// Fecha o stream: sinaliza a thread, dá join, libera. No-op se inválido.
pub fn close(handle: u64) {
    let removed = streams().lock().unwrap().remove(&handle);
    if let Some(mut out) = removed {
        out.stop.store(true, Ordering::Release);
        if let Some(t) = out.thread.take() {
            let _ = t.join();
        }
        // `ready` e demais Arcs caem aqui.
        let _ = &out.ready;
    }
}
