// Exemplo do namespace `audio` — o primitivo cru.
//
// Toca 2 segundos de um tom (seno 440Hz) SEM usar nenhuma engine de alto
// nível: o TS gera os samples, escreve f32 num buffer e empurra para o stream
// via audio.write(), no modelo PULL (o TS controla o loop e respeita o
// backpressure com available_frames). É exatamente o que a engine builtin/audio
// fará por baixo, mas aqui mostramos a base nua.
//
// Rodar (JIT):  target/release/rts.exe run examples/audio_tone.ts

import { audio, buffer, time, math, io } from "rts";

// 1) Abre o device default (sample_rate=0, channels=0 → usa o do device).
//    capacity_frames=0 → ring de ~500ms.
const sr = audio.default_sample_rate();
const ch = audio.default_channels();
io.print("device default: " + sr + " Hz, " + ch + " canais");

const stream = audio.open_output(0, 0, 0);
if (stream === 0) {
  io.print("falha ao abrir o device de áudio");
} else {
  const rate = audio.sample_rate(stream);
  const channels = audio.channels(stream);
  io.print("stream aberto: handle=" + stream + ", " + rate + " Hz, " + channels + " ch");

  audio.master_volume(stream, 0.3); // volume confortável

  // 2) Buffer reutilizável para um bloco de samples (frames * channels).
  //    Aloca o suficiente para o maior bloco que vamos escrever de uma vez.
  const blockFrames = 1024;
  const blockSamples = blockFrames * channels;
  const buf = buffer.alloc(blockSamples * 4); // 4 bytes por f32

  const freq = 440.0;
  let phase = 0.0;
  const phaseInc = freq / rate;

  // 3) Loop de produção (pull): por ~2s, mantém o ring cheio.
  const totalFrames = rate * 2; // 2 segundos
  let produced = 0;

  while (produced < totalFrames) {
    const free = audio.available_frames(stream);
    if (free <= 0) {
      time.sleep_ms(2); // ring cheio: espera o device drenar
      continue;
    }

    // Gera no máximo `free` frames (limitado ao tamanho do buffer e ao que
    // falta para completar os 2s).
    let n = free;
    if (n > blockFrames) n = blockFrames;
    if (n > totalFrames - produced) n = totalFrames - produced;

    // Preenche o buffer com o seno (interleaved: mesmo valor em cada canal).
    let s = 0;
    for (let i = 0; i < n; i++) {
      const v = math.sin(phase * 2.0 * math.PI);
      phase += phaseInc;
      if (phase >= 1.0) phase -= 1.0;
      for (let c = 0; c < channels; c++) {
        buffer.write_f32(buf, s * 4, v);
        s++;
      }
    }

    // Empurra os n*channels samples para o stream.
    audio.write(stream, buf, n * channels);
    produced += n;
  }

  // 4) Espera o ring drenar antes de fechar (senão corta o fim do som).
  //    Fecha assim que esvazia — sem sleep extra que faria o callback girar
  //    vazio e contar "underruns" de cauda inofensivos.
  while (audio.queued_frames(stream) > 0) {
    time.sleep_ms(5);
  }

  io.print("underruns durante a reprodução: " + audio.underruns(stream));
  audio.close(stream);
  io.print("fim — você deve ter ouvido um tom de ~2s.");
}
