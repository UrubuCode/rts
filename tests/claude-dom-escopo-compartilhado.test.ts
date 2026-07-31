import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

// Os `<script>` de uma página compartilham UM escopo global — o script A define
// `requireLazy = …` e o script B, compilado depois, enxerga.
//
// Dois bugs distintos quebravam isso, ambos corrigidos aqui:
//
// 1. A lista de "nomes já publicados" vivia num array `.ts` de prelude. Cada
//    `<script>` é compilado como um PROGRAMA novo (`new Function`), então o
//    `push` de um se perdia antes do próximo ler. Agora a lista é o próprio
//    `DomScope` (Rust), que é o único estado que atravessa programas.
//
// 2. O saco era chaveado por DOIS valores diferentes do mesmo handle: o campo
//    `doc._dom` carrega o valor completo e uma variável/parâmetro `i64` recebe
//    uma versão truncada (#1870). O Proxy gravava sob uma chave e o leitor
//    procurava sob a outra — publicava e "sumia".
//
// O sintoma real: no bootstrap do WhatsApp Web, `__d` (o registrador de módulos
// da Meta) era definido pelo primeiro script e os 9 bundles de aplicação
// morriam em "call to unknown function `__d`" — a página nunca montava.
//
// Pré-computado no top-level (regra do projeto).

// ── um script publica, o SEGUINTE enxerga ──────────────────────────────────
const d1: i64 = dom.parseHtml("<html><body>"
  + "<script>publicado = 42; helper = function(x){ return x * 2 };</script>"
  + "<script>usado = helper(publicado);</script>"
  + "</body></html>");
const doc1 = new Document(d1);
const rodou1 = runScripts(doc1);
const total1 = DomScope.count(doc1._dom);
const temPublicado = DomScope.has(doc1._dom, "publicado");
const temHelper = DomScope.has(doc1._dom, "helper");
const temUsado = DomScope.has(doc1._dom, "usado");

// ── a forma EXATA do loader da Meta: stub + função que empurra pra ele ─────
const d2: i64 = dom.parseHtml("<html><body>"
  + "<script>__d_stub=[],__d=function(a,b,c,e){__d_stub.push([a,b,c,e])};</script>"
  + "<script>__d(\"ModuloUm\",[],function(){},0);__d(\"ModuloDois\",[],function(){},0);</script>"
  + "<script>registrados = __d_stub.length;</script>"
  + "</body></html>");
const doc2 = new Document(d2);
const rodou2 = runScripts(doc2);
const temD = DomScope.has(doc2._dom, "__d");
const temRegistrados = DomScope.has(doc2._dom, "registrados");

// ── um script QUEBRADO não publica os globais dele (como no browser) ───────
const d3: i64 = dom.parseHtml("<html><body>"
  + "<script>antes = 1;</script>"
  + "<script>quebrado = ((((;</script>"
  + "<script>depois = 2;</script>"
  + "</body></html>");
const doc3 = new Document(d3);
const rodou3 = runScripts(doc3);
const temAntes = DomScope.has(doc3._dom, "antes");
const temDepois = DomScope.has(doc3._dom, "depois");
const temQuebrado = DomScope.has(doc3._dom, "quebrado");

// ── documentos diferentes têm escopos SEPARADOS ────────────────────────────
const d4: i64 = dom.parseHtml("<html><body><script>soDaqui = 1;</script></body></html>");
const doc4 = new Document(d4);
runScripts(doc4);
const vazouParaOutro = DomScope.has(doc1._dom, "soDaqui");

describe("escopo global compartilhado entre <script>", () => {
  test("o segundo script enxerga o que o primeiro publicou", () => {
    expect(rodou1).toBe(2);
    expect(temPublicado).toBe(1);
    expect(temHelper).toBe(1);
    expect(temUsado).toBe(1);
    expect(total1).toBe(3);
  });

  test("padrão do loader da Meta (__d + stub) atravessa os scripts", () => {
    expect(rodou2).toBe(3);
    expect(temD).toBe(1);
    expect(temRegistrados).toBe(1);
  });

  test("script quebrado não derruba os outros nem publica o seu", () => {
    expect(rodou3).toBe(2);
    expect(temAntes).toBe(1);
    expect(temDepois).toBe(1);
    expect(temQuebrado).toBe(0);
  });

  test("documentos diferentes não compartilham escopo", () => {
    expect(vazouParaOutro).toBe(0);
  });
});
