//! `audio` namespace — low-level audio device I/O via cpal.
//!
//! Raw primitive only ("virtual sound card"): open/close an output device,
//! push interleaved f32 samples into a ring buffer, drained on the real-time
//! audio thread. All high-level audio (oscillators, ADSR, mixer, effects,
//! decode) is TypeScript on top of this. Pull model: TS owns the loop; Rust
//! never calls back into TS, so no GC ever runs on the audio thread.
//!
//! The `cpal::Stream` is `!Send`, so it stays pinned on a dedicated audio thread
//! and only `Send + Sync` state (ring + atomics) lives in the handle map; the
//! extern boundary sees only an opaque u64.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rts_engine::abi::ty::{Bool, F64, Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, with_entry};

/// Interleaved f32 ring buffer. Producer: TS (`write`); consumer: RT callback.
pub type Ring = Arc<Mutex<VecDeque<f32>>>;

/// An active output stream's `Send + Sync` state (the `!Send` cpal::Stream lives
/// on the audio thread).
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

/// Default output device config (sample_rate, channels). (0, 0) if unavailable.
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

/// Opens an output stream (always at the device's NATIVE config — forcing a
/// non-native rate breaks the real-time callback on WASAPI shared mode).
/// Returns the handle (>0) or 0 on failure. `capacity_frames` 0 → ~500ms.
pub fn open_output(_sample_rate: u32, _channels: u16, capacity_frames: usize) -> u64 {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        return 0;
    };
    let Ok(default_cfg) = device.default_output_config() else {
        return 0;
    };
    if default_cfg.sample_format() != SampleFormat::F32 {
        return 0;
    }

    let real_sr = default_cfg.sample_rate();
    let real_ch = default_cfg.channels();
    let real_cap = if capacity_frames == 0 {
        (real_sr as usize / 2).max(1)
    } else {
        capacity_frames
    };

    let ring: Ring = Arc::new(Mutex::new(VecDeque::with_capacity(
        real_cap * real_ch as usize,
    )));
    let master_gain = Arc::new(Mutex::new(1.0_f32));
    let underruns = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new((Mutex::new(None::<bool>), std::sync::Condvar::new()));

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

        // Set by the cpal error callback when the backend reports the stream is
        // gone — the classic WASAPI `DeviceNotAvailable` / "Default audio device
        // changed" that happens when the OS default output switches mid-playback
        // (headset reconnect, app re-routing). The supervisor loop below sees this
        // and REBUILDS the stream on the CURRENT default device, instead of leaving
        // a dead stream that drains nothing (silent playback — the bug this fixes).
        let lost = Arc::new(AtomicBool::new(false));

        // Build (and `play`) a fresh output stream on the current default device.
        // Returns `None` if no device / non-F32 / build/play failed. The ring,
        // gain and underrun counters are SHARED across rebuilds, so playback
        // resumes from where the producer left off (no glitch beyond the gap).
        let build_stream = {
            let ring_cb_src = ring_t.clone();
            let gain_cb_src = gain_t.clone();
            let under_cb_src = under_t.clone();
            let lost_src = lost.clone();
            move || -> Option<cpal::Stream> {
                let host = cpal::default_host();
                let device = host.default_output_device()?;
                let cfg = device.default_output_config().ok()?;
                if cfg.sample_format() != SampleFormat::F32 {
                    return None;
                }
                let ring_cb = ring_cb_src.clone();
                let gain_cb = gain_cb_src.clone();
                let under_cb = under_cb_src.clone();
                let lost_cb = lost_src.clone();
                let stream = device
                    .build_output_stream::<f32, _, _>(
                        cfg.config(),
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
                        move |e| {
                            eprintln!("[audio] stream error: {e}");
                            lost_cb.store(true, Ordering::Release);
                        },
                        None,
                    )
                    .ok()?;
                stream.play().ok()?;
                Some(stream)
            }
        };

        let mut stream = match build_stream() {
            Some(s) => s,
            None => {
                signal_ready(false);
                return;
            }
        };
        signal_ready(true);

        // Supervisor loop: hold the stream alive; on a reported loss, rebuild on
        // the current default device. A rebuild that fails (device truly gone) is
        // retried each tick, so playback recovers as soon as a device returns.
        // Hold the live stream in an Option so a rebuild can drop the old one by
        // assignment without a borrow-checker move across loop iterations.
        let mut stream = Some(stream);
        while !stop_t.load(Ordering::Acquire) {
            if lost.swap(false, Ordering::AcqRel) {
                stream = None; // drop the dead stream first
                match build_stream() {
                    Some(s) => stream = Some(s),
                    None => {
                        // No device right now; keep the producer's ring intact and
                        // retry shortly. Put the loss flag back so we re-check.
                        lost.store(true, Ordering::Release);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    }
                }
            }
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

// ---------------------------------------------------------------------------
// extern "C" ABI symbols (hand-written, sem macro)
// ---------------------------------------------------------------------------

/// Default output device sample rate (Hz), or 0 if unavailable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_DEFAULT_SAMPLE_RATE() -> I64 {
    default_output_config().0 as i64
}

