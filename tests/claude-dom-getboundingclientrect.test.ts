import { describe, test, expect } from "rts:test";

// `Element.getBoundingClientRect()` estava morto em produção: a fachada chamava
// `dom.boundingComponent(doc, node, vw, which)`, uma chave que o bridge nunca
// registou (a chave certa é `boundingRect`, e sem o 4º argumento — a função Rust
// não recebe viewport por chamada). `el.getBoundingClientRect(1280)` lançava
// `TypeError: dom.boundingComponent is not a function` para TODO chamador, e
// nenhum `*.test.ts` a exercia — é este ficheiro que fecha essa lacuna.
//
// A caixa vem do layout real (box model), não de um valor fixo: `width`/`height`
// explícitos no `style=""` inline vencem o conteúdo (`bloco.rs`, `explicit_content_h
// .unwrap_or(content_h)` e o mesmo ramo para `width`).
//
// x/y NÃO são a origem do viewport: este ficheiro nasceu em `e6bfeb7d8` (vaga 1,
// 2026-09-04), quando a folha de UA ainda não dava margem nenhuma a `body`. O
// lote I (`ccc266cfd`, mesma vaga 2) trocou isso por uma folha de UA real —
// `crates/rts-dom/src/style/ua.css` — que tem `body { margin: 8px }`, a mesma
// regra que o Chrome aplica (e que `body { margin: 0 }`, o reset mais comum da
// web, existe precisamente para revogar). `doc.getElementById` só existe depois
// de `parseDocument` ter completado a árvore `html > head/body`, então o `div`
// deste teste é o 1º filho de um `body` com 8px de margem UA dos dois lados —
// x=8, y=8, como no Chrome. Este comentário e a asserção abaixo diziam x=0/y=0,
// que era certo ANTES do lote I e ficou stale depois dele; corrigido aqui pela
// regra do brief (teste que contradiz o Chrome real é o lado errado).
const html = "<div id='a' style='width:100px;height:20px'>x</div>";
const doc = parseDocument(html);
const a = doc.getElementById("a");
const rect = a === null ? null : a.getBoundingClientRect();

describe("Element.getBoundingClientRect()", () => {
  test("resolve (não lança TypeError) e lê a caixa do layout", () => {
    expect(rect).not.toBeNull();
  });
  test("width/height vêm do style inline explícito", () => {
    if (rect === null) { throw new Error("elemento #a não encontrado"); }
    expect(rect.width).toBe(100);
    expect(rect.height).toBe(20);
  });
  test("x/y somam a margem UA do body (8px), o 1º filho não tem margem própria", () => {
    if (rect === null) { throw new Error("elemento #a não encontrado"); }
    expect(rect.x).toBe(8);
    expect(rect.y).toBe(8);
  });
  test("top/left/right/bottom derivam de x/y/width/height", () => {
    if (rect === null) { throw new Error("elemento #a não encontrado"); }
    expect(rect.top).toBe(rect.y);
    expect(rect.left).toBe(rect.x);
    expect(rect.right).toBe(rect.x + rect.width);
    expect(rect.bottom).toBe(rect.y + rect.height);
  });
});
