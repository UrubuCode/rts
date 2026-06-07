// Exemplo: GERA um arquivo WAV, depois LÊ, DECODIFICA e REPRODUZ.
//
// Mostra o ciclo completo de "reproduzir um arquivo de áudio" usando só o
// primitivo cru `audio` + `fs` + `buffer`. Todo o codec (montar e parsear o
// WAV) é TypeScript — o Rust só fornece device I/O e bytes de arquivo.
//
// WAV usado: PCM 16-bit, estéreo. Tocamos um acorde de Dó maior (C-E-G) por 2s.
//
// Rodar (JIT):   target/release/rts.exe run examples/audio_play_wav.ts
// Compilar:      target/release/rts.exe compile -p examples/audio_play_wav.ts out

import { audio, fs, buffer, math, time, io } from "rts";

const PATH = "examples/_chord.wav";
const SR = 44100;
const CH = 2;
const DUR_SEC = 2;
const N_FRAMES = SR * DUR_SEC;

// ───────────────────────── 1) GERAR O WAV ─────────────────────────
// Layout WAV PCM 16-bit:
//   "RIFF" <chunkSize:u32> "WAVE"
//   "fmt " <16:u32> <1:u16 pcm> <ch:u16> <sr:u32> <byteRate:u32> <blockAlign:u16> <16:u16 bits>
//   "data" <dataSize:u32> <samples i16 LE interleaved...>

function writeU32(buf: number, off: number, v: number): void {
  buffer.write_u8(buf, off, v & 0xff);
  buffer.write_u8(buf, off + 1, (v >> 8) & 0xff);
  buffer.write_u8(buf, off + 2, (v >> 16) & 0xff);
  buffer.write_u8(buf, off + 3, (v >> 24) & 0xff);
}
function writeU16(buf: number, off: number, v: number): void {
  buffer.write_u8(buf, off, v & 0xff);
  buffer.write_u8(buf, off + 1, (v >> 8) & 0xff);
}
function writeChar4(buf: number, off: number, a: number, b: number, c: number, d: number): void {
  buffer.write_u8(buf, off, a);
  buffer.write_u8(buf, off + 1, b);
  buffer.write_u8(buf, off + 2, c);
  buffer.write_u8(buf, off + 3, d);
}

const bytesPerSample = 2;
const blockAlign = CH * bytesPerSample;
const dataSize = N_FRAMES * blockAlign;
const fileSize = 44 + dataSize; // 44 = header padrão
const wav = buffer.alloc(fileSize);

// RIFF header
writeChar4(wav, 0, 82, 73, 70, 70);      // "RIFF"
writeU32(wav, 4, 36 + dataSize);
writeChar4(wav, 8, 87, 65, 86, 69);      // "WAVE"
// fmt chunk
writeChar4(wav, 12, 102, 109, 116, 32);  // "fmt "
writeU32(wav, 16, 16);
writeU16(wav, 20, 1);                     // PCM
writeU16(wav, 22, CH);
writeU32(wav, 24, SR);
writeU32(wav, 28, SR * blockAlign);       // byteRate
writeU16(wav, 32, blockAlign);
writeU16(wav, 34, 16);                     // bits per sample
// data chunk
writeChar4(wav, 36, 100, 97, 116, 97);   // "data"
writeU32(wav, 40, dataSize);

// Samples: acorde C maior. Frequências C4, E4, G4.
const f1 = 261.63;
const f2 = 329.63;
const f3 = 392.00;
let ph1 = 0.0;
let ph2 = 0.0;
let ph3 = 0.0;
const amp = 0.25;

let off = 44;
for (let i = 0; i < N_FRAMES; i++) {
  const v =
    (math.sin(ph1 * 2.0 * math.PI) +
      math.sin(ph2 * 2.0 * math.PI) +
      math.sin(ph3 * 2.0 * math.PI)) *
    amp /
    3.0;
  ph1 += f1 / SR; if (ph1 >= 1.0) ph1 -= 1.0;
  ph2 += f2 / SR; if (ph2 >= 1.0) ph2 -= 1.0;
  ph3 += f3 / SR; if (ph3 >= 1.0) ph3 -= 1.0;

  // f32 [-1,1] → i16
  let s = v;
  if (s > 1.0) s = 1.0;
  if (s < -1.0) s = -1.0;
  const i16 = math.floor(s * 32767.0);
  const u16 = i16 < 0 ? i16 + 65536 : i16; // two's complement LE
  for (let c = 0; c < CH; c++) {
    writeU16(wav, off, u16);
    off += 2;
  }
}

