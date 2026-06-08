//! `asio_audio` namespace — low-latency audio output via ASIO (Windows).
//!
//! Identical API to `audio` (pull model, ring buffer, f32 samples via buffer),
//! but routed through the ASIO host instead of the OS default. Gives exclusive,
//! low-latency access to the hardware — relevant for live monitoring / virtual
//! instruments, not for plain playback (where WASAPI `audio` is already fine).
//!
//! Compiled ONLY with the `asio` feature (requires the Steinberg ASIO SDK +
//! LLVM/libclang at build time) and needs an installed ASIO driver (ASIO4ALL or
//! the sound card's own). See `docs/specs/audio.md`.
//!
//! Like `audio`, the `cpal::Stream` (`!Send`) stays pinned on a dedicated audio
//! thread; only `Send + Sync` state (ring + atomics) lives in the handle map and
//! the extern boundary sees only an opaque u64.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, HostId, SampleFormat, StreamConfig};
use rts_abi::ty::{Bool, F64, Handle, I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, with_entry};

/// Interleaved f32 ring buffer. Producer: TS (`write`); consumer: RT callback.
pub type Ring = Arc<Mutex<VecDeque<f32>>>;

/// An active ASIO output stream's `Send + Sync` state (the `!Send` cpal::Stream
/// lives on the audio thread).
pub struct OutputHandle {
    pub ring: Ring,
    pub sample_rate: u32,
    pub channels: u16,
    pub capacity_frames: usize,
    pub master_gain: Arc<Mutex<f32>>,
    pub underruns: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    ready: Arc<(Mutex<Option<bool>>, std::sync::Condvar)>,
}

static STREAMS: OnceLock<Mutex<HashMap<u64, OutputHandle>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn streams() -> &'static Mutex<HashMap<u64, OutputHandle>> {
    STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolve the ASIO host. None if the feature/driver are unavailable.
fn asio_host() -> Option<cpal::Host> {
    cpal::host_from_id(HostId::Asio).ok()
}

/// Whether the ASIO host is available (driver installed).
pub fn is_available() -> bool {
    asio_host().is_some()
}

/// Default ASIO output device config (sample_rate, channels). (0, 0) if none.
pub fn default_output_config() -> (u32, u16) {
    let Some(host) = asio_host() else {
        return (0, 0);
    };
    let Some(device) = host.default_output_device() else {
        return (0, 0);
    };
    match device.default_output_config() {
        Ok(cfg) => (cfg.sample_rate(), cfg.channels()),
        Err(_) => (0, 0),
    }
}

