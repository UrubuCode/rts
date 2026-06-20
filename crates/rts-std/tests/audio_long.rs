//! Tom LONGO (25s) em Rust puro no Kraken — o callback gera direto (sem ring, sem
//! RTS). Isola se a reprodução longa emudece por causa do DEVICE/Windows (então o
//! tom puro também cai) ou da arquitetura ring+thread do RTS (então o puro toca os
//! 25s inteiros). Imprime um marcador por segundo para casar com o que se ouve.
//!
//! Rodar: cargo test --release -p rts-std --test audio_long -- --nocapture

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

fn dev_name<D: DeviceTrait>(d: &D) -> String {
    d.description()
        .ok()
        .map(|x| x.name().to_string())
        .unwrap_or_else(|| "<sem nome>".into())
}

#[test]
fn long_tone_25s() {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        eprintln!("sem device default — pulando");
        return;
    };
    eprintln!("device: {}", dev_name(&device));
    let cfg = device.default_output_config().expect("cfg");
    if cfg.sample_format() != SampleFormat::F32 {
        eprintln!("não-F32 — pulando");
        return;
    }
    let sr = cfg.sample_rate() as f32;
    let ch = cfg.channels() as usize;

    // Contador de frames produzidos, lido pela main p/ marcar segundos.
    let frames = Arc::new(AtomicU64::new(0));
    let frames_cb = frames.clone();
    let mut phase = 0.0f32;
    let freq = 440.0f32;

    let stream = device
        .build_output_stream::<f32, _, _>(
            cfg.config(),
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in out.chunks_mut(ch) {
                    let s = (phase * std::f32::consts::TAU).sin() * 0.4;
                    phase += freq / sr;
                    if phase >= 1.0 {
                        phase -= 1.0;
                    }
                    for x in frame.iter_mut() {
                        *x = s;
                    }
                }
                frames_cb.fetch_add((out.len() / ch) as u64, Ordering::Relaxed);
            },
            |e| eprintln!("[audio] stream error: {e}"),
            None,
        )
        .expect("build");
    stream.play().expect("play");

    eprintln!("tocando 440Hz puro por 25s — avise se/quando sumir");
    let srate = sr as u64;
    for sec in 1..=25 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let f = frames.load(Ordering::Relaxed);
        eprintln!("  {sec}s — frames produzidos pelo callback: {f} (esperado ~{})", sec as u64 * srate);
    }
    eprintln!("fim.");
}