/// Default output device channel count, or 0 if unavailable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_DEFAULT_CHANNELS() -> I64 {
    default_output_config().1 as i64
}

/// Opens an output stream. 0/0 args use the device default; capacity 0 ≈ 500ms.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_OPEN_OUTPUT(sample_rate: I64, channels: I64, capacity_frames: I64) -> Handle {
    let sr = sample_rate.max(0) as u32;
    let ch = channels.clamp(0, u16::MAX as i64) as u16;
    let cap = capacity_frames.max(0) as usize;
    open_output(sr, ch, cap)
}

/// Effective stream sample rate (may differ from requested). 0 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_SAMPLE_RATE(handle: U64) -> I64 {
    with_stream(handle, 0, |s| s.sample_rate as i64)
}

/// Effective stream channel count. 0 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_CHANNELS(handle: U64) -> I64 {
    with_stream(handle, 0, |s| s.channels as i64)
}

/// Is the stream alive? 1 = yes, 0 = no.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_IS_OPEN(handle: U64) -> Bool {
    with_stream(handle, 0, |_| 1)
}

/// Free frames in the ring (room for `write` without backpressure). -1 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_AVAILABLE_FRAMES(handle: U64) -> I64 {
    with_stream(handle, -1, |s| {
        let queued_samples = s.ring.lock().unwrap().len();
        let queued_frames = queued_samples / s.channels.max(1) as usize;
        (s.capacity_frames as i64 - queued_frames as i64).max(0)
    })
}

/// Frames currently queued in the ring. -1 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_QUEUED_FRAMES(handle: U64) -> I64 {
    with_stream(handle, -1, |s| {
        let queued_samples = s.ring.lock().unwrap().len();
        (queued_samples / s.channels.max(1) as usize) as i64
    })
}

/// Pushes f32 samples (interleaved) from `buffer_handle` into the ring.
/// Returns the SAMPLES accepted (may be < n_samples under backpressure).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_WRITE(handle: U64, buffer_handle: U64, n_samples: I64) -> I64 {
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

/// Sets the master gain (multiplied per sample on the RT callback). Clamped [0, 4].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_MASTER_VOLUME(handle: U64, gain: F64) {
    let g = (gain as f32).clamp(0.0, 4.0);
    with_stream(handle, (), |s| {
        *s.master_gain.lock().unwrap() = g;
    });
}

/// Total underruns since open (device asked for data with an empty ring). -1 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_UNDERRUNS(handle: U64) -> I64 {
    with_stream(handle, -1, |s| s.underruns.load(Ordering::Relaxed) as i64)
}

/// Closes the stream and frees the device. Repeated calls are no-ops.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_AUDIO_CLOSE(handle: U64) {
    close(handle);
}

// ---------------------------------------------------------------------------
// Builder registration (Fase 2 — hand-written, sem macro)
// ---------------------------------------------------------------------------