const wrote = fs.write_bytes(PATH, buffer.ptr(wav), fileSize);
io.print("WAV escrito: " + PATH + " (" + wrote + " bytes)");
buffer.free(wav);

// ───────────────────────── 2) LER + DECODIFICAR ─────────────────────────
const fsize = fs.size(PATH);
io.print("tamanho do arquivo: " + fsize + " bytes");
const fbuf = buffer.alloc(fsize);
const got = fs.read_all(PATH, buffer.ptr(fbuf), fsize);
io.print("lidos: " + got + " bytes");

// Parse mínimo do header (assumindo layout canônico de 44 bytes).
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

const wavCh = rdU16(fbuf, 22);
const wavSr = rdU32(fbuf, 24);
const wavBits = rdU16(fbuf, 34);
const wavData = rdU32(fbuf, 40);
io.print("decodificado: " + wavSr + " Hz, " + wavCh + " ch, " + wavBits + " bits, data=" + wavData + " bytes");

// ───────────────────────── 3) REPRODUZIR ─────────────────────────
// Abre no rate NATIVO do device (0 = default). No WASAPI shared o rate é fixo
// pelo mixer; forçar outro valor falharia. O TS então resampleia o WAV do seu
// rate para o rate efetivo do device — é o player que se adapta ao hardware.
const stream = audio.open_output(0, wavCh, 0);
if (stream === 0) {
  io.print("falha ao abrir o device");
} else {
  const devSr = audio.sample_rate(stream);
  const devCh = audio.channels(stream);
  io.print("device aberto: " + devSr + " Hz, " + devCh + " ch (WAV: " + wavSr + " Hz)");
  audio.master_volume(stream, 0.9);

  // Resample linear: para cada frame de saída, posição no WAV = outFrame * ratio.
  // (Leitura de sample inline — o RTS ainda não captura variáveis livres em
  // funções aninhadas, issue #195, então evitamos closures no loop quente.)
  const wavFrames = wavData / (wavCh * 2);
  const ratio = wavSr / devSr;
  const outFrames = math.floor(wavFrames / ratio);
  const blockFrames = 2048;
  const sbuf = buffer.alloc(blockFrames * devCh * 4); // f32 de saída

  io.print("resample " + wavSr + "→" + devSr + " Hz, " + outFrames + " frames de saída");

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
      // clampa índices ao range válido do WAV
      if (i0 >= wavFrames) i0 = wavFrames - 1;
      if (i1 >= wavFrames) i1 = wavFrames - 1;
      for (let c = 0; c < devCh; c++) {
        const srcCh = c < wavCh ? c : wavCh - 1;
        // sample a (frame i0, canal srcCh)
        let ra = rdU16(fbuf, 44 + i0 * wavCh * 2 + srcCh * 2);
        if (ra >= 32768) ra -= 65536;
        const a = ra / 32768.0;
        // sample b (frame i1, canal srcCh)
        let rb = rdU16(fbuf, 44 + i1 * wavCh * 2 + srcCh * 2);
        if (rb >= 32768) rb -= 65536;
        const b = rb / 32768.0;
        const v = a + (b - a) * frac; // interpolação linear
        buffer.write_f32(sbuf, so * 4, v);
        so++;
      }
    }
    const acceptedSamples = audio.write(stream, sbuf, n * devCh);
    const acceptedFrames = acceptedSamples / devCh;
    out += acceptedFrames;
  }

  // Espera drenar (com teto de segurança pra nunca travar).
  let guard = 0;
  while (audio.queued_frames(stream) > 0 && guard < 2000) {
    time.sleep_ms(2);
    guard++;
  }

  io.print("underruns durante a reprodução: " + audio.underruns(stream));
  audio.close(stream);
  buffer.free(sbuf);
  io.print("fim — você deve ter ouvido um acorde de Dó maior por ~2s.");
}
buffer.free(fbuf);
