// Fumaça de `audio.escuta_iniciar`/`audio.escuta_ler` pelo motor real, com o
// dispositivo de saída REAL (não o nulo): toca um tom de 997 Hz pelas
// nativas existentes e escuta por loopback SEM bloquear a thread principal
// — como o motor híbrido exige (o humano usa a janela enquanto o agente
// escuta ao mesmo tempo). Faz o polling de `escuta_ler` a cada "quadro"
// simulado, do mesmo jeito que o jogo faria.
//   target/release/rts.exe run crates/rts-audio/tests/escuta_fumaca.ts
import audio from "rts:audio";
import { time } from "rts";

const dev = audio.open_output(48000, 2, 0);
console.log("abriu " + dev + " taxa=" + audio.sample_rate(dev) + " canais=" + audio.channels(dev));

const taxa = 48000;
const canais = 2;
const duracaoMs = 500;
const freqHz = 997;
const quadros = Math.floor((taxa * duracaoMs) / 1000);
const bloco = new Float32Array(quadros * canais);
for (let i = 0; i < quadros; i++) {
  const v = Math.sin((2 * Math.PI * freqHz * i) / taxa) * 0.5;
  bloco[i * canais] = v;
  bloco[i * canais + 1] = v;
}
console.log("escritas " + audio.write(dev, bloco, bloco.length));

// Dá tempo do dispositivo começar a tocar antes de escutar.
time.sleep_ms(50);

const iniciou = audio.escuta_iniciar(300, freqHz);
console.log("escuta_iniciar ok=" + iniciou);

// Polling não bloqueante: a thread principal continua livre entre chamadas —
// é assim que o agente escutaria por quadro, sem travar a janela do humano.
const out = new Float64Array(8);
let pronto = 0;
let voltas = 0;
while (pronto === 0 && voltas < 200) {
  pronto = audio.escuta_ler(out);
  if (pronto === 0) time.sleep_ms(10);
  voltas++;
}
console.log(
  "escuta_ler pronto=" + pronto + " voltas=" + voltas +
  " rms=" + out[0] +
  " pico=" + out[1] +
  " silencio=" + out[2] +
  " quadros=" + out[3] +
  " taxa=" + out[4] +
  " canais=" + out[5] +
  " energiaFreq=" + out[6] +
  " underruns=" + out[7]
);

audio.close(dev);
console.log("fechou");
