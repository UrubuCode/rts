// Engine de síntese expandida (Fase 2+) — 100% TypeScript sobre rts:audio.
// Adiciona sobre synth.ts: ADSR completo, filtro lowpass por voz, delay/eco
// global e um mini-sequenciador que toca "Frère Jacques".
//
// Rodar: target/release/rts.exe run examples/synth2.ts

import { audio, buffer, math, time, io } from "rts";

// ─────────────────────────── ondas ───────────────────────────
function waveSample(wave: number, phase: number): number {
  if (wave === 0) return math.sin(phase * 2.0 * math.PI);
  if (wave === 1) return phase < 0.5 ? 1.0 : -1.0;
  if (wave === 2) return 2.0 * phase - 1.0;
  return phase < 0.5 ? (4.0 * phase - 1.0) : (3.0 - 4.0 * phase);
}

// ─────────────────────────── vozes + ADSR ───────────────────────────
// ADSR stages: 0=idle, 1=attack, 2=decay, 3=sustain, 4=release
const MAX = 8;
const vFreq = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const vPhase = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const vAmp = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const vWave = [0, 0, 0, 0, 0, 0, 0, 0];
const vStage = [0, 0, 0, 0, 0, 0, 0, 0];
const vEnv = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
const vLpf = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // estado do filtro lowpass por voz

// Parâmetros ADSR globais (por-frame increments, ajustados ao sr no início)
let aRate = 0.0;   // attack: subida por frame
let dRate = 0.0;   // decay: descida por frame até sustain
let sLevel = 0.6;  // sustain: nível mantido
let rRate = 0.0;   // release: descida por frame até 0
let lpfCoef = 0.25; // coeficiente do filtro lowpass (0..1, menor=mais grave)

function noteOn(freq: number, amp: number, wave: number): number {
  for (let i = 0; i < MAX; i++) {
    if (vStage[i] === 0) {
      vFreq[i] = freq; vPhase[i] = 0.0; vAmp[i] = amp; vWave[i] = wave;
      vStage[i] = 1; vEnv[i] = 0.0; vLpf[i] = 0.0;
      return i;
    }
  }
  return -1;
}
function noteOff(slot: number): void {
  if (slot >= 0 && slot < MAX && vStage[slot] !== 0) vStage[slot] = 4; // release
}

// ─────────────────────────── delay/eco global ───────────────────────────
const DELAY_LEN = 12000; // ~250ms @48k
const delayBuf = buffer.alloc(DELAY_LEN * 4); // f32 ring de eco
let delayPos = 0;
const delayFeedback = 0.35;
const delayMix = 0.3;

// ─────────────────────────── render ───────────────────────────
function render(bufHandle: number, frames: number, sr: number, ch: number): void {
  let s = 0;
  for (let f = 0; f < frames; f++) {
    let mix = 0.0;
    for (let i = 0; i < MAX; i++) {
      if (vStage[i] !== 0) {
        // avança envelope ADSR
        if (vStage[i] === 1) {            // attack
          vEnv[i] = vEnv[i] + aRate;
          if (vEnv[i] >= 1.0) { vEnv[i] = 1.0; vStage[i] = 2; }
        } else if (vStage[i] === 2) {     // decay
          vEnv[i] = vEnv[i] - dRate;
          if (vEnv[i] <= sLevel) { vEnv[i] = sLevel; vStage[i] = 3; }
        } else if (vStage[i] === 4) {     // release
          vEnv[i] = vEnv[i] - rRate;
          if (vEnv[i] <= 0.0) { vEnv[i] = 0.0; vStage[i] = 0; }
        }
        // oscilador
        let raw = waveSample(vWave[i], vPhase[i]);
        // filtro lowpass one-pole por voz
        vLpf[i] = vLpf[i] + (raw - vLpf[i]) * lpfCoef;
        mix = mix + vLpf[i] * vAmp[i] * vEnv[i];
        // avança fase
        vPhase[i] = vPhase[i] + vFreq[i] / sr;
        if (vPhase[i] >= 1.0) vPhase[i] = vPhase[i] - 1.0;
      }
    }

    // delay/eco: lê amostra atrasada, soma feedback, escreve de volta
    const echoed = buffer.read_f32(delayBuf, delayPos * 4);
    const out = mix + echoed * delayMix;
    buffer.write_f32(delayBuf, delayPos * 4, mix + echoed * delayFeedback);
    delayPos = delayPos + 1;
    if (delayPos >= DELAY_LEN) delayPos = 0;

    let v = out;
    if (v > 1.0) v = 1.0;
    if (v < -1.0) v = -1.0;
    buffer.write_f32(bufHandle, s * 4, v); s = s + 1;
    buffer.write_f32(bufHandle, s * 4, v); s = s + 1;
  }
}

function playFor(stream: number, ms: number, sr: number, ch: number, buf: number, block: number): void {
  const total = (sr * ms) / 1000;
  let done = 0;
  while (done < total) {
    const free = audio.available_frames(stream);
    if (free <= 0) { time.sleep_ms(2); continue; }
    let n = free;
    if (n > block) n = block;
    if (n > total - done) n = total - done;
    render(buf, n, sr, ch);
    audio.write(stream, buf, n * ch);
    done = done + n;
  }
}

// ─────────────────────────── sequenciador ───────────────────────────
// notas MIDI -> Hz. A4=440 (midi 69). freq = 440 * 2^((m-69)/12)
function midiToFreq(m: number): number {
  return 440.0 * math.pow(2.0, (m - 69.0) / 12.0);
}

// ─────────────────────────── música ───────────────────────────
const stream = audio.open_output(0, 2, 0);
if (stream === 0) {
  io.print("falha ao abrir o device");
} else {
  const sr = audio.sample_rate(stream);
  const ch = audio.channels(stream);
  // ajusta taxas ADSR ao sample rate (tempos em segundos -> por-frame)
  aRate = 1.0 / (0.01 * sr);   // attack 10ms
  dRate = (1.0 - sLevel) / (0.08 * sr); // decay 80ms
  rRate = sLevel / (0.15 * sr);  // release 150ms

  io.print("synth2: " + sr + " Hz, " + ch + " ch — Frère Jacques (saw + ADSR + lpf + delay)");
  audio.master_volume(stream, 0.6);

  const block = 1024;
  const buf = buffer.alloc(block * ch * 4);

  // Frère Jacques: C D E C | C D E C | E F G | E F G  (MIDI)
  const melody = [60, 62, 64, 60, 60, 62, 64, 60, 64, 65, 67, 64, 65, 67];
  const durs =   [300, 300, 300, 300, 300, 300, 300, 300, 300, 300, 600, 300, 300, 600];

  for (let n = 0; n < 14; n++) {
    const v = noteOn(midiToFreq(melody[n]), 0.5, 2); // saw
    playFor(stream, durs[n] - 40, sr, ch, buf, block);
    noteOff(v);
    playFor(stream, 40, sr, ch, buf, block); // gap curto entre notas (release)
  }

  // deixa o delay/release decair
  playFor(stream, 800, sr, ch, buf, block);

  while (audio.queued_frames(stream) > 0) { time.sleep_ms(5); }
  io.print("underruns: " + audio.underruns(stream));
  audio.close(stream);
  buffer.free(buf);
  buffer.free(delayBuf);
  io.print("fim — Frère Jacques tocada pela engine TS.");
}
