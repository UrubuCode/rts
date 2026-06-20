// "Ode à Alegria" (Beethoven) em LOOP infinito — síntese TS inline.
//
// Igual a music.ts, mas o sequenciador é envolto num `while (true)`: ao chegar
// na última nota, reseta seqIdx=0 e recomeça, emendando as voltas (sem o tail
// de 500ms entre elas, p/ soar contínuo). Ctrl+C para parar.
//
// Tudo permanece INLINE no top-level (o motor tem um gap com funções aninhadas
// que fazem I/O de áudio + estado global — ver comentário em music.ts).
//
// Rodar: target/release/rts.exe run examples/music_loop.ts

import { audio, buffer, math, time, io } from "rts";

const stream = audio.open_output(0, 2, 0);
if (stream === 0) {
  io.print("falha ao abrir o device");
} else {
  const sr = audio.sample_rate(stream);
  const ch = audio.channels(stream);
  audio.master_volume(stream, 0.7);
  io.print("♪ Ode à Alegria — LOOP — " + sr + "Hz " + ch + "ch (Ctrl+C p/ parar)");

  const MAX = 8;
  const vFreq = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
  const vPhase = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
  const vAmp = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
  const vWave = [0, 0, 0, 0, 0, 0, 0, 0];
  const vStage = [0, 0, 0, 0, 0, 0, 0, 0]; // 0 idle,1 atk,2 dec,3 sus,4 rel
  const vEnv = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

  const aRate = 1.0 / (0.008 * sr);
  const sLevel = 0.7;
  const dRate = (1.0 - sLevel) / (0.05 * sr);
  const rRate = sLevel / (0.12 * sr);

  const seqMel = [64, 64, 65, 67, 67, 65, 64, 62, 60, 60, 62, 64, 64, 62, 62,
                  64, 64, 65, 67, 67, 65, 64, 62, 60, 60, 62, 64, 62, 60, 60];
  const seqBass = [48, 0, 0, 52, 48, 0, 0, 50, 48, 0, 0, 52, 48, 0, 50,
                   48, 0, 0, 52, 48, 0, 0, 50, 48, 0, 0, 52, 48, 0, 48];
  const seqDur = [380, 380, 380, 380, 380, 380, 380, 380, 380, 380, 380, 380, 570, 190, 760,
                  380, 380, 380, 380, 380, 380, 380, 380, 380, 380, 380, 380, 570, 190, 760];
  const NOTES = 30;

  const buf = buffer.alloc(1024 * ch * 4);

  // ── LOOP INFINITO em volta da sequência ──
  while (true) {
    let seqIdx = 0;
    while (seqIdx < NOTES) {
      const melHz = 440.0 * math.pow(2.0, (seqMel[seqIdx] - 69.0) / 12.0);
      let mslot = -1;
      for (let i = 0; i < MAX; i++) { if (mslot < 0 && vStage[i] === 0) mslot = i; }
      if (mslot >= 0) {
        vFreq[mslot] = melHz; vPhase[mslot] = 0.0; vAmp[mslot] = 0.5;
        vWave[mslot] = 0; vStage[mslot] = 1; vEnv[mslot] = 0.0;
      }
      let bslot = -1;
      if (seqBass[seqIdx] > 0) {
        const bHz = 440.0 * math.pow(2.0, (seqBass[seqIdx] - 69.0) / 12.0);
        for (let i = 0; i < MAX; i++) { if (bslot < 0 && vStage[i] === 0) bslot = i; }
        if (bslot >= 0) {
          vFreq[bslot] = bHz; vPhase[bslot] = 0.0; vAmp[bslot] = 0.35;
          vWave[bslot] = 2; vStage[bslot] = 1; vEnv[bslot] = 0.0;
        }
      }

      const noteFrames = (sr * seqDur[seqIdx]) / 1000;
      const releaseAt = noteFrames - (sr * 60) / 1000;
      let played = 0;
      while (played < noteFrames) {
        const free = audio.available_frames(stream);
        if (free <= 0) { time.sleep_ms(2); continue; }
        let n = free;
        if (n > 1024) n = 1024;
        if (n > noteFrames - played) n = noteFrames - played;

        if (played < releaseAt && played + n >= releaseAt) {
          if (mslot >= 0 && vStage[mslot] !== 0) vStage[mslot] = 4;
          if (bslot >= 0 && vStage[bslot] !== 0) vStage[bslot] = 4;
        }

        let s = 0;
        for (let f = 0; f < n; f++) {
          let mix = 0.0;
          for (let i = 0; i < MAX; i++) {
            if (vStage[i] !== 0) {
              if (vStage[i] === 1) { vEnv[i] = vEnv[i] + aRate; if (vEnv[i] >= 1.0) { vEnv[i] = 1.0; vStage[i] = 2; } }
              else if (vStage[i] === 2) { vEnv[i] = vEnv[i] - dRate; if (vEnv[i] <= sLevel) { vEnv[i] = sLevel; vStage[i] = 3; } }
              else if (vStage[i] === 4) { vEnv[i] = vEnv[i] - rRate; if (vEnv[i] <= 0.0) { vEnv[i] = 0.0; vStage[i] = 0; } }
              let osc = 0.0;
              if (vWave[i] === 0) osc = math.sin(vPhase[i] * 2.0 * math.PI);
              else osc = 2.0 * vPhase[i] - 1.0;
              mix = mix + osc * vAmp[i] * vEnv[i];
              vPhase[i] = vPhase[i] + vFreq[i] / sr;
              if (vPhase[i] >= 1.0) vPhase[i] = vPhase[i] - 1.0;
            }
          }
          if (mix > 1.0) mix = 1.0;
          if (mix < -1.0) mix = -1.0;
          buffer.write_f32(buf, s * 4, mix); s = s + 1;
          buffer.write_f32(buf, s * 4, mix); s = s + 1;
        }
        audio.write(stream, buf, n * ch);
        played = played + n;
      }
      seqIdx = seqIdx + 1;
    }
    // sem tail entre voltas — emenda direto na próxima repetição
  }
}