/// Função `audio.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `audio` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("audio")
        .doc("Low-level audio device I/O (cpal). Samples flow as f32 LE bytes in a Buffer.")
        .member(func(
            "default_sample_rate",
            "__RTS_FN_NS_AUDIO_DEFAULT_SAMPLE_RATE",
            Sig::new(Vec::new(), AbiType::I64),
            "default_sample_rate(): number",
            "Default output device sample rate (Hz), or 0 if unavailable.",
            __RTS_FN_NS_AUDIO_DEFAULT_SAMPLE_RATE as *const u8,
        ))
        .member(func(
            "default_channels",
            "__RTS_FN_NS_AUDIO_DEFAULT_CHANNELS",
            Sig::new(Vec::new(), AbiType::I64),
            "default_channels(): number",
            "Default output device channel count, or 0 if unavailable.",
            __RTS_FN_NS_AUDIO_DEFAULT_CHANNELS as *const u8,
        ))
        .member(func(
            "open_output",
            "__RTS_FN_NS_AUDIO_OPEN_OUTPUT",
            Sig::new(vec![AbiType::I64, AbiType::I64, AbiType::I64], AbiType::Handle),
            "open_output(sample_rate: number, channels: number, capacity_frames: number): number",
            "Opens an output stream. 0/0 args use the device default; capacity 0 ≈ 500ms.",
            __RTS_FN_NS_AUDIO_OPEN_OUTPUT as *const u8,
        ))
        .member(func(
            "sample_rate",
            "__RTS_FN_NS_AUDIO_SAMPLE_RATE",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "sample_rate(handle: number): number",
            "Effective stream sample rate (may differ from requested). 0 if invalid.",
            __RTS_FN_NS_AUDIO_SAMPLE_RATE as *const u8,
        ))
        .member(func(
            "channels",
            "__RTS_FN_NS_AUDIO_CHANNELS",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "channels(handle: number): number",
            "Effective stream channel count. 0 if invalid.",
            __RTS_FN_NS_AUDIO_CHANNELS as *const u8,
        ))
        .member(func(
            "is_open",
            "__RTS_FN_NS_AUDIO_IS_OPEN",
            Sig::new(vec![AbiType::U64], AbiType::Bool),
            "is_open(handle: number): boolean",
            "Is the stream alive? 1 = yes, 0 = no.",
            __RTS_FN_NS_AUDIO_IS_OPEN as *const u8,
        ))
        .member(func(
            "available_frames",
            "__RTS_FN_NS_AUDIO_AVAILABLE_FRAMES",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "available_frames(handle: number): number",
            "Free frames in the ring (room for `write` without backpressure). -1 if invalid.",
            __RTS_FN_NS_AUDIO_AVAILABLE_FRAMES as *const u8,
        ))
        .member(func(
            "queued_frames",
            "__RTS_FN_NS_AUDIO_QUEUED_FRAMES",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "queued_frames(handle: number): number",
            "Frames currently queued in the ring. -1 if invalid.",
            __RTS_FN_NS_AUDIO_QUEUED_FRAMES as *const u8,
        ))
        .member(func(
            "write",
            "__RTS_FN_NS_AUDIO_WRITE",
            Sig::new(vec![AbiType::U64, AbiType::U64, AbiType::I64], AbiType::I64),
            "write(handle: number, buffer_handle: number, n_samples: number): number",
            "Pushes f32 samples (interleaved) from `buffer_handle` into the ring.\nReturns the SAMPLES accepted (may be < n_samples under backpressure).",
            __RTS_FN_NS_AUDIO_WRITE as *const u8,
        ))
        .member(func(
            "master_volume",
            "__RTS_FN_NS_AUDIO_MASTER_VOLUME",
            Sig::new(vec![AbiType::U64, AbiType::F64], AbiType::Void),
            "master_volume(handle: number, gain: number): void",
            "Sets the master gain (multiplied per sample on the RT callback). Clamped [0, 4].",
            __RTS_FN_NS_AUDIO_MASTER_VOLUME as *const u8,
        ))
        .member(func(
            "underruns",
            "__RTS_FN_NS_AUDIO_UNDERRUNS",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "underruns(handle: number): number",
            "Total underruns since open (device asked for data with an empty ring). -1 if invalid.",
            __RTS_FN_NS_AUDIO_UNDERRUNS as *const u8,
        ))
        .member(func(
            "close",
            "__RTS_FN_NS_AUDIO_CLOSE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "close(handle: number): void",
            "Closes the stream and frees the device. Repeated calls are no-ops.",
            __RTS_FN_NS_AUDIO_CLOSE as *const u8,
        ))
        .done();
}
