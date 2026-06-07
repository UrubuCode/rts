//! Estado global do namespace `asio_audio` — streams de saída via ASIO.
//!
//! Espelha `audio::state`, mas usa o host ASIO (`HostId::Asio`) em vez do
//! default do SO. ASIO dá acesso de baixa latência e exclusivo ao hardware no
//! Windows — exige o ASIO SDK da Steinberg no build (feature `asio`) e um driver
//! ASIO instalado (ASIO4ALL ou o driver da placa).
//!
//! Como em `audio`, o `cpal::Stream` (`!Send`) vive numa thread dedicada; só o
//! ring buffer (`Arc<Mutex<VecDeque<f32>>>`) e contadores atômicos cruzam o
//! boundary extern "C". Modelo pull: o TS controla o loop.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, HostId, SampleFormat, StreamConfig};

pub type Ring = Arc<Mutex<VecDeque<f32>>>;

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

/// Resolve o host ASIO. None se a feature/driver não estiverem disponíveis.
fn asio_host() -> Option<cpal::Host> {
    cpal::host_from_id(HostId::Asio).ok()
}

/// 1 se o host ASIO está disponível (driver instalado), 0 caso contrário.
pub fn is_available() -> bool {
    asio_host().is_some()
}

/// Config do device de saída ASIO default (sample_rate, channels). 0,0 se nada.
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

/// Abre um stream de saída ASIO. Retorna o handle (>0) ou 0 em falha.
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
