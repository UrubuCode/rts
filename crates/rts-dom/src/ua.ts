// UA-STYLESHEET do HTML — os defaults de `display` das tags, em TS (não no Rust).
//
// É o equivalente à folha de estilo do agente-usuário do navegador (a que define
// que `<div>` é block, `<span>` é inline, etc). Vive aqui, como DADOS no TS, e é
// injetada como PRELUDE em todo programa (padrão CONSOLE_TS/DOM facade) — então o
// motor de layout em Rust NÃO nomeia nenhuma tag HTML: ele só lê o que foi
// registrado. Quer que `<dialog>` seja block? Registre aqui; o Rust não muda.
//
// Mecanismo: `dom.defineBlock(tag, display, indent, prefix, flags)` é o sistema de
// criação de tags já existente. display: 0=block(vertical) 1=wrap(inline-flow)
// 2=flex(horizontal). Tags NÃO registradas e desconhecidas são inline por padrão
// (como `<span>`/`<b>`), salvo `display:`/`defineBlock` explícito do autor.

// Blocos de fluxo (vertical, ocupam a largura) — o grosso do HTML estrutural.
const __UA_BLOCK = [
  "html", "body", "div", "p", "section", "header", "footer", "main",
  "article", "aside", "nav", "blockquote", "pre", "figure", "figcaption",
  "ul", "ol", "li", "dl", "dt", "dd", "form", "fieldset", "table",
  "thead", "tbody", "tfoot", "tr", "address", "hr",
];
for (let i = 0; i < __UA_BLOCK.length; i = i + 1) {
  dom.defineBlock(__UA_BLOCK[i], 0, 0, 0, 0);
}

// Cabeçalhos: block, com o tamanho de fonte default embutido no `indent` (o render
// usa indent como tamanho quando a flag HEADING está ligada). h1 maior → h6 menor.
const __UA_HEADINGS = ["h1", "h2", "h3", "h4", "h5", "h6"];
const __UA_HSIZE = [32, 24, 19, 16, 13, 11];
for (let i = 0; i < __UA_HEADINGS.length; i = i + 1) {
  // flags=4 (HEADING): texto forte; indent carrega o tamanho.
  dom.defineBlock(__UA_HEADINGS[i], 0, __UA_HSIZE[i], 0, 4);
}

// Inlines de ênfase: transparentes, só ligam bits de estilo (negrito/itálico/mono).
dom.defineInline("b", 8);       // FLAG_BOLD
dom.defineInline("strong", 8);
dom.defineInline("i", 16);      // FLAG_ITALIC
dom.defineInline("em", 16);
dom.defineInline("code", 1);    // FLAG_MONO
dom.defineInline("kbd", 1);
dom.defineInline("samp", 1);
