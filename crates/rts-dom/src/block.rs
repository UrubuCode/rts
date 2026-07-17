//! "Alocador dinâmico de blocos": o mapa `tag → como renderizar` definido pelo
//! TS, não hardcodado no Rust.
//!
//! O Rust é um motor de layout GENÉRICO — conhece só primitivos (eixo de
//! layout, recuo, prefixo de marcador, flags de tipografia), nunca nomes de tag.
//! O TS registra, via `egui.defineBlock(tag, display, indent, prefix, flags)`, o
//! comportamento de cada tag (`ul`, `table`, `blockquote`, …). Isso espelha a
//! doutrina do projeto (Rust expõe primitivos; a política vive na camada alta) e
//! deixa o conjunto de tags editável SEM recompilar o Rust.
//!
//! Fundamento (CSS): todo elemento é, na base, `inline` ou `block`. Aqui isso
//! vira o eixo `Display`: `block` ⇒ `Vertical`, `inline` ⇒ `Wrap`. Os demais
//! (`Horizontal`, `Grid`) e os modificadores cobrem lista/tabela/pre.

use std::cell::RefCell;
use std::collections::HashMap;

// ── Códigos do eixo de layout (DISPLAY) — devem casar com as consts no TS ──────
/// Empilha os filhos verticalmente, cada um ocupando a linha (CSS `block`).
pub const DISPLAY_VERTICAL: i64 = 0;
/// Flui os filhos lado a lado com quebra ao fim da largura (CSS `inline` flow).
pub const DISPLAY_WRAP: i64 = 1;
/// Coloca os filhos lado a lado SEM quebra (linha de uma tabela, flex-row).
pub const DISPLAY_HORIZONTAL: i64 = 2;
/// Grade 2-D via `egui::Grid` (CSS `table`): cada filho-linha vira uma row.
pub const DISPLAY_GRID: i64 = 3;

// ── Prefixo (marcador de item de lista) ────────────────────────────────────────
pub const PREFIX_NONE: i64 = 0;
pub const PREFIX_BULLET: i64 = 1;
pub const PREFIX_NUMBER: i64 = 2;

// ── Flags de tipografia (bitmask) ──────────────────────────────────────────────
// Reusadas tanto por blocos (`FLAG_*`) quanto por inlines (`INLINE_*`): bold,
// italic e mono são os mesmos bits, então um inline e um bloco podem combiná-los.
/// Fonte monoespaçada (`pre`/`code`).
pub const FLAG_MONO: i64 = 1;
/// Preserva espaços/quebras do texto (`pre`).
pub const FLAG_PRESERVE_WS: i64 = 2;
/// Renderiza o texto como heading forte (`indent` vira o TAMANHO da fonte).
pub const FLAG_HEADING: i64 = 4;
/// Negrito (inline `<b>`/`<strong>`).
pub const FLAG_BOLD: i64 = 8;
/// Itálico (inline `<i>`/`<em>`).
pub const FLAG_ITALIC: i64 = 16;

/// Definição de layout de uma tag. `Copy` — é só um punhado de inteiros.
#[derive(Clone, Copy)]
pub struct BlockDef {
    pub display: i64,
    /// Recuo à esquerda em pontos (lista/blockquote). Para `FLAG_HEADING`,
    /// reaproveitado como TAMANHO de fonte do cabeçalho.
    pub indent: f32,
    pub prefix: i64,
    pub flags: i64,
}

impl BlockDef {
    pub fn has(&self, flag: i64) -> bool {
        self.flags & flag != 0
    }
}

thread_local! {
    /// Mapa tag → BlockDef, povoado pelo TS via `defineBlock`. Vazio até o TS
    /// registrar (a fachada `window.ts` registra os defaults HTML no construtor).
    static BLOCKS: RefCell<HashMap<String, BlockDef>> = RefCell::new(HashMap::new());
    /// Mapa tag inline → flags de estilo (`FLAG_BOLD`/`FLAG_ITALIC`/`FLAG_MONO`),
    /// povoado pelo TS via `defineInline`. Uma tag inline é "transparente": só
    /// liga bits de estilo e desce nos filhos. Tag ausente nos DOIS mapas é
    /// inline transparente sem estilo (default seguro).
    static INLINES: RefCell<HashMap<String, i64>> = RefCell::new(HashMap::new());
}

/// Registra/atualiza o layout de BLOCO de uma tag (primitivo `defineBlock`).
pub fn define(tag: &str, def: BlockDef) {
    BLOCKS.with(|m| {
        m.borrow_mut().insert(tag.to_ascii_lowercase(), def);
    });
    // muda o layout de toda tag registrada → invalida os caches de layout.
    crate::style::props::bump_style_epoch();
}

/// Registra/atualiza o estilo INLINE de uma tag (primitivo `defineInline`).
pub fn define_inline(tag: &str, flags: i64) {
    INLINES.with(|m| {
        m.borrow_mut().insert(tag.to_ascii_lowercase(), flags);
    });
    crate::style::props::bump_style_epoch();
}

/// Consulta o layout de BLOCO de uma tag. `None` ⇒ não é bloco.
pub fn lookup(tag: &str) -> Option<BlockDef> {
    BLOCKS.with(|m| m.borrow().get(tag).copied())
}

/// Consulta os flags de estilo INLINE de uma tag. `0` ⇒ sem estilo (transparente).
pub fn lookup_inline(tag: &str) -> i64 {
    INLINES.with(|m| m.borrow().get(tag).copied().unwrap_or(0))
}

