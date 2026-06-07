// Player de música WAV — lê um arquivo .wav real do disco, decodifica o header,
// resampleia para o rate do device e reproduz. Todo o codec é TypeScript; o
// Rust só fornece device I/O (audio) e leitura de arquivo (fs).
//
// Arquivo: examples/music_test.wav (Chopin — Scherzo nº 2, domínio público,
// Internet Archive / OnClassical). PCM 16-bit estéreo 44100 Hz.
//
// Rodar:    target/release/rts.exe run examples/play_music.ts
// Compilar: target/release/rts.exe compile -p examples/play_music.ts out

import { audio, fs, buffer, math, time, io } from "rts";

const PATH = "examples/music_test.wav";

// ───────────────────── 1) LER O ARQUIVO ─────────────────────
const fsize = fs.size(PATH);
if (fsize <= 44) {
  io.print("arquivo não encontrado ou inválido: " + PATH);
} else {
  io.print("arquivo: " + PATH + " (" + fsize + " bytes)");
  const fbuf = buffer.alloc(fsize);
  const got = fs.read_all(PATH, buffer.ptr(fbuf), fsize);
  io.print("lidos: " + got + " bytes");

  // ───────────────────── 2) DECODIFICAR HEADER ─────────────────────
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

  const fmt = rdU16(fbuf, 20); // 1 = PCM
  const wavCh = rdU16(fbuf, 22);
  const wavSr = rdU32(fbuf, 24);
  const wavBits = rdU16(fbuf, 34);
  const wavData = rdU32(fbuf, 40);
  io.print(
    "WAV: fmt=" + fmt + ", " + wavSr + " Hz, " + wavCh + " ch, " + wavBits +
    " bits, data=" + wavData + " bytes"
  );

  if (fmt !== 1 || wavBits !== 16) {
    io.print("este player só suporta PCM 16-bit; arquivo incompatível.");
  } else {
    // ───────────────────── 3) REPRODUZIR ─────────────────────
    const stream = audio.open_output(0, wavCh, 0); // rate nativo do device
    if (stream === 0) {
      io.print("falha ao abrir o device de áudio");
    } else {
      const devSr = audio.sample_rate(stream);
      const devCh = audio.channels(stream);
      io.print("device: " + devSr + " Hz, " + devCh + " ch — tocando...");
      audio.master_volume(stream, 1.0);

      const wavFrames = wavData / (wavCh * 2);
      const ratio = wavSr / devSr;
      const outFrames = math.floor(wavFrames / ratio);
      const durSec = math.floor(outFrames / devSr);
      io.print("duração: ~" + durSec + "s (resample " + wavSr + "→" + devSr + " Hz)");

      const blockFrames = 4096;
      const sbuf = buffer.alloc(blockFrames * devCh * 4);

      let out = 0;
      let lastReport = 0;
      while (out < outFrames) {
        const free = audio.available_frames(stream);
        if (free <= 0) {
          time.sleep_ms(3);
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
            let ra = rdU16(fbuf, 44 + i0 * wavCh * 2 + srcCh * 2);
            if (ra >= 32768) ra -= 65536;
            const a = ra / 32768.0;
            let rb = rdU16(fbuf, 44 + i1 * wavCh * 2 + srcCh * 2);
            if (rb >= 32768) rb -= 65536;
            const b = rb / 32768.0;
            buffer.write_f32(sbuf, so * 4, a + (b - a) * frac);
            so++;
          }
        }
        const accepted = audio.write(stream, sbuf, n * devCh);
        out += accepted / devCh;

        // Relatório de progresso a cada ~2s tocados.
        const sec = math.floor(out / devSr);
        if (sec > lastReport) {
          lastReport = sec;
          io.print("  ..." + sec + "s / " + durSec + "s");
        }
      }

      let guard = 0;
      while (audio.queued_frames(stream) > 0 && guard < 5000) {
        time.sleep_ms(3);
        guard++;
      }

      io.print("underruns: " + audio.underruns(stream));
      audio.close(stream);
      buffer.free(sbuf);
      io.print("fim da música.");
    }
  }
  buffer.free(fbuf);
}
