//! Diagnóstico: lista os devices de saída que o cpal vê, e toca um tom curto no
//! device cujo nome contém "Kraken" (case-insensitive). Isola se o backend
//! CONSEGUE forçar a saída no Razer Kraken — base p/ o `open_output_named` no ABI.
//!
//! cpal 0.18: nome via `device.description()?.name()`; `sample_rate()` é `u32`.
//!
//! Rodar: cargo test --release -p rts-std --test audio_kraken -- --nocapture

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

fn dev_name<D: DeviceTrait>(d: &D) -> String {
    d.description()
        .ok()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|| "<sem nome>".into())
}

#[test]
fn list_and_play_kraken() {
    let host = cpal::default_host();

    eprintln!("=== default output device ===");
    if let Some(d) = host.default_output_device() {
        eprintln!("  DEFAULT: {}", dev_name(&d));
    }

    eprintln!("=== todos os output devices que o cpal vê ===");
    let mut target = None;
    if let Ok(devices) = host.output_devices() {
        for d in devices {
            let name = dev_name(&d);
            eprintln!("  - {name}");
            if name.to_lowercase().contains("kraken") {
                eprintln!("    >>> casa 'kraken' — alvo");
                target = Some(d);
            }
        }
    }

    let Some(device) = target else {
        eprintln!("Nenhum device com 'kraken' no nome — pulando (não é falha de código)");
        return;
    };

    let cfg = device
        .default_output_config()
        .expect("kraken default config");
    eprintln!(
        "alvo: {}  {}Hz {}ch {:?}",
        dev_name(&device),
        cfg.sample_rate(),
        cfg.channels(),
        cfg.sample_format()
    );
    if cfg.sample_format() != SampleFormat::F32 {
        eprintln!("formato não-F32; este smoke só faz F32 — pulando");
        return;
    }

    let sr = cfg.sample_rate() as f32;
    let ch = cfg.channels() as usize;
    let mut phase = 0.0f32;
    let freq = 440.0f32;

    let stream = device
        .build_output_stream::<f32, _, _>(
            cfg.config(),
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in out.chunks_mut(ch) {
                    let s = (phase * std::f32::consts::TAU).sin() * 0.3;
                    phase += freq / sr;
                    if phase >= 1.0 {
                        phase -= 1.0;
                    }
                    for x in frame.iter_mut() {
                        *x = s;
                    }
                }
            },
            |e| eprintln!("[audio] stream error: {e}"),
            None,
        )
        .expect("build kraken stream");

    stream.play().expect("play");
    eprintln!("tocando 440Hz no Kraken por 3s...");
    std::thread::sleep(std::time::Duration::from_secs(3));
    eprintln!("fim.");
}
