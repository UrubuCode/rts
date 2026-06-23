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
}

/// Registra/atualiza o estilo INLINE de uma tag (primitivo `defineInline`).
pub fn define_inline(tag: &str, flags: i64) {
    INLINES.with(|m| {
        m.borrow_mut().insert(tag.to_ascii_lowercase(), flags);
    });
}

/// Consulta o layout de BLOCO de uma tag. `None` ⇒ não é bloco.
pub fn lookup(tag: &str) -> Option<BlockDef> {
    BLOCKS.with(|m| m.borrow().get(tag).copied())
}

/// Consulta os flags de estilo INLINE de uma tag. `0` ⇒ sem estilo (transparente).
pub fn lookup_inline(tag: &str) -> i64 {
    INLINES.with(|m| m.borrow().get(tag).copied().unwrap_or(0))
}
