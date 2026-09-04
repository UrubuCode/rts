import { describe, test, expect } from "rts:test";

// O escopo de uma página não vê o Node — nem por leitura NUA, nem por `eval`.
//
// `crates/rts-codegen/src/emit/globals.rs` tem uma lista `NODE_ONLY`
// (`process`, `Buffer`, `setImmediate`, `require`, …) e um comentário que cita
// o bug real que ela existe para evitar: com `setImmediate` visível, o
// scheduler do React 18 escolhia o ramo Node em vez do ramo DOM, e `#root`
// ficava vazio sem um erro. A auditoria estrutural de 2026-09-04
// (`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/07-critico.md`)
// mediu AO VIVO que essa lista sozinha não bastava: um `<script>` de página
// lia `process`/`Buffer`/`setImmediate` com valor REAL, tanto por leitura nua
// quanto por `eval('process')`. Duas causas, independentes:
//
//   1. `rts_core::entry::environment_names` — usado por `eval` e pela
//      compilação de `<script>` de página — devolvia os nomes achados na
//      cadeia do objeto-ambiente ANTES de `globals::resolves`/`NODE_ONLY`
//      sequer serem consultados;
//   2. um `eval()` chamado de DENTRO de um `<script>` de página compilava
//      como um programa NOVO, sem saber que o escopo que o chamou era uma
//      página — então `eval("process")` reabria a porta mesmo que a leitura
//      nua estivesse fechada.
//
// Este ficheiro é a régua da correção das duas, mais o controlo que prova que
// a diferença é o ESCOPO DE PÁGINA e não uma remoção do global: fora de uma
// página, `process` continua real.

// ── controlo: FORA da página, `process` continua um global real ─────────────
//
// Este próprio ficheiro de teste é módulo comum (não um `<script>` de
// página), então se a correção tivesse removido `process` em vez de
// escondê-lo só do escopo de página, esta linha teria acusado.
const tipoProcessoFora = typeof process;
const processoTemPid = typeof process.pid === "number";

// ── dentro da página: nem leitura nua, nem `eval`, veem o Node ──────────────
const html =
  "<div id='out'>vazio</div>" +
  "<script>" +
  "  const r = [];" +
  "  r.push(typeof process);" +
  "  r.push(typeof Buffer);" +
  "  r.push(typeof setImmediate);" +
  "  r.push(typeof require);" +
  "  r.push(eval('typeof process'));" +
  "  r.push(typeof setTimeout);" +
  "  r.push(typeof fetch);" +
  "  r.push(typeof document);" +
  "  const el = document.getElementById('out');" +
  "  if (el !== null) { el.setInnerHTML(r.join('|')); }" +
  "</script>";
const doc = parseDocument(html);
const ran = runScripts(doc);
const out = doc.getElementById("out");
const texto = out === null ? "" : out.textContent;
const campos = texto.split("|");

describe("uma página não vê o Node (leitura nua)", () => {
  test("o script compilou e correu", () => {
    expect(ran).toBe(1);
  });

  test("typeof process é undefined dentro do <script>", () => {
    expect(campos[0]).toBe("undefined");
  });

  test("typeof Buffer é undefined dentro do <script>", () => {
    expect(campos[1]).toBe("undefined");
  });

  test("typeof setImmediate é undefined dentro do <script>", () => {
    expect(campos[2]).toBe("undefined");
  });

  test("typeof require é undefined dentro do <script>", () => {
    expect(campos[3]).toBe("undefined");
  });
});

describe("uma página não vê o Node (via eval, a segunda fuga)", () => {
  test("eval(\"typeof process\") de dentro do <script> é undefined", () => {
    expect(campos[4]).toBe("undefined");
  });
});

describe("o que uma página CONTINUA a ver — não é uma remoção do global", () => {
  test("typeof setTimeout é function, como num browser", () => {
    expect(campos[5]).toBe("function");
  });

  test("typeof fetch é function, como num browser", () => {
    expect(campos[6]).toBe("function");
  });

  test("typeof document é object, como num browser", () => {
    expect(campos[7]).toBe("object");
  });
});

describe("controlo: fora da página, process continua um global real", () => {
  test("typeof process é object fora de um <script> de página", () => {
    expect(tipoProcessoFora).toBe("object");
  });

  test("process.pid continua um número real", () => {
    expect(processoTemPid).toBe(true);
  });
});
