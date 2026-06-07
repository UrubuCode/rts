//! Funções extern "C" do namespace `audio` — o primitivo cru.
//!
//! Filosofia (ver `.claude/rules/04-workflow.md`): o Rust expõe só primitivos.
//! É uma "placa de som virtual" — abre o device, recebe samples num ring
//! buffer e os drena na thread real-time. NADA de síntese, mixer ou efeito
//! aqui: oscilador, ADSR, mixagem e decode de formatos são responsabilidade
//! do TS (engine `builtin/audio/`, fase 2), que controla 100% o áudio.
//!
//! ## Como os samples chegam do TS
//!
//! Não há `AbiType::F32` nem typed-array nativo. Samples f32 trafegam como
//! BYTES dentro de um `Entry::Buffer` (Vec<u8>): o TS escreve os f32 num
//! buffer (via `Float32Array` view sobre o buffer) e passa o HANDLE do buffer
//! a `write` — nunca um ponteiro cru (ptr arith em extern corrompe; o GC pode
//! coletar o handle — ver memórias do projeto). O Rust lê os bytes, reinterpreta
//! como f32 little-endian e empurra no ring.

use super::super::gc::handles::{with_entry, Entry};
use super::state;

/// Sample rate do device de saída default (Hz), ou 0 se indisponível.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_DEFAULT_SAMPLE_RATE() -> i64 {
    state::default_output_config().0 as i64
}

/// Número de canais do device de saída default, ou 0 se indisponível.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_DEFAULT_CHANNELS() -> i64 {
    state::default_output_config().1 as i64
}

/// Abre um stream de saída e começa a tocar (silêncio até o primeiro `write`).
///
/// `sample_rate`/`channels` em 0 usam o default do device. `capacity_frames`
/// em 0 usa ~500ms. Retorna o handle do stream (>0) ou 0 em falha.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_OPEN_OUTPUT(
    sample_rate: i64,
    channels: i64,
    capacity_frames: i64,
) -> u64 {
    let sr = sample_rate.max(0) as u32;
    let ch = channels.clamp(0, u16::MAX as i64) as u16;
    let cap = capacity_frames.max(0) as usize;
    state::open_output(sr, ch, cap)
}

/// Sample rate efetivo do stream (pode diferir do pedido). 0 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_SAMPLE_RATE(handle: u64) -> i64 {
    state::with_stream(handle, 0, |s| s.sample_rate as i64)
}

/// Número de canais efetivo do stream. 0 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_CHANNELS(handle: u64) -> i64 {
    state::with_stream(handle, 0, |s| s.channels as i64)
}

/// Stream vivo? 1 = sim, 0 = não.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_IS_OPEN(handle: u64) -> i64 {
    state::with_stream(handle, 0, |_| 1)
}

/// Frames livres no ring buffer agora (espaço para `write` sem bloquear).
/// O TS usa isto para backpressure: gera no máximo `available_frames`. -1 se
/// o handle for inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_AVAILABLE_FRAMES(handle: u64) -> i64 {
    state::with_stream(handle, -1, |s| {
        let queued_samples = s.ring.lock().unwrap().len();
        let queued_frames = queued_samples / s.channels.max(1) as usize;
        (s.capacity_frames as i64 - queued_frames as i64).max(0)
    })
}

/// Frames atualmente enfileirados no ring (esperando reprodução). -1 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_QUEUED_FRAMES(handle: u64) -> i64 {
    state::with_stream(handle, -1, |s| {
        let queued_samples = s.ring.lock().unwrap().len();
        (queued_samples / s.channels.max(1) as usize) as i64
    })
}

/// Empurra samples f32 (interleaved) do buffer para o ring do stream.
///
/// - `handle`: stream de saída.
/// - `buffer_handle`: `Entry::Buffer` contendo `n_samples * 4` bytes f32 LE.
/// - `n_samples`: total de samples (frames * channels) a ler do buffer.
///
/// Respeita backpressure: nunca enfileira além de `capacity_frames`. Retorna
/// o número de SAMPLES efetivamente aceitos (pode ser < n_samples se o ring
/// estava quase cheio; 0 em handle inválido). O TS deve reenviar o resto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_WRITE(
    handle: u64,
    buffer_handle: u64,
    n_samples: i64,
) -> i64 {
    if n_samples <= 0 {
        return 0;
    }
    let n = n_samples as usize;

    state::with_stream(handle, 0, |s| {
        let channels = s.channels.max(1) as usize;
        let cap_samples = s.capacity_frames * channels;

        // Lê os f32 do buffer (cópia para soltar o lock do HandleTable antes de
        // travar o ring — evita aninhar locks de subsistemas diferentes).
        let samples: Option<Vec<f32>> = with_entry(buffer_handle, |entry| match entry {
            Some(Entry::Buffer(bytes)) => {
                let avail = bytes.len() / 4; // f32 = 4 bytes
                let take = n.min(avail);
                let mut v = Vec::with_capacity(take);
                for i in 0..take {
                    let off = i * 4;
                    let arr = [
                        bytes[off],
                        bytes[off + 1],
                        bytes[off + 2],
                        bytes[off + 3],
                    ];
                    v.push(f32::from_le_bytes(arr));
                }
                Some(v)
            }
            _ => None,
        });

        let Some(samples) = samples else { return 0 };
        if samples.is_empty() {
            return 0;
        }

        let mut ring = s.ring.lock().unwrap();
        let free = cap_samples.saturating_sub(ring.len());
        // Empurra apenas múltiplos completos de `channels` para não dessincronizar
        // os canais (um frame parcial deslocaria L/R para sempre).
        let mut accept = samples.len().min(free);
        accept -= accept % channels;
        ring.extend(samples.into_iter().take(accept));
        accept as i64
    })
}

/// Define o ganho master do stream (multiplicado em cada sample no callback).
/// `gain` chega como f64 e é truncado para f32. Clampeado em [0, 4].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_MASTER_VOLUME(handle: u64, gain: f64) {
    let g = (gain as f32).clamp(0.0, 4.0);
    state::with_stream(handle, (), |s| {
        *s.master_gain.lock().unwrap() = g;
    });
}

/// Total de underruns (samples pedidos pelo device sem dado no ring) desde a
/// abertura. Útil para diagnóstico de starvation. -1 se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_UNDERRUNS(handle: u64) -> i64 {
    state::with_stream(handle, -1, |s| {
        s.underruns.load(std::sync::atomic::Ordering::Relaxed) as i64
    })
}

/// Fecha o stream e libera o device. Chamadas repetidas são no-op.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_CLOSE(handle: u64) {
    state::close(handle);
}