// ── UA-stylesheet do HTML (defaults de cada tag) — DADOS, não lógica ─────────────
// O equivalente à folha do agente-usuário do navegador: quais tags são block,
// quais inline-ênfase, tamanhos de heading, margem vertical default. É uma TABELA
// (não um `match` espalhado), instalada via `install_ua_defaults` quando o primeiro
// DOM é criado (`parse_html_to_dom`) — então NÃO roda em programas sem DOM (era um
// prelude `.ts` antes, mas isso quebrava todo programa: o `ua.ts` chamava `dom.*`
// no top-level e `dom` é unbound sem `import "rts:dom"`). O motor de LAYOUT não
// nomeia nenhuma tag; só lê o que esta tabela registra.

/// Uma entrada da UA-stylesheet: tudo de uma tag junto (lista de objetos).
struct UaEntry {
    tag: &'static str,
    /// display: 0=block(vertical) 1=wrap(inline-flow) 2=flex. (inline-ênfase usa `inline`.)
    display: i64,
    /// margem vertical default (top/bottom), em pontos (0 = nenhuma).
    margin_v: f32,
    /// tamanho de fonte para heading (0 = não-heading / herda).
    font_size: f32,
    /// `true`: cabeçalho (texto forte; `font_size` é o tamanho).
    heading: bool,
    /// flags inline (FLAG_BOLD/ITALIC/MONO); != 0 ⇒ a tag é inline-ênfase (defineInline).
    inline: i64,
}

/// A tabela da UA-stylesheet — uma linha por tag, todos os defaults juntos.
const UA_TABLE: &[UaEntry] = &[
    // blocos de fluxo (sem margem por padrão)
    UaEntry { tag: "html", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "body", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "div", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "section", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "header", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "footer", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "main", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "article", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "aside", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "nav", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "figcaption", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "address", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "li", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "form", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "table", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: 0 },
    // blocos com margem vertical
    UaEntry { tag: "p", display: 0, margin_v: 16.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "ul", display: 0, margin_v: 16.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "ol", display: 0, margin_v: 16.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "blockquote", display: 0, margin_v: 16.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "pre", display: 0, margin_v: 13.0, font_size: 0.0, heading: false, inline: FLAG_MONO },
    UaEntry { tag: "figure", display: 0, margin_v: 16.0, font_size: 0.0, heading: false, inline: 0 },
    UaEntry { tag: "hr", display: 0, margin_v: 8.0, font_size: 0.0, heading: false, inline: 0 },
    // cabeçalhos (block, forte, tamanho + margem)
    UaEntry { tag: "h1", display: 0, margin_v: 21.0, font_size: 32.0, heading: true, inline: 0 },
    UaEntry { tag: "h2", display: 0, margin_v: 16.0, font_size: 24.0, heading: true, inline: 0 },
    UaEntry { tag: "h3", display: 0, margin_v: 16.0, font_size: 19.0, heading: true, inline: 0 },
    UaEntry { tag: "h4", display: 0, margin_v: 16.0, font_size: 16.0, heading: true, inline: 0 },
    UaEntry { tag: "h5", display: 0, margin_v: 16.0, font_size: 13.0, heading: true, inline: 0 },
    UaEntry { tag: "h6", display: 0, margin_v: 16.0, font_size: 11.0, heading: true, inline: 0 },
    // inlines de ênfase (transparentes; só ligam bits de estilo)
    UaEntry { tag: "b", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: FLAG_BOLD },
    UaEntry { tag: "strong", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: FLAG_BOLD },
    UaEntry { tag: "i", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: FLAG_ITALIC },
    UaEntry { tag: "em", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: FLAG_ITALIC },
    UaEntry { tag: "code", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: FLAG_MONO },
    UaEntry { tag: "kbd", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: FLAG_MONO },
    UaEntry { tag: "samp", display: 0, margin_v: 0.0, font_size: 0.0, heading: false, inline: FLAG_MONO },
];

thread_local! {
    /// Flag de "UA já instalada nesta thread" (idempotência sem custo por-parse).
    static UA_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Instala a UA-stylesheet (uma vez por thread) — `defineBlock`/`defineInline` +
/// margem vertical default de cada tag da [`UA_TABLE`]. Chamado por
/// `parse_html_to_dom` na criação do primeiro DOM, então NÃO roda em programas que
/// não usam o DOM. Idempotente. (margem via `style::define_style` no slot vertical.)
pub fn install_ua_defaults() {
    if UA_INSTALLED.with(|f| f.replace(true)) {
        return; // já instalada nesta thread.
    }
    for e in UA_TABLE {
        if e.inline != 0 {
            define_inline(e.tag, e.inline);
        } else {
            let flags = if e.heading { FLAG_HEADING } else { 0 };
            define(e.tag, BlockDef { display: e.display, indent: e.font_size, prefix: PREFIX_NONE, flags });
        }
        if e.margin_v != 0.0 {
            crate::style::define_style(e.tag, crate::style::SLOT_MARGIN_V, e.margin_v as i64);
        }
    }
    // `<center>` (tag legada, viva em páginas anos-2000 — a home legada do
    // google): bloco com text-align:center HERDÁVEL. Centralização de blocos
    // filhos é um refino futuro; o inline-flow centrado já resolve o visual.
    define("center", BlockDef { display: 0, indent: 0.0, prefix: PREFIX_NONE, flags: 0 });
    crate::style::define_style("center", crate::style::SLOT_TEXT_ALIGN, 1);
}
