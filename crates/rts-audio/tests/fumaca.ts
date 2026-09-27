// Fumaça de rts:audio pelo motor real, com o dispositivo NULO:
//   target/release/rts.exe run crates/rts-audio/tests/fumaca.ts
import audio from "rts:audio";
import { time } from "rts";
import { readFileSync } from "node:fs";

const dev = audio.open_output(48000, 2, 1);
console.log("abriu " + dev + " taxa=" + audio.sample_rate(dev) + " canais=" + audio.channels(dev));
const bloco = new Float32Array(9600);
for (let i = 0; i < bloco.length; i++) bloco[i] = 0.25;
console.log("escritas " + audio.write(dev, bloco, 9600) + " enfileirados>4600 " + (audio.queued_frames(dev) > 4600 ? "sim" : "nao"));
time.sleep_ms(60);
const st = new Float64Array(6);
audio.stats(dev, st);
console.log("drenou " + (st[0] > 1500 ? "sim" : "nao") + " nulo=" + st[5] + " enfileirados<4800 " + (audio.queued_frames(dev) < 4800 ? "sim" : "nao"));

const dst = new Float32Array(8);
const src = new Float32Array([0.5, 0.5, 0.5, 0.5]);
const d = new Float64Array(16);
d[1] = 1; d[2] = 1; d[3] = 2; d[4] = 4; d[5] = 1; d[6] = 1; d[7] = 1; d[8] = 1; d[9] = 1; d[13] = -1;
console.log("mix " + audio.mix_add(dst, src, d) + " " + dst[0] + " " + dst[7] + " pos=" + d[0]);
console.log("alias " + audio.mix_add(dst, dst, d));
const nivel = new Float64Array(8);
nivel[0] = 2; nivel[1] = 4;
console.log("nivel " + audio.mix_level(dst, nivel) + " pico=" + nivel[2]);

const info = new Float64Array(4);
const h = audio.decode_ogg(readFileSync("crates/rts-audio/tests/fixtures/seno440_mono_22050.ogg"), info);
const amostras = new Float32Array(info[2] * info[1]);
console.log("ogg taxa=" + info[0] + " canais=" + info[1] + " copiadas=" + (audio.ogg_take(h, amostras) === amostras.length ? "todas" : "faltou"));
console.log("lixo " + audio.decode_ogg(new Uint8Array([1, 2, 3]), info) + " erro=" + info[3]);
console.log("handle ruim " + audio.queued_frames(99));
audio.close(dev);
console.log("fechou");
