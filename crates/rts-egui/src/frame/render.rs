//! Render do DOM RETIDO sobre um `egui::Ui`. "Render em cima da árvore": a fonte
//! da verdade é a hierarquia de nós, e o COMO de cada tag vem do mapa
//! `block::lookup`/`lookup_inline` (definido pelo TS), não de nomes hardcodados.
//! O Rust só aplica primitivos de layout.

/// Estilo inline herdado ao descer na árvore — flags de tag (`<b>`/`<i>`) MAIS as
/// propriedades CSS computadas do `style="..."` (cor/tamanho). Herdado: filhos
/// começam do estilo do pai; o próprio `style` de cada nó sobrepõe.
#[derive(Clone, Copy, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    mono: bool,
    color: Option<egui::Color32>,
    size: Option<f32>,
}

/// Converte a cor própria do motor de estilo (`u32` RGBA `0xRRGGBBAA`, egui-free)
/// para o `Color32` do egui. A conversão vive AQUI (no render), não no `style.rs`,
/// que é deliberadamente egui-free (F0(d) do roadmap).
fn rgba_to_color32(c: crate::style::Rgba) -> egui::Color32 {
    let r = ((c >> 24) & 0xFF) as u8;
    let g = ((c >> 16) & 0xFF) as u8;
    let b = ((c >> 8) & 0xFF) as u8;
    let a = (c & 0xFF) as u8;
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Mescla o `style="..."` (CSS inline) de um nó SOBRE um `InlineStyle` herdado.
/// Propriedade ausente no CSS mantém a herdada; presente sobrescreve.
fn merge_node_style(dom: &crate::dom::Dom, id: crate::dom::NodeIdx, mut st: InlineStyle) -> InlineStyle {
    if let Some(s) = dom.node(id).attr("style") {
        let css = crate::style::parse_inline(s);
        if let Some(c) = css.color {
            st.color = Some(rgba_to_color32(c));
        }
        if let Some(sz) = css.font_size {
            st.size = Some(sz);
        }
        if let Some(b) = css.bold {
            st.bold = b;
        }
        if let Some(i) = css.italic {
            st.italic = i;
        }
    }
    st
}

/// Renderiza um `Dom` inteiro no `ui`: cada filho do `#document` é um bloco.
///
/// "Render em cima da árvore": a fonte da verdade é a hierarquia de nós, e o COMO
/// de cada tag vem do mapa `block::lookup`/`lookup_inline` (definido pelo TS),
/// não de nomes hardcodados. O Rust só aplica primitivos de layout.
pub(crate) fn render_dom(ui: &mut egui::Ui, dom: &crate::dom::Dom) {
    let root = dom.node(dom.root);
    let mut index = 0usize;
    for &child in &root.children {
        render_block(ui, dom, child, &mut index);
    }
}

/// Renderiza um nó em contexto de BLOCO. `index` é a posição entre irmãos de
/// bloco (usada para numerar itens de lista com `PREFIX_NUMBER`).
fn render_block(
    ui: &mut egui::Ui,
    dom: &crate::dom::Dom,
    id: crate::dom::NodeIdx,
    index: &mut usize,
) {
    use crate::dom::NodeKind;
    // `tag` fica emprestado da arena (sem `.clone()`): só é lido por `block::lookup`
    // logo abaixo e não sobrevive a este escopo. O `dom` é read-only no render.
    let tag = match &dom.node(id).kind {
        NodeKind::Element { tag } => tag.as_str(),
        // Texto solto / não-elemento no nível de bloco: emite inline direto.
        _ => return render_inline(ui, dom, id, InlineStyle::default()),
    };

    // Tag sem layout de bloco registrado ⇒ inline transparente (default seguro,
    // igual a uma tag desconhecida): preserva o texto dos filhos.
    let Some(def) = crate::block::lookup(tag) else {
        return render_inline(ui, dom, id, InlineStyle::default());
    };

    let this_index = *index;
    *index += 1;

    // Heading: texto concatenado; `indent` é reusado como TAMANHO de fonte.
    if def.has(crate::block::FLAG_HEADING) {
        let text = collect_text(dom, id);
        // `style="..."` do heading sobrepõe tamanho/cor; senão usa o default do
        // nível (indent reusado como tamanho).
        let css = dom
            .node(id)
            .attr("style")
            .map(crate::style::parse_inline)
            .unwrap_or_default();
        let size = css.font_size.unwrap_or(if def.indent > 0.0 { def.indent } else { 20.0 });
        let mut rt = egui::RichText::new(text).strong().size(size);
        if let Some(c) = css.color {
            rt = rt.color(rgba_to_color32(c));
        }
        ui.heading(rt);
        return;
    }

    // Recuo à esquerda (lista/blockquote) via `ui.indent`; senão renderiza direto.
    if def.indent > 0.0 {
        ui.indent(("blk", id), |ui| render_block_body(ui, dom, id, def, this_index));
    } else {
        render_block_body(ui, dom, id, def, this_index);
    }
}

/// Corpo de um bloco (já dentro do recuo): aplica o eixo (`display`) + o
/// marcador (`prefix`) e desce nos filhos.
fn render_block_body(
    ui: &mut egui::Ui,
    dom: &crate::dom::Dom,
    id: crate::dom::NodeIdx,
    def: crate::block::BlockDef,
    this_index: usize,
) {
    use crate::block::*;

    let prefix = match def.prefix {
        x if x == PREFIX_BULLET => Some("•  ".to_string()),
        x if x == PREFIX_NUMBER => Some(format!("{}.  ", this_index + 1)),
        _ => None,
    };
    let mono = def.has(FLAG_MONO);

    match def.display {
        // GRID: cada filho-elemento é uma linha; os netos são as células.
        x if x == DISPLAY_GRID => {
            egui::Grid::new(("grid", id)).striped(true).show(ui, |ui| {
                for &row in &dom.node(id).children {
                    if !matches!(dom.node(row).kind, crate::dom::NodeKind::Element { .. }) {
                        continue; // ignora texto solto entre linhas
                    }
                    for &cell in &dom.node(row).children {
                        render_block(ui, dom, cell, &mut 0);
                    }
                    ui.end_row();
                }
            });
        }
        // HORIZONTAL: filhos lado a lado, sem quebra (linha de tabela / flex-row).
        x if x == DISPLAY_HORIZONTAL => {
            ui.horizontal(|ui| {
                let mut i = 0usize;
                for &child in &dom.node(id).children {
                    render_block(ui, dom, child, &mut i);
                }
            });
        }
        // WRAP: flui inline (CSS inline-flow) — o parágrafo clássico.
        x if x == DISPLAY_WRAP => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if let Some(p) = &prefix {
                    ui.label(egui::RichText::new(p).strong());
                }
                // Seed do estilo inline: mono do bloco + o `style="..."` do PRÓPRIO
                // bloco (ex. `<p style="color:red">` tinge todo o texto interno).
                let st = merge_node_style(dom, id, InlineStyle { mono, ..Default::default() });
                for &child in &dom.node(id).children {
                    render_inline(ui, dom, child, st);
                }
            });
        }
        // VERTICAL (default block): empilha os filhos.
        _ => {
            ui.vertical(|ui| {
                if let Some(p) = &prefix {
                    ui.label(egui::RichText::new(p).strong());
                }
                let mut i = 0usize;
                for &child in &dom.node(id).children {
                    render_block(ui, dom, child, &mut i);
                }
            });
        }
    }
}