/// Opens an ASIO output stream. Returns the handle (>0) or 0 on failure.
pub fn open_output(sample_rate: u32, channels: u16, capacity_frames: usize) -> u64 {
    let Some(host) = asio_host() else {
        return 0;
    };
    let Some(device) = host.default_output_device() else {
        return 0;
    };
    let Ok(default_cfg) = device.default_output_config() else {
        return 0;
    };
    if default_cfg.sample_format() != SampleFormat::F32 {
        return 0;
    }

    let sr = if sample_rate == 0 {
        default_cfg.sample_rate()
    } else {
        sample_rate
    };
    let ch = if channels == 0 {
        default_cfg.channels()
    } else {
        channels
    };
    let cap_frames = if capacity_frames == 0 {
        (sr as usize / 2).max(1)
    } else {
        capacity_frames
    };

    let ring: Ring = Arc::new(Mutex::new(VecDeque::with_capacity(cap_frames * ch as usize)));
    let master_gain = Arc::new(Mutex::new(1.0_f32));
    let underruns = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new((Mutex::new(None::<bool>), std::sync::Condvar::new()));
    let effective_sr = Arc::new(AtomicU32::new(sr));
    let effective_ch = Arc::new(AtomicU16::new(ch));
    let default_sr = default_cfg.sample_rate();
    let default_ch = default_cfg.channels();

    let ring_t = ring.clone();
    let gain_t = master_gain.clone();
    let under_t = underruns.clone();
    let stop_t = stop.clone();
    let ready_t = ready.clone();
    let eff_sr_t = effective_sr.clone();
    let eff_ch_t = effective_ch.clone();

    let thread = std::thread::spawn(move || {
        let signal_ready = |ok: bool| {
            let (lock, cv) = &*ready_t;
            *lock.lock().unwrap() = Some(ok);
            cv.notify_all();
        };

        let build = |try_sr: u32, try_ch: u16| {
            let ring_cb = ring_t.clone();
            let gain_cb = gain_t.clone();
            let under_cb = under_t.clone();
            let err_fn = |e| eprintln!("[asio_audio] stream error: {e}");
            device.build_output_stream::<f32, _, _>(
                StreamConfig {
                    channels: try_ch,
                    sample_rate: try_sr,
                    buffer_size: BufferSize::Default,
                },
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
            )
        };

        let (stream, used_sr, used_ch) = match build(sr, ch) {
            Ok(s) => (s, sr, ch),
            Err(_) => match build(default_sr, default_ch) {
                Ok(s) => (s, default_sr, default_ch),
                Err(_) => {
                    signal_ready(false);
                    return;
                }
            },
        };
        if stream.play().is_err() {
            signal_ready(false);
            return;
        }
        eff_sr_t.store(used_sr, Ordering::Release);
        eff_ch_t.store(used_ch, Ordering::Release);
        signal_ready(true);

        while !stop_t.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(stream);
    });

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

    let real_sr = effective_sr.load(Ordering::Acquire);
    let real_ch = effective_ch.load(Ordering::Acquire);
    let real_cap = if capacity_frames == 0 {
        (real_sr as usize / 2).max(1)
    } else {
        cap_frames
    };

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

/// Runs `f` with the stream handle, if present.
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

/// Closes a stream: signals + joins the audio thread, frees state. No-op if invalid.
pub fn close(handle: u64) {
    let removed = streams().lock().unwrap().remove(&handle);
    if let Some(mut out) = removed {
        out.stop.store(true, Ordering::Release);
        if let Some(t) = out.thread.take() {
            let _ = t.join();
        }
        let _ = &out.ready;
    }
}

/// Low-latency audio output via ASIO (Windows). Same API as `audio` but routed
/// through the ASIO host. Requires the `asio` build feature and an installed
/// ASIO driver. Samples flow as f32 LE bytes in a Buffer.
#[rts_namespace(asio_audio)]
impl AsioAudioNs {
    /// Whether an ASIO host/driver is available (1) or not (0).
    #[rts_fn]
    pub fn is_available() -> Bool {
        if is_available() { 1 } else { 0 }
    }

    /// Sample rate (Hz) of the default ASIO output device, or 0 if unavailable.
    #[rts_fn]
    pub fn default_sample_rate() -> I64 {
        default_output_config().0 as i64
    }

    /// Channel count of the default ASIO output device, or 0 if unavailable.
    #[rts_fn]
    pub fn default_channels() -> I64 {
        default_output_config().1 as i64
    }

    /// Opens an ASIO output stream (low latency). Same contract as
    /// `audio.open_output`. Returns the stream handle (>0) or 0 on failure.
    #[rts_fn]
    pub fn open_output(sample_rate: I64, channels: I64, capacity_frames: I64) -> Handle {
        let sr = sample_rate.max(0) as u32;
        let ch = channels.clamp(0, u16::MAX as i64) as u16;
        let cap = capacity_frames.max(0) as usize;
        open_output(sr, ch, cap)
    }

    /// Effective sample rate (Hz) of the stream, or 0 if invalid.
    #[rts_fn]
    pub fn sample_rate(handle: U64) -> I64 {
        with_stream(handle, 0, |s| s.sample_rate as i64)
    }

    /// Effective channel count of the stream, or 0 if invalid.
    #[rts_fn]
    pub fn channels(handle: U64) -> I64 {
        with_stream(handle, 0, |s| s.channels as i64)
    }

    /// Whether the stream is still open (1) or not (0).
    #[rts_fn]
    pub fn is_open(handle: U64) -> Bool {
        with_stream(handle, 0, |_| 1)
    }

    /// Free frames in the ring buffer right now (backpressure). -1 if invalid.
    #[rts_fn]
    pub fn available_frames(handle: U64) -> I64 {
        with_stream(handle, -1, |s| {
            let queued_samples = s.ring.lock().unwrap().len();
            let queued_frames = queued_samples / s.channels.max(1) as usize;
            (s.capacity_frames as i64 - queued_frames as i64).max(0)
        })
    }

    /// Frames currently queued in the ring (awaiting playback). -1 if invalid.
    #[rts_fn]
    pub fn queued_frames(handle: U64) -> I64 {
        with_stream(handle, -1, |s| {
            let queued_samples = s.ring.lock().unwrap().len();
            (queued_samples / s.channels.max(1) as usize) as i64
        })
    }

    /// Pushes interleaved f32 samples from a buffer into the stream's ring.
    /// Same contract as `audio.write`. Returns samples accepted.
    #[rts_fn]
    pub fn write(handle: U64, buffer_handle: U64, n_samples: I64) -> I64 {
        if n_samples <= 0 {
            return 0;
        }
        let n = n_samples as usize;
        with_stream(handle, 0, |s| {
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

    /// Sets the stream master gain (multiplied per sample). Clamped to [0, 4].
    #[rts_fn]
    pub fn master_volume(handle: U64, gain: F64) {
        let g = (gain as f32).clamp(0.0, 4.0);
        with_stream(handle, (), |s| {
            *s.master_gain.lock().unwrap() = g;
        });
    }

    /// Total underruns since open. -1 if invalid.
    #[rts_fn]
    pub fn underruns(handle: U64) -> I64 {
        with_stream(handle, -1, |s| s.underruns.load(Ordering::Relaxed) as i64)
    }

    /// Closes the stream and releases the device. Repeated calls are a no-op.
    #[rts_fn]
    pub fn close(handle: U64) {
        close(handle);
    }
}
