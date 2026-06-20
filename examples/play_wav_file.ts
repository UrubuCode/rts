// Player de um arquivo WAV ARBITRÁRIO (PCM 16-bit). Diferente de
// audio_play_wav.ts (que gera seu próprio WAV de 44 bytes), este PARSEIA os
// chunks RIFF de verdade — acha `fmt ` e `data` por varredura, então funciona
// com WAVs reais (ex: convertidos de MP3 via ffmpeg, que inserem um chunk LIST).
//
// Rodar: target/release/rts.exe run examples/play_wav_file.ts

import { audio, fs, buffer, math, time, io } from "rts";

const PATH = "examples/_cancao.wav";

// ───────── ler arquivo ─────────
const fsize = fs.size(PATH);
io.print("arquivo: " + PATH + " (" + fsize + " bytes)");
const fbuf = buffer.alloc(fsize);
fs.read_all(PATH, buffer.ptr(fbuf), fsize);

function rdU32(buf: number, o: number): number {
  return (
    buffer.read_u8(buf, o) |
    (buffer.read_u8(buf, o + 1) << 8) |
    (buffer.read_u8(buf, o + 2) << 16) |
    (buffer.read_u8(buf, o + 3) << 24)
  );
}
function rdU16(buf: number, o: number): number {
  return buffer.read_u8(buf, o) | (buffer.read_u8(buf, o + 1) << 8);
}
function is4(buf: number, o: number, a: number, b: number, c: number, d: number): boolean {
  return (
    buffer.read_u8(buf, o) === a &&
    buffer.read_u8(buf, o + 1) === b &&
    buffer.read_u8(buf, o + 2) === c &&
    buffer.read_u8(buf, o + 3) === d
  );
}

// RIFF....WAVE — confirma e varre os chunks a partir do offset 12.
let wavCh = 2;
let wavSr = 44100;
let wavBits = 16;
let dataOff = -1;
let dataSize = 0;

let o = 12;
while (o + 8 <= fsize) {
  const id0 = buffer.read_u8(fbuf, o);
  const sz = rdU32(fbuf, o + 4);
  if (is4(fbuf, o, 102, 109, 116, 32)) {
    // "fmt "
    wavCh = rdU16(fbuf, o + 10);
    wavSr = rdU32(fbuf, o + 12);
    wavBits = rdU16(fbuf, o + 22);
  } else if (is4(fbuf, o, 100, 97, 116, 97)) {
    // "data"
    dataOff = o + 8;
    dataSize = sz;
  }
  // avança: 8 (header) + sz, com padding de paridade
  let adv = 8 + sz;
  if (adv % 2 === 1) adv += 1;
  o += adv;
  if (dataOff >= 0) o = fsize; // já achamos data, para
  if (id0 < 0) o = fsize;
}

io.print("WAV: " + wavSr + " Hz, " + wavCh + " ch, " + wavBits + " bits, data=" + dataSize + " bytes @ " + dataOff);

if (dataOff < 0 || wavBits !== 16) {
  io.print("formato não suportado (precisa PCM 16-bit com chunk data)");
} else {
  // ───────── reproduzir com resample p/ o rate do device ─────────
  const stream = audio.open_output(0, wavCh, 0);
  if (stream === 0) {
    io.print("falha ao abrir o device");
  } else {
    const devSr = audio.sample_rate(stream);
    const devCh = audio.channels(stream);
    audio.master_volume(stream, 0.9);
    io.print("device: " + devSr + " Hz, " + devCh + " ch — tocando...");

    const wavFrames = dataSize / (wavCh * 2);
    const ratio = wavSr / devSr;
    const outFrames = math.floor(wavFrames / ratio);
    const blockFrames = 2048;
    const sbuf = buffer.alloc(blockFrames * devCh * 4);

    let out = 0;
    while (out < outFrames) {
      const free = audio.available_frames(stream);
      if (free <= 0) {
        time.sleep_ms(2);
        continue;
      }
      let n = free;
      if (n > blockFrames) n = blockFrames;
      if (n > outFrames - out) n = outFrames - out;

      let so = 0;
      for (let i = 0; i < n; i++) {
        const srcPos = (out + i) * ratio;
        let i0 = math.floor(srcPos);
        const frac = srcPos - i0;
        let i1 = i0 + 1;
        if (i0 >= wavFrames) i0 = wavFrames - 1;
        if (i1 >= wavFrames) i1 = wavFrames - 1;
        for (let c = 0; c < devCh; c++) {
          const srcCh = c < wavCh ? c : wavCh - 1;
          let ra = rdU16(fbuf, dataOff + i0 * wavCh * 2 + srcCh * 2);
          if (ra >= 32768) ra -= 65536;
          const a = ra / 32768.0;
          let rb = rdU16(fbuf, dataOff + i1 * wavCh * 2 + srcCh * 2);
          if (rb >= 32768) rb -= 65536;
          const b = rb / 32768.0;
          const v = a + (b - a) * frac;
          buffer.write_f32(sbuf, so * 4, v);
          so++;
        }
      }
      const accepted = audio.write(stream, sbuf, n * devCh);
      out += accepted / devCh;
    }

    let guard = 0;
    while (audio.queued_frames(stream) > 0 && guard < 5000) {
      time.sleep_ms(2);
      guard++;
    }
    io.print("underruns: " + audio.underruns(stream));
    audio.close(stream);
    buffer.free(sbuf);
    io.print("fim.");
  }
}
buffer.free(fbuf);