/// Renderiza um nó em contexto INLINE, herdando `style`. As tags inline e seu
/// estilo vêm do mapa `block::lookup_inline` (definido pelo TS) — o Rust não
/// nomeia nenhuma tag. Tag inline ausente do mapa é transparente (sem estilo).
fn render_inline(
    ui: &mut egui::Ui,
    dom: &crate::dom::Dom,
    id: crate::dom::NodeIdx,
    style: InlineStyle,
) {
    use crate::dom::NodeKind;
    match &dom.node(id).kind {
        NodeKind::Text(text) => {
            let mut rt = egui::RichText::new(text);
            if style.bold {
                rt = rt.strong();
            }
            if style.italic {
                rt = rt.italics();
            }
            if style.mono {
                rt = rt.monospace();
            }
            if let Some(sz) = style.size {
                rt = rt.size(sz);
            }
            if let Some(c) = style.color {
                rt = rt.color(c);
            }
            ui.label(rt);
        }
        NodeKind::Element { tag } => {
            // Liga os bits de estilo registrados para esta tag inline, depois
            // sobrepõe o CSS do `style="..."` deste nó, e desce nos filhos.
            let flags = crate::block::lookup_inline(tag);
            let mut st = style;
            st.bold |= flags & crate::block::FLAG_BOLD != 0;
            st.italic |= flags & crate::block::FLAG_ITALIC != 0;
            st.mono |= flags & crate::block::FLAG_MONO != 0;
            st = merge_node_style(dom, id, st);
            for &child in &dom.node(id).children {
                render_inline(ui, dom, child, st);
            }
        }
        NodeKind::Document => {}
    }
}

/// Concatena o texto de todos os descendentes de `id` (em ordem de documento).
fn collect_text(dom: &crate::dom::Dom, id: crate::dom::NodeIdx) -> String {
    use crate::dom::NodeKind;
    let mut out = String::new();
    collect_text_into(dom, id, &mut out);
    return out;

    fn collect_text_into(dom: &crate::dom::Dom, id: crate::dom::NodeIdx, out: &mut String) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => out.push_str(t),
            _ => {
                for &child in &dom.node(id).children {
                    collect_text_into(dom, child, out);
                }
            }
        }
    }
}
