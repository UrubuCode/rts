// Mini-engine de síntese polifônica — 100% TypeScript sobre o primitivo
// `rts:audio`. Demonstra a Fase 2: o dev pensa em "notas" e "ondas", não em
// buffers crus. O Rust só fornece o device (open_output/write); toda a síntese,
// mixagem e envelope são TS.
//
// Polifonia via ARRAYS PARALELOS de float (freq/phase/amp/wave/active) — o RTS
// não suporta array de instâncias de classe com dispatch de método, mas suporta
// arrays de float (fix de codegen desta fase), que é tudo que a engine precisa.
//
// Toca um arpejo de Dó maior (C-E-G-C) seguido do acorde cheio, ~4s.
//
// Rodar:    target/release/rts.exe run examples/synth.ts
// ASIO:     trocar import para "asio_audio as audio"

import { audio, buffer, math, time, io } from "rts";

// ─────────────────────────── formas de onda ───────────────────────────
// wave: 0=sine, 1=square, 2=saw, 3=triangle. phase em [0,1).
function waveSample(wave: number, phase: number): number {
  if (wave === 0) {
    return math.sin(phase * 2.0 * math.PI);
  } else if (wave === 1) {
    return phase < 0.5 ? 1.0 : -1.0;
  } else if (wave === 2) {
    return 2.0 * phase - 1.0; // saw -1..1
  } else {
    // triangle
    return phase < 0.5 ? (4.0 * phase - 1.0) : (3.0 - 4.0 * phase);
  }
}

// ─────────────────────────── estado das vozes ───────────────────────────
const MAX_VOICES = 8;
const vFreq = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const vPhase = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const vAmp = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const vWave = [0, 0, 0, 0, 0, 0, 0, 0];
const vActive = [0, 0, 0, 0, 0, 0, 0, 0];
const vEnv = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];   // nível atual do envelope
const vTarget = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // alvo (1=on, 0=off)

// noteOn: ativa a primeira voz livre. Retorna o slot, ou -1 se cheio.
function noteOn(freq: number, amp: number, wave: number): number {
  for (let i = 0; i < MAX_VOICES; i++) {
    if (vActive[i] === 0) {
      vFreq[i] = freq;
      vPhase[i] = 0.0;
      vAmp[i] = amp;
      vWave[i] = wave;
      vActive[i] = 1;
      vEnv[i] = 0.0;
      vTarget[i] = 1.0;
      return i;
    }
  }
  return -1;
}

// noteOff: inicia o fade-out de um slot.
function noteOff(slot: number): void {
  if (slot >= 0 && slot < MAX_VOICES) {
    vTarget[slot] = 0.0;
  }
}

// ─────────────────────────── render ───────────────────────────
// Preenche `bufHandle` com `frames` frames estéreo (interleaved f32).
// Avança o estado das vozes. Coef de envelope: aproxima o alvo suavemente
// (one-pole) pra evitar cliques no on/off.
const ENV_RATE = 0.0008; // velocidade do fade (por frame)

function render(bufHandle: number, frames: number, sr: number): void {
  let s = 0;
  for (let f = 0; f < frames; f++) {
    let mix = 0.0;
    for (let i = 0; i < MAX_VOICES; i++) {
      if (vActive[i] === 1) {
        // envelope one-pole rumo ao alvo
        vEnv[i] = vEnv[i] + (vTarget[i] - vEnv[i]) * ENV_RATE * 60.0;
        const sample = waveSample(vWave[i], vPhase[i]) * vAmp[i] * vEnv[i];
        mix = mix + sample;
        vPhase[i] = vPhase[i] + vFreq[i] / sr;
        if (vPhase[i] >= 1.0) vPhase[i] = vPhase[i] - 1.0;
        // desativa quando o fade-out completou
        if (vTarget[i] === 0.0 && vEnv[i] < 0.001) {
          vActive[i] = 0;
          vEnv[i] = 0.0;
        }
      }
    }
    // soft clip leve
    if (mix > 1.0) mix = 1.0;
    if (mix < -1.0) mix = -1.0;
    // estéreo: mesmo valor nos 2 canais
    buffer.write_f32(bufHandle, s * 4, mix); s = s + 1;
    buffer.write_f32(bufHandle, s * 4, mix); s = s + 1;
  }
}

// Toca `ms` milissegundos, mantendo o ring cheio (modelo pull).
function playFor(stream: number, ms: number, sr: number, ch: number, buf: number, blockFrames: number): void {
  const totalFrames = (sr * ms) / 1000;
  let done = 0;
  while (done < totalFrames) {
    const free = audio.available_frames(stream);
    if (free <= 0) { time.sleep_ms(2); continue; }
    let n = free;
    if (n > blockFrames) n = blockFrames;
    if (n > totalFrames - done) n = totalFrames - done;
    render(buf, n, sr);
    audio.write(stream, buf, n * ch);
    done = done + n;
  }
}

// ─────────────────────────── música ───────────────────────────
const stream = audio.open_output(0, 2, 0);
if (stream === 0) {
  io.print("falha ao abrir o device de áudio");
} else {
  const sr = audio.sample_rate(stream);
  const ch = audio.channels(stream);
  io.print("synth: " + sr + " Hz, " + ch + " ch — tocando arpejo + acorde de Dó maior");
  audio.master_volume(stream, 0.7);

  const blockFrames = 1024;
  const buf = buffer.alloc(blockFrames * ch * 4);

  // Arpejo: C E G C (saw pra timbre rico)
  const notes = [261.63, 329.63, 392.00, 523.25];
  for (let n = 0; n < 4; n++) {
    const v = noteOn(notes[n], 0.25, 2); // saw
    playFor(stream, 250, sr, ch, buf, blockFrames);
    noteOff(v);
  }

  // Acorde cheio C-E-G (sine) sustentado
  noteOn(261.63, 0.22, 0);
  noteOn(329.63, 0.22, 0);
  noteOn(392.00, 0.22, 0);
  playFor(stream, 1500, sr, ch, buf, blockFrames);
  // libera todas
  for (let i = 0; i < MAX_VOICES; i++) noteOff(i);
  playFor(stream, 400, sr, ch, buf, blockFrames); // deixa o fade terminar

  while (audio.queued_frames(stream) > 0) { time.sleep_ms(5); }
  io.print("underruns: " + audio.underruns(stream));
  audio.close(stream);
  buffer.free(buf);
  io.print("fim — synth polifônico 100% TypeScript.");
}
