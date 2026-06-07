//! Funções extern "C" do namespace `asio_audio` — backend ASIO (baixa latência).
//!
//! API idêntica a `audio` (mesmo modelo pull + ring buffer + samples f32 via
//! buffer), mas roteada pelo host ASIO. Disponível só quando compilado com a
//! feature `asio` e com um driver ASIO instalado. Ver `docs/specs/audio.md`.

use super::super::gc::handles::{with_entry, Entry};
use super::state;

/// 1 se o host ASIO está disponível (driver instalado e feature ativa), 0 senão.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_IS_AVAILABLE() -> i64 {
    if state::is_available() {
        1
    } else {
        0
    }
}

/// Sample rate do device ASIO default (Hz), ou 0 se indisponível.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_DEFAULT_SAMPLE_RATE() -> i64 {
    state::default_output_config().0 as i64
}

/// Canais do device ASIO default, ou 0 se indisponível.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_DEFAULT_CHANNELS() -> i64 {
    state::default_output_config().1 as i64
}

/// Abre um stream de saída ASIO. Args/contrato iguais a `audio.open_output`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_OPEN_OUTPUT(
    sample_rate: i64,
    channels: i64,
    capacity_frames: i64,
) -> u64 {
    let sr = sample_rate.max(0) as u32;
    let ch = channels.clamp(0, u16::MAX as i64) as u16;
    let cap = capacity_frames.max(0) as usize;
    state::open_output(sr, ch, cap)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_SAMPLE_RATE(handle: u64) -> i64 {
    state::with_stream(handle, 0, |s| s.sample_rate as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_CHANNELS(handle: u64) -> i64 {
    state::with_stream(handle, 0, |s| s.channels as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_IS_OPEN(handle: u64) -> i64 {
    state::with_stream(handle, 0, |_| 1)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_AVAILABLE_FRAMES(handle: u64) -> i64 {
    state::with_stream(handle, -1, |s| {
        let queued_samples = s.ring.lock().unwrap().len();
        let queued_frames = queued_samples / s.channels.max(1) as usize;
        (s.capacity_frames as i64 - queued_frames as i64).max(0)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_QUEUED_FRAMES(handle: u64) -> i64 {
    state::with_stream(handle, -1, |s| {
        let queued_samples = s.ring.lock().unwrap().len();
        (queued_samples / s.channels.max(1) as usize) as i64
    })
}

/// Empurra samples f32 (interleaved) de um buffer para o ring. Igual a
/// `audio.write`: buffer_handle deve conter n_samples*4 bytes f32 LE. Retorna
/// samples aceitos (backpressure).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_WRITE(
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

        let samples: Option<Vec<f32>> = with_entry(buffer_handle, |entry| match entry {
            Some(Entry::Buffer(bytes)) => {
                let avail = bytes.len() / 4;
                let take = n.min(avail);
                let mut v = Vec::with_capacity(take);
                for i in 0..take {
                    let off = i * 4;
                    let arr = [bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]];
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
        let mut accept = samples.len().min(free);
        accept -= accept % channels;
        ring.extend(samples.into_iter().take(accept));
        accept as i64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_MASTER_VOLUME(handle: u64, gain: f64) {
    let g = (gain as f32).clamp(0.0, 4.0);
    state::with_stream(handle, (), |s| {
        *s.master_gain.lock().unwrap() = g;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_UNDERRUNS(handle: u64) -> i64 {
    state::with_stream(handle, -1, |s| {
        s.underruns.load(std::sync::atomic::Ordering::Relaxed) as i64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ASIO_AUDIO_CLOSE(handle: u64) {
    state::close(handle);
}
