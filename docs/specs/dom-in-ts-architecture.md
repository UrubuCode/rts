# DOM em TS + egui canvas burro — arquitetura (acordada 2026-06-25)

> Decisão tomada nesta sessão com o usuário, incorporando a visão do outro dev.
> Referência de fidelidade da API: **MDN — Document Object Model**
> (https://developer.mozilla.org/en-US/docs/Web/API/Document_Object_Model).
> Substitui o desenho anterior "egui faz o layout" do roadmap F0–F5 a partir do
> ponto em que o layout migra para o TS (ver "Relação com o roadmap" no fim).

## Por que o DOM (e o layout) em TS

Não é ergonomia — é **paralelismo**. O RTS tem paralelização silenciosa (passes
que reescrevem código TS para rodar em rayon automaticamente). Se o **cálculo de
layout do DOM for código TS**, o paralelizador do RTS o alcança: layout de N nós
independentes vira paralelo de graça. Um motor de layout em **Rust** (compilado)
fica fora do alcance desses passes. Logo, o layout mora no TS de propósito.

## A divisão de responsabilidades (3 camadas)

```
rts-dom (Rust)          a ÁRVORE + parser + estado. ABI rts:dom p/ LER os nós.
   ↓ (TS lê via ABI)
fachada DOM (TS)        Node/Document/Element/Text/NodeList (spec MDN) + o LAYOUT
   ↓ (comandos)         (calculado em TS → paralelizável). Emite comandos de pintura.
rts-egui (canvas burro) drawRect/drawText/drawLine + measureText. SÓ executa + mede.
```

### Camada 1 — `rts-dom` (Rust): a árvore, não o layout

Fica em Rust por performance de parsing e por já estar pronto/testado:
- parser HTML → árvore em arena, `NodeId` versionado `{gen,idx}`, índices O(1).
- store de `Dom`s por handle (`crate::store`, fonte única da verdade).
- ABI `rts:dom` de LEITURA da árvore que o TS consome para fazer layout:
  `parseHtml`, `querySelector`/`querySelectorAllCount`/`At`, `childCount`/`At`,
  `getText`, `getAttribute`, `tagName`, `setText`/`setAttr`/`createElement`/
  `appendChild`/`removeNode`, `rootId`. **Nenhum cálculo de layout aqui.**

### Camada 2 — fachada DOM em TS (prelude): a spec + o layout

A fachada `.ts` (hoje `crates/rts-dom/src/dom.ts`) implementa a API DOM **fiel à
MDN** sobre a ABI da camada 1, e — o ponto novo — **calcula o layout em TS**:
- Interfaces (subset MDN, ver tabela abaixo): `Node`, `Document`, `Element`,
  `Text`, `NodeList`/`HTMLCollection`.
- O **layout engine em TS**: percorre a árvore, calcula posições/tamanhos de cada
  caixa (box model: margin/padding/border/width), resolve `width%` contra o pai,
  e para texto usa `measureText` (camada 3). Esse percurso é o que o paralelizador
  do RTS pode acelerar.
- Emite a lista de **comandos de pintura** para a camada 3.

### Camada 3 — `rts-egui` (canvas burro): só pinta + mede texto

O egui deixa de fazer layout. Vira um canvas de primitivos:
- `drawRect(x,y,w,h, fillRGBA, strokeW, strokeRGBA, radius)`
- `drawText(x,y, s, RGBA, size, bold, italic, mono)`
- `drawLine(x1,y1,x2,y2, RGBA, w)`
- `measureText(s, size, bold) -> width` (e altura de linha) — **a única coisa
  "inteligente" que sobra**, porque medir texto exige a fonte (atlas do egui).
  O TS NÃO mede texto (Risco 1 do roadmap: nunca reimplementar `glyph_width`).
- o loop: TS calcula layout → manda comandos → egui pinta o frame.

## Por que o egui NÃO some de vez (o muro do texto)

A versão pura "egui só pinta retângulos prontos, TS faz 100%" é **impossível
hoje**: para calcular onde uma linha quebra, o TS precisaria das métricas da
fonte (largura de cada glifo), que vivem no egui/wgpu. Reimplementar métricas de
fonte em TS é grande e lento (Risco 1). Então o egui retém EXATAMENTE uma
responsabilidade inteligente: **medir texto** (`measureText`). Todo o resto do
layout é TS.

## Subset DOM a implementar (fiel à MDN, fases)

| Interface | Membros (fase 1 = mínimo viável) |
|---|---|
| `Node` | `nodeType`, `nodeName`, `parentNode`, `childNodes`, `firstChild`, `nextSibling`, `textContent` (get/set), `appendChild`, `removeChild`, `insertBefore` |
| `Document` | `documentElement`, `body`, `querySelector`, `querySelectorAll`, `getElementById`, `createElement`, `createTextNode` |
| `Element` | `tagName`, `id`, `className`, `classList`, `textContent`, `innerHTML`, `attributes`, `children`, `getAttribute`/`setAttribute`/`removeAttribute`/`hasAttribute`, `querySelector`/`querySelectorAll`, `appendChild`/`removeChild`/`insertBefore`, `remove` |
| `Text` | `data` |
| `NodeList`/`HTMLCollection` | `length`, acesso por índice |

Já implementado (fase 0, `dom.ts`): `Document` (querySelector/querySelectorAll/
getElementById/createElement/documentElement) + `Element` (textContent get/set,
tagName, id, className, getAttribute/setAttribute/hasAttribute, querySelector/
querySelectorAll, children, appendChild, remove). Validado E2E.

**Regras de design impostas pelo motor (levantadas empiricamente):**
1. Toda propriedade pública = getter/setter, nunca campo público lido pós-chamada.
2. APIs `T | null` (querySelector) = MÉTODOS de classe, nunca funções livres.

## Plano de implementação (fatias)

1. **F-canvas:** egui expõe `drawRect`/`drawText`/`drawLine`/`measureText`
   (substitui/coexiste com `egui.html`). Loop: TS manda comandos.
2. **F-layout-TS:** layout engine mínimo em TS (blocos verticais + texto via
   `measureText`) emitindo comandos. Substitui o `render.rs` que fazia layout.
3. **F-dom-spec:** completar a fachada à tabela MDN acima (Node/Text/classList/
   innerHTML/attributes/insertBefore).
4. **F-paralelo:** validar que o paralelizador do RTS pega o loop de layout.

## Relação com o roadmap F0–F5 (`docs/specs/html-engine/`)

O roadmap dizia "egui faz o layout por padrão". Esta decisão **diverge** a partir
do momento em que o layout migra pro TS (motivada pelo paralelismo, que o roadmap
não considerou). O `rts-dom` (árvore/estado) e a doutrina seguem; o que muda é
QUEM calcula o layout. Atualizar o roadmap quando esta arquitetura estabilizar.

## O que NÃO se perde do trabalho já feito

- `rts-dom` (árvore, parser, NodeId, store, ABI de leitura): **base da camada 1**.
- fachada `document`/`Element` (`dom.ts`): **base da camada 2** (ganha o layout).
- `style.rs`/`block.rs` migrados pro rts-dom: o **estado** que o layout-TS lê.
- O `render.rs` do egui (layout via egui nativo) é o que será **substituído** pelo
  canvas burro + layout-TS — mas serve de referência do comportamento esperado.
