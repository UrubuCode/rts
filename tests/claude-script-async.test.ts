import { describe, test, expect } from "rts:test";

// async function DENTRO do <script> da página (#207 fatia 2): declara, chama,
// encadeia .then com o valor resolvido CORRETO (int e double — a word do
// settle não é mais re-adivinhada pela ponte raw).
// Pré-computado no top-level; o then roda no drain do event loop, então o
// resultado é capturado via var de módulo checada num segundo describe... como
// o suite roda tudo síncrono antes do drain, validamos o que É observável
// sincronamente: compilar + disparar + o contrato do valor via await num async
// de teste do programa principal.

// 1. o script compila e roda (antes: 'unrecognized statement async function')
const html = "<div id='t'>x</div>" +
  "<script>" +
  "async function carrega() { return 42; }" +
  "carrega().then(function(v) {" +
  "  const el = document.getElementById('t');" +
  "  if (el !== null) { el.setAttribute('data-v', '' + v); }" +
  "});" +
  "</script>";
const doc = parseDocument(html);
const ran = runScripts(doc);

// 2. valores corretos fim-a-fim (mesma maquinaria do script): int e double
//    resolvidos via promise.wait (síncrono-observável no top-level; um gcell
//    escrito de dentro do async não serve — o corpo roda em worker e o gcell é
//    thread_local).
import { promise } from "rts";
async function intAsync(): i64 { return 8; }
const vInt = promise.wait(intAsync());
async function somaAsync(): f64 {
  const b = 1.5;
  return b + 8.0; // 9.5 — se a word do double fosse re-adivinhada, viraria lixo
}
// promise.wait é a superfície CRUA legada (trunca double); o double correto se
// verifica com o await do MOTOR (que reboxa a word) — devolvendo a comparação
// como int pela superfície crua.
async function checaDouble(): i64 {
  const b = await somaAsync();
  if (b === 9.5) return 1;
  return 0;
}
const vSoma = promise.wait(checaDouble());

describe("async no <script> + valores corretos (#207 fatia 2)", () => {
  test("script com async function compila e roda", () => {
    expect(ran).toBe(1);
  });
  test("int resolvido correto", () => {
    expect(vInt).toBe(8);
  });
  test("double resolvido correto (via await do motor)", () => {
    expect(vSoma).toBe(1);
  });
});
