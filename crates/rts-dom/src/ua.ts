// UA-STYLESHEET do HTML — os defaults de cada tag, em TS (não no Rust).
//
// É o equivalente à folha de estilo do agente-usuário do navegador (a que define
// que `<div>` é block, `<span>` é inline, `<h1>` é grande e tem margem, etc). Vive
// aqui, como DADOS, e é injetada como PRELUDE em todo programa (padrão CONSOLE_TS/
// DOM facade) — então o motor de layout em Rust NÃO nomeia nenhuma tag HTML: só lê
// o que esta folha registra. Quer ajustar `<dialog>`? Edite a lista; o Rust não muda.
//
// Cada entrada é UM OBJETO com tudo de uma tag junto (em vez de arrays paralelos):
//   tag      — nome da tag
//   display  — 0=block(vertical) 1=wrap(inline-flow) 2=flex(horizontal). default block.
//   margin   — margem vertical default (px), o espaço entre blocos. default 0.
//   fontSize — tamanho de fonte (px) p/ headings. 0 = herda. default 0.
//   heading  — true: texto forte (a flag HEADING; usa fontSize como tamanho).
//   inline   — flags de ênfase inline (negrito/itálico/mono). Se setado, é inline.
// Tags NÃO listadas e desconhecidas são inline transparentes (como `<span>`/`<b>`).

// slots de estilo (espelham style.rs): 9 = margin VERTICAL (só top/bottom — os
// margins default do navegador são `margin: Npx 0`, não afetam o eixo horizontal).
const __UA_SLOT_MARGIN = 9;
// flags inline (espelham block.rs): bold=8 italic=16 mono=1.
const __UA_BOLD = 8, __UA_ITALIC = 16, __UA_MONO = 1;

const __UA: {
  tag: string;
  display: number;
  margin: number;
  fontSize: number;
  heading: boolean;
  inline: number;
}[] = [
  // ── Blocos de fluxo estruturais (block, sem margem por padrão) ──────────────
  { tag: "html", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "body", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "div", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "section", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "header", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "footer", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "main", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "article", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "aside", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "nav", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "figcaption", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "address", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "hr", display: 0, margin: 8, fontSize: 0, heading: false, inline: 0 },

  // ── Blocos com margem vertical (o navegador separa estes) ───────────────────
  { tag: "p", display: 0, margin: 16, fontSize: 0, heading: false, inline: 0 },
  { tag: "ul", display: 0, margin: 16, fontSize: 0, heading: false, inline: 0 },
  { tag: "ol", display: 0, margin: 16, fontSize: 0, heading: false, inline: 0 },
  { tag: "li", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "blockquote", display: 0, margin: 16, fontSize: 0, heading: false, inline: 0 },
  { tag: "pre", display: 0, margin: 13, fontSize: 0, heading: false, inline: __UA_MONO },
  { tag: "figure", display: 0, margin: 16, fontSize: 0, heading: false, inline: 0 },
  { tag: "form", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },
  { tag: "table", display: 0, margin: 0, fontSize: 0, heading: false, inline: 0 },

  // ── Cabeçalhos (block, forte, com tamanho + margem) ─────────────────────────
  { tag: "h1", display: 0, margin: 21, fontSize: 32, heading: true, inline: 0 },
  { tag: "h2", display: 0, margin: 16, fontSize: 24, heading: true, inline: 0 },
  { tag: "h3", display: 0, margin: 16, fontSize: 19, heading: true, inline: 0 },
  { tag: "h4", display: 0, margin: 16, fontSize: 16, heading: true, inline: 0 },
  { tag: "h5", display: 0, margin: 16, fontSize: 13, heading: true, inline: 0 },
  { tag: "h6", display: 0, margin: 16, fontSize: 11, heading: true, inline: 0 },

  // ── Inlines de ênfase (transparentes; só ligam bits de estilo) ──────────────
  { tag: "b", display: 0, margin: 0, fontSize: 0, heading: false, inline: __UA_BOLD },
  { tag: "strong", display: 0, margin: 0, fontSize: 0, heading: false, inline: __UA_BOLD },
  { tag: "i", display: 0, margin: 0, fontSize: 0, heading: false, inline: __UA_ITALIC },
  { tag: "em", display: 0, margin: 0, fontSize: 0, heading: false, inline: __UA_ITALIC },
  { tag: "code", display: 0, margin: 0, fontSize: 0, heading: false, inline: __UA_MONO },
  { tag: "kbd", display: 0, margin: 0, fontSize: 0, heading: false, inline: __UA_MONO },
  { tag: "samp", display: 0, margin: 0, fontSize: 0, heading: false, inline: __UA_MONO },
];

// Aplica a folha: inline-ênfase via defineInline; bloco via defineBlock (+ margem
// via defineStyle quando houver). Um único laço sobre a lista de objetos.
for (let i = 0; i < __UA.length; i = i + 1) {
  const e = __UA[i];
  if (e.inline !== 0) {
    dom.defineInline(e.tag, e.inline);
  } else {
    // flags=4 (HEADING) p/ cabeçalho; indent carrega o tamanho da fonte.
    const flags = e.heading ? 4 : 0;
    dom.defineBlock(e.tag, e.display, e.fontSize, 0, flags);
  }
  if (e.margin !== 0) {
    dom.defineStyle(e.tag, __UA_SLOT_MARGIN, e.margin);
  }
}
