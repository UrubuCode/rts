import { describe, test, expect } from "rts:test";
import dom from "rts:dom";
import { time } from "rts";

// Timers de página (`setTimeout`/`setInterval` chamados por um `<script>`)
// disparam via a FILA POR DOCUMENTO em Rust (`DomTimers`), bombeada pelo frame
// do host (`pumpTimerCallbacks`).
//
// Por que Rust e não prelude `.ts`: cada `new Function` compila um PROGRAMA
// novo — uma fila `.ts` seria por-programa, o script agendaria num programa e o
// pump do host (outro programa) drenaria uma fila vazia para sempre. Era
// exatamente o sintoma: página estática, "não vejo nada mudar" — o script
// rodava, mas nenhum callback de timer disparava nunca.
//
// O callback atravessa como `Poly` (função volta CHAMÁVEL; com f64 vira
// undefined) — o mesmo contrato do `DomScope`.
//
// Pré-computado no top-level (regra do projeto). Os tempos têm folga proposital
// (intervalo 50ms, ~1s de pump) para não flakear em máquina lenta.

const html = "<html><body>"
  + "<p id='alvo'>original</p><p id='tick'>0</p>"
  + "<script>"
  + "document.getElementById('alvo').textContent = 'mudado';"
  + "var n = 0;"
  + "setInterval(function(){ n = n + 1; document.getElementById('tick').textContent = '' + n; }, 50);"
  + "setTimeout(function(){ document.getElementById('alvo').textContent = 'timeout disparou'; }, 100);"
  + "</script></body></html>";

const d: i64 = dom.parseHtml(html);
const doc = new Document(d);
const rodou = runScripts(doc);

const alvo = dom.querySelector(d, "#alvo");
const tick = dom.querySelector(d, "#tick");
// Mutação SÍNCRONA do script (sem timer) já aparece antes de qualquer pump.
const aposScript = dom.getText(d, alvo);

// Sem pump, nada dispara — mesmo dormindo além do prazo.
time.sleep_ms(120);
const semPump = dom.getText(d, alvo);

// Simula o loop de frames por ~1s.
let frames = 0;
while (frames < 60) {
  pumpTimerCallbacks(doc);
  time.sleep_ms(16);
  frames = frames + 1;
}
const aposPump = dom.getText(d, alvo);
const ticks = Number(dom.getText(d, tick));

// `clearInterval` de verdade: agenda outro, cancela, pump — não dispara.
const d2: i64 = dom.parseHtml("<html><body><p id='x'>0</p>"
  + "<script>var id = setInterval(function(){ document.getElementById('x').textContent = 'vazou'; }, 30);"
  + "clearInterval(id);</script></body></html>");
const doc2 = new Document(d2);
runScripts(doc2);
let f2 = 0;
while (f2 < 10) { pumpTimerCallbacks(doc2); time.sleep_ms(16); f2 = f2 + 1; }
const cancelado = dom.getText(d2, dom.querySelector(d2, "#x"));

describe("timers de página via pumpTimerCallbacks", () => {
  test("script roda e a mutação síncrona aparece", () => {
    expect(rodou).toBe(1);
    expect(aposScript).toBe("mudado");
  });

  test("sem pump nenhum timer dispara (o host dirige o tempo)", () => {
    expect(semPump).toBe("mudado");
  });

  test("setTimeout dispara no pump", () => {
    expect(aposPump).toBe("timeout disparou");
  });

  test("setInterval re-arma e acumula ticks", () => {
    // ~1s de pump com período de 50ms: ≥3 com folga generosa p/ máquina lenta.
    expect(ticks >= 3).toBe(true);
  });

  test("clearInterval cancela antes do primeiro disparo", () => {
    expect(cancelado).toBe("0");
  });
});
