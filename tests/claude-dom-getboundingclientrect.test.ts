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
// .unwrap_or(content_h)` e o mesmo ramo para `width`), e o primeiro filho do
// documento, sem margem (UA-stylesheet não dá margem a `div`/`body`), fica na
// origem do viewport — x=0, y=0.
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
  test("x/y são a origem do viewport (1º filho, sem margem UA)", () => {
    if (rect === null) { throw new Error("elemento #a não encontrado"); }
    expect(rect.x).toBe(0);
    expect(rect.y).toBe(0);
  });
  test("top/left/right/bottom derivam de x/y/width/height", () => {
    if (rect === null) { throw new Error("elemento #a não encontrado"); }
    expect(rect.top).toBe(rect.y);
    expect(rect.left).toBe(rect.x);
    expect(rect.right).toBe(rect.x + rect.width);
    expect(rect.bottom).toBe(rect.y + rect.height);
  });
});
