use super::*;

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
    UaEntry {
        tag: "html",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "body",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "div",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "section",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "header",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "footer",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "main",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "article",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "aside",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "nav",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "figcaption",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "address",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "li",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "form",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "table",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    // blocos com margem vertical
    UaEntry {
        tag: "p",
        display: 0,
        margin_v: 16.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "ul",
        display: 0,
        margin_v: 16.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "ol",
        display: 0,
        margin_v: 16.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "blockquote",
        display: 0,
        margin_v: 16.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "pre",
        display: 0,
        margin_v: 13.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_MONO,
    },
    UaEntry {
        tag: "figure",
        display: 0,
        margin_v: 16.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    UaEntry {
        tag: "hr",
        display: 0,
        margin_v: 8.0,
        font_size: 0.0,
        heading: false,
        inline: 0,
    },
    // cabeçalhos (block, forte, tamanho + margem)
    UaEntry {
        tag: "h1",
        display: 0,
        margin_v: 21.0,
        font_size: 32.0,
        heading: true,
        inline: 0,
    },
    UaEntry {
        tag: "h2",
        display: 0,
        margin_v: 16.0,
        font_size: 24.0,
        heading: true,
        inline: 0,
    },
    UaEntry {
        tag: "h3",
        display: 0,
        margin_v: 16.0,
        font_size: 19.0,
        heading: true,
        inline: 0,
    },
    UaEntry {
        tag: "h4",
        display: 0,
        margin_v: 16.0,
        font_size: 16.0,
        heading: true,
        inline: 0,
    },
    UaEntry {
        tag: "h5",
        display: 0,
        margin_v: 16.0,
        font_size: 13.0,
        heading: true,
        inline: 0,
    },
    UaEntry {
        tag: "h6",
        display: 0,
        margin_v: 16.0,
        font_size: 11.0,
        heading: true,
        inline: 0,
    },
    // inlines de ênfase (transparentes; só ligam bits de estilo)
    UaEntry {
        tag: "b",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_BOLD,
    },
    UaEntry {
        tag: "strong",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_BOLD,
    },
    UaEntry {
        tag: "i",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_ITALIC,
    },
    UaEntry {
        tag: "em",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_ITALIC,
    },
    UaEntry {
        tag: "code",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_MONO,
    },
    UaEntry {
        tag: "kbd",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_MONO,
    },
    UaEntry {
        tag: "samp",
        display: 0,
        margin_v: 0.0,
        font_size: 0.0,
        heading: false,
        inline: FLAG_MONO,
    },
];

/// O `display` default da UA para as tags cujo valor NÃO cabe no código inteiro
/// da [`UA_TABLE`] — `list-item` e os quatro de tabela.
///
/// Vive à parte e não como uma coluna nova da tabela por uma razão de forma: o
/// `display` daquela tabela é um `i64` que o TS também escreve (`defineBlock`), e
/// é o EIXO de empilhamento; `table-row` não é um eixo, é um papel dentro de um
/// algoritmo. Alargar o inteiro obrigaria o TS a conhecer códigos que ele nunca
/// escolhe. A alternativa rejeitada foi hardcodar estes nomes dentro do
/// `layout.rs` — o motor não nomeia tags HTML, a UA-stylesheet sim, e este
/// ficheiro É a UA-stylesheet.
///
/// Uma regra de AUTOR vence sempre: quem chama só consulta isto quando o CSS
/// computado não declarou `display`.
pub fn ua_display(tag: &str) -> Option<crate::style::DisplayKind> {
    use crate::style::DisplayKind as D;
    Some(match tag {
        "li" => D::ListItem,
        "table" => D::Table,
        "thead" | "tbody" | "tfoot" => D::TableRowGroup,
        "tr" => D::TableRow,
        "td" | "th" => D::TableCell,
        "caption" => D::TableCaption,
        // `<col>`/`<colgroup>` carregam largura de coluna e não geram caixa.
        "col" | "colgroup" => D::None,
        _ => return None,
    })
}

/// O RECUO default da UA para as listas: `<ul>`/`<ol>` têm
/// `padding-inline-start: 40px` na folha de todo browser, e é o que põe o texto
/// do `<li>` 40px à direita da caixa da lista (o marcador vive nesse recuo).
///
/// Devolvido como função em vez de virar um `SLOT_PADDING` na UA-stylesheet
/// porque aquele slot é o padding dos QUATRO lados, e aplicá-lo daria 40px em
/// cima e em baixo também. Quem chama respeita a precedência: um
/// `padding-left` do autor (`list-style:none;padding-left:0` de um menu) anula
/// este default, como na cascade real.
pub const UA_LIST_INDENT: f32 = 40.0;

/// `true` se a tag é uma das duas caixas de lista que recebem [`UA_LIST_INDENT`].
pub fn is_list_container(tag: &str) -> bool {
    matches!(tag, "ul" | "ol" | "menu" | "dir")
}

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
            define(
                e.tag,
                BlockDef {
                    display: e.display,
                    indent: e.font_size,
                    prefix: PREFIX_NONE,
                    flags,
                },
            );
        }
        if e.margin_v != 0.0 {
            crate::style::define_style(e.tag, crate::style::SLOT_MARGIN_V, e.margin_v as i64);
        }
    }
    // `<center>` (tag legada, viva em páginas anos-2000 — a home legada do
    // google): bloco com text-align:center HERDÁVEL. Centralização de blocos
    // filhos é um refino futuro; o inline-flow centrado já resolve o visual.
    define(
        "center",
        BlockDef {
            display: 0,
            indent: 0.0,
            prefix: PREFIX_NONE,
            flags: 0,
        },
    );
    crate::style::define_style("center", crate::style::SLOT_TEXT_ALIGN, 1);
    // `<a>` — o link default do browser: azul (#0000EE) + sublinhado. Uma regra
    // de AUTOR (`a{color:...}`/`text-decoration:none`) vence (especificidade da
    // tag < classe/id do autor; a UA é a camada mais fraca da cascade).
    crate::style::define_style("a", crate::style::SLOT_COLOR, 0x0000EEFF);
    crate::style::define_style("a", crate::style::SLOT_TEXT_DECORATION, 1);
    // Os CONTROLOS DE FORMULÁRIO não herdam a fonte do documento: a folha do
    // browser dá-lhes uma fonte própria (`font: 400 13.3333px Arial` no Chrome),
    // e é por isso que um `<input>` dentro de um corpo de 16px sai mais pequeno.
    // Medido na página real: são os ÚNICOS 8 elementos de 16 354 em que o nosso
    // `font-size` diverge do Chrome — nós dávamos 16 (herdado) onde ele dá
    // 13,3333. Uma regra de autor vence isto, como qualquer default de UA.
    for tag in ["input", "button", "select", "textarea"] {
        crate::style::define_style_font_px(tag, 13.3333);
    }
}
