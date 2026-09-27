// Fumaça de `audio.escutar` pelo motor real, com o dispositivo de saída REAL
// (não o nulo): toca um tom de 440 Hz pelas nativas existentes e escuta por
// loopback, imprimindo o array [rms, pico, silencio, quadros, taxa, canais].
//   target/release/rts.exe run crates/rts-audio/tests/escuta_fumaca.ts
import audio from "rts:audio";
import { time } from "rts";

const dev = audio.open_output(48000, 2, 0);
console.log("abriu " + dev + " taxa=" + audio.sample_rate(dev) + " canais=" + audio.channels(dev));

const taxa = 48000;
const canais = 2;
const duracaoMs = 500;
const quadros = Math.floor((taxa * duracaoMs) / 1000);
const bloco = new Float32Array(quadros * canais);
for (let i = 0; i < quadros; i++) {
  const v = Math.sin((2 * Math.PI * 440 * i) / taxa) * 0.5;
  bloco[i * canais] = v;
  bloco[i * canais + 1] = v;
}
console.log("escritas " + audio.write(dev, bloco, bloco.length));

// Dá tempo do dispositivo começar a tocar antes de escutar.
time.sleep_ms(50);

const out = new Float64Array(6);
const ok = audio.escutar(300, out);
console.log(
  "escutar ok=" + ok +
  " rms=" + out[0] +
  " pico=" + out[1] +
  " silencio=" + out[2] +
  " quadros=" + out[3] +
  " taxa=" + out[4] +
  " canais=" + out[5]
);

audio.close(dev);
console.log("fechou");
