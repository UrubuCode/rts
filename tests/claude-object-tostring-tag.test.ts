import { describe, test, expect } from "rts:test";

// `Object.prototype.toString.call(v)` — o `[object <Tag>]` da spec, e O idioma
// de checagem de tipo do ecossistema JS (`… === "[object Array]"`), usado
// porque é o único teste que atravessa realms e não pode ser enganado por uma
// propriedade forjada.
//
// Dois defeitos independentes, ambos SILENCIOSOS:
//
// 1. `Object.prototype.toString` devolvia a CONSTANTE "[object Object]",
//    ignorando o receiver — um ARRAY se reportava como `Object`. Agora um
//    trampolim único (`__rtsadp_object_tag`) computa a tag pelo valor.
//
// 2. Um atalho no front reescrevia `X.prototype.M.call(recv, …)` como
//    `recv.M(…)` — o "idioma do método emprestado". Correto para
//    `hasOwnProperty`/`slice`/`join`, e ERRADO justamente para `toString`:
//    é o método que TODO builtin sobrescreve, então reescrever descarta o
//    empréstimo e chama a versão do próprio receiver.
//    `Object.prototype.toString.call([1])` respondia `"1"` (o join do array).
//    `toString`/`toLocaleString` agora ficam fora do atalho; os outros
//    empréstimos seguem intactos.
//
// O 1 sozinho não bastava: a forma HOISTADA (`const t = …; t.call(x)`) passou a
// funcionar enquanto a INLINE — que é a que bibliotecas escrevem — continuava
// errada. Só a fixture cross-runtime pegou isso.
//
// ESCOPO CONHECIDO: receiver PRIMITIVO (`.call(1)`, `.call(null)`) ainda
// diverge — o despacho de `.call` sobre não-objeto é outro caminho e continua
// resolvendo pelo receiver. Registrado, não mascarado.
//
// Valores conferidos contra Node e Bun (fixture cross-runtime
// tests/cross-runtime/object-meta/425_object_prototype_tostring_tag.ts).

const tag = Object.prototype.toString;

// ── forma HOISTADA ──────────────────────────────────────────────────────────
const hObj = tag.call({});
const hArr = tag.call([1, 2, 3]);
const hVazio = tag.call([]);

// ── forma INLINE (a que bibliotecas escrevem) ───────────────────────────────
const iTopo = Object.prototype.toString.call([1]);
function ehArray(x: any): any {
  return Object.prototype.toString.call(x) === "[object Array]";
}
const ehArr = ehArray([1]);
const ehObj = ehArray({});
const ehStr = ehArray("abc");

// ── instância de classe: sem Symbol.toStringTag o resultado é Object ────────
class Caixa { v = 1; }
const inst = tag.call(new Caixa());

// ── o idioma do método EMPRESTADO não pode ter regredido ────────────────────
const hop = Object.prototype.hasOwnProperty.call({ a: 1 }, "a");
const slice = Array.prototype.slice.call([1, 2, 3], 1).join("|");
const join = Array.prototype.join.call([1, 2], "-");

// ── o toString NORMAL (não via .call) não pode ter mudado ───────────────────
const direto = ({} as any).toString();
const diretoArr = [1, 2].toString();
const concat = "" + ({} as any);
const viaString = String({});

describe("Object.prototype.toString.call — a tag da spec", () => {
  test("forma hoistada distingue objeto de array", () => {
    expect(hObj).toBe("[object Object]");
    expect(hArr).toBe("[object Array]");
    expect(hVazio).toBe("[object Array]");
  });
  test("forma inline funciona igual à hoistada", () => {
    expect(iTopo).toBe("[object Array]");
  });
  test("o idioma de checagem de tipo de biblioteca", () => {
    expect(ehArr).toBe(true);
    expect(ehObj).toBe(false);
    expect(ehStr).toBe(false);
  });
  test("instância de classe reporta Object", () => {
    expect(inst).toBe("[object Object]");
  });
  test("outros métodos emprestados seguem intactos", () => {
    expect(hop).toBe(true);
    expect(slice).toBe("2|3");
    expect(join).toBe("1-2");
  });
  test("toString normal não mudou", () => {
    expect(direto).toBe("[object Object]");
    expect(diretoArr).toBe("1,2");
    expect(concat).toBe("[object Object]");
    expect(viaString).toBe("[object Object]");
  });
});
