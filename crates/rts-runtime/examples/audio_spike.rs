//! Spike de validação do cpal 0.18 — confirma que o backend default abre o
//! device de saída, reporta config, e que conseguimos tocar áudio fornecido por
//! um RING BUFFER (modelo "TS enche buffer, thread RT drena"), não síntese no
//! callback. Toca 1s de seno 440Hz empurrado de fora da thread de áudio.
//!
//! Rodar: cargo run -p rts-runtime --example audio_spike

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, StreamConfig};

fn main() {
    let host = cpal::default_host();
    println!("[spike] host: {:?}", host.id());

    let device = host
        .default_output_device()
        .expect("nenhum device de saída default");
    let dev_name = device
        .description()
        .map(|d| format!("{d:?}"))
        .unwrap_or_else(|_| "<sem descrição>".into());
    println!("[spike] device: {dev_name}");

    let default_cfg = device.default_output_config().expect("sem config default");
    let sample_rate: u32 = default_cfg.sample_rate();
    let channels = default_cfg.channels() as usize;
    println!(
        "[spike] default config: {} Hz, {} ch, fmt {:?}",
        sample_rate,
        channels,
        default_cfg.sample_format()
    );

    if default_cfg.sample_format() != SampleFormat::F32 {
        eprintln!(
            "[spike] formato {:?} não tratado neste spike (só F32). Saindo.",
            default_cfg.sample_format()
        );
        return;
    }

    // Ring buffer compartilhado: o "TS" (aqui, a main thread) escreve samples
    // intercalados; a thread de áudio (callback cpal) só drena. Sem síntese no
    // callback — é o modelo que queremos para o namespace.
    let ring: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let underruns = Arc::new(AtomicU64::new(0));
    let err_fn = |e| eprintln!("[spike] stream error: {e}");

    // Builder do callback (clona os Arcs) — reusado nos dois attempts.
    let make_cb = |ring: Arc<Mutex<VecDeque<f32>>>, underruns: Arc<AtomicU64>| {
        move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut r = ring.lock().unwrap();
            for s in out.iter_mut() {
                *s = r.pop_front().unwrap_or_else(|| {
                    underruns.fetch_add(1, Ordering::Relaxed);
                    0.0
                });
            }
        }
    };

    let base_cfg = |buf: BufferSize| StreamConfig {
        channels: default_cfg.channels(),
        sample_rate,
        buffer_size: buf,
    };

    // Tenta buffer pequeno (baixa latência). No WASAPI shared pode falhar/ser
    // ignorado — caímos em Default e reportamos.
    let stream = device
        .build_output_stream::<f32, _, _>(
            base_cfg(BufferSize::Fixed(256)),
            make_cb(ring.clone(), underruns.clone()),
            err_fn,
            None,
        )
        .inspect(|_| println!("[spike] buffer Fixed(256) aceito"))
        .or_else(|e| {
            eprintln!("[spike] Fixed(256) falhou ({e}); usando BufferSize::Default");
            device.build_output_stream::<f32, _, _>(
                base_cfg(BufferSize::Default),
                make_cb(ring.clone(), underruns.clone()),
                err_fn,
                None,
            )
        })
        .expect("não foi possível abrir o output stream");

    // Gerador de seno (estado de fase contínuo entre blocos → sem clique).
    let freq = 440.0_f32;
    let mut phase = 0.0_f32;
    let phase_inc = freq / sample_rate as f32;
    let mut gen_block = |frames: usize| -> Vec<f32> {
        let mut tmp = Vec::with_capacity(frames * channels);
        for _ in 0..frames {
            let v = (phase * 2.0 * PI).sin() * 0.2;
            phase += phase_inc;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            for _ in 0..channels {
                tmp.push(v);
            }
        }
        tmp
    };

    // Pré-enche o ring (~120ms) ANTES de dar play — evita o underrun inicial
    // enquanto o produtor ainda não engatou. É o que o TS faria: priming.
    let prime_frames = sample_rate as usize / 8; // 125ms
    ring.lock().unwrap().extend(gen_block(prime_frames));

    stream.play().expect("falha ao iniciar stream");
    println!("[spike] stream tocando — seno 440Hz contínuo (3s) com backpressure...");

    // Produtor com BACKPRESSURE real. TODA a contabilidade é em FRAMES para
    // evitar o bug clássico de misturar frames com samples (samples = frames *
    // channels). Mantém o ring acima de um hi-water; só gera quando há espaço —
    // espelha o que o TS fará consultando audio.available(handle).
    let target_frames = sample_rate as usize * 3; // 3 segundos de tom
    let hi_water_frames = sample_rate as usize / 4; // mantém ~250ms enfileirado
    let block_frames = 256usize;
    let mut produced_frames = prime_frames; // já geramos o priming acima

    while produced_frames < target_frames {
        // len() está em samples → converte p/ frames dividindo por channels.
        let queued_frames = ring.lock().unwrap().len() / channels;
        if queued_frames < hi_water_frames {
            let n = block_frames.min(target_frames - produced_frames);
            let b = gen_block(n);
            ring.lock().unwrap().extend(b);
            produced_frames += n;
        } else {
            // ring cheio o bastante; espera o callback drenar (~5ms ≈ 240 frames
            // @48k, bem abaixo da margem de 250ms → sem risco de underrun).
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    let u_after_produce = underruns.load(Ordering::Relaxed);
    let queued_after = ring.lock().unwrap().len() / channels;
    println!(
        "[spike] após produzir {target_frames} frames: underruns={u_after_produce}, ainda no ring={queued_after} frames"
    );

    // Drena o resto e PARA o stream assim que esvazia — sem cauda de underrun.
    // (No namespace real, close()/pause() faz exatamente isto.)
    loop {
        let queued = ring.lock().unwrap().len();
        if queued == 0 {
            break;
        }
        // dorme proporcional ao que falta drenar, sem girar o callback vazio.
        let ms = ((queued / channels) as u64 * 1000 / sample_rate as u64).max(1);
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    stream.pause().ok(); // para o callback imediatamente → zero cauda

    let u = underruns.load(Ordering::Relaxed);
    println!("[spike] concluído. underruns (samples sem dado): {u}");
    if u == 0 {
        println!("[spike] ✅ ZERO underruns — tom contínuo limpo de 3s.");
    } else {
        println!("[spike] ⚠️ {u} underruns (esperado ~0 com backpressure).");
    }
}
