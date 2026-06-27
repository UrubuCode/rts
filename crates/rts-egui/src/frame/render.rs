//! Render do DOM RETIDO sobre um `egui::Ui`. "Render em cima da árvore": a fonte
//! da verdade é a hierarquia de nós, e o COMO de cada tag vem do mapa
//! `block::lookup`/`lookup_inline` (definido pelo TS), não de nomes hardcodados.
//! O Rust só aplica primitivos de layout.

/// Tamanho de fonte default (pontos) quando a tag não especifica — base de `em`
/// (font deste nó) e `rem` (font da raiz, até a cascade de `:root` existir).
/// Casa com o default de texto usado no `render_block_body`.
const DEFAULT_FONT_SIZE: f32 = 20.0;

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

/// Aplica um `ComputedStyle` (cor/bg/tamanho/peso) SOBRE um `InlineStyle`:
/// propriedade `Some` sobrescreve, `None` mantém a herdada. (`bg` ainda não é
/// usado no inline — chega no box model, F2.)
fn apply_computed(st: &mut InlineStyle, css: &crate::style::ComputedStyle) {
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

/// Mescla o estilo de um nó SOBRE um `InlineStyle` herdado, na ordem de
/// precedência CSS: herdado < estilo-de-TAG (`defineStyle`, F1) < `style=""`
/// inline do nó. Propriedade ausente mantém a anterior; presente sobrescreve.
fn merge_node_style(dom: &crate::dom::Dom, id: crate::dom::NodeIdx, mut st: InlineStyle) -> InlineStyle {
    // 1) estilo registrado para a TAG (slot opaco via defineStyle).
    if let crate::dom::NodeKind::Element { tag } = &dom.node(id).kind {
        if let Some(tag_css) = crate::style::lookup_style(tag) {
            apply_computed(&mut st, &tag_css);
        }
    }
    // 2) `style="..."` inline do nó sobrepõe o estilo da tag.
    if let Some(s) = dom.node(id).attr("style") {
        let css = crate::style::parse_inline(s);
        apply_computed(&mut st, &css);
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
        // Precedência: estilo-de-TAG (defineStyle) < `style=""` inline. O tamanho
        // cai para o default do nível (indent) se nenhum dos dois o definir.
        let mut css = crate::style::lookup_style(tag).unwrap_or_default();
        if let Some(s) = dom.node(id).attr("style") {
            let inline = crate::style::parse_inline(s);
            if inline.color.is_some() {
                css.color = inline.color;
            }
            if inline.font_size.is_some() {
                css.font_size = inline.font_size;
            }
        }
        let size = css.font_size.unwrap_or(if def.indent > 0.0 { def.indent } else { 20.0 });
        let mut rt = egui::RichText::new(text).strong().size(size);
        if let Some(c) = css.color {
            rt = rt.color(rgba_to_color32(c));
        }
        ui.heading(rt);
        return;
    }

    // Box model (F2): se a tag tem caixa (bg/padding/margin/border/raio), envolve
    // o corpo num `egui::Frame`; senão renderiza direto (sem overhead). O estilo de
    // TAG e o `style=""` inline já combinados aqui (mesma precedência do texto).
    let mut box_css = crate::style::lookup_style(tag).unwrap_or_default();
    if let Some(s) = dom.node(id).attr("style") {
        let inline = crate::style::parse_inline(s);
        merge_box_props(&mut box_css, &inline);
    }

    let width = box_css.width;
    let font_size = box_css.font_size.unwrap_or(DEFAULT_FONT_SIZE);
    let body = |ui: &mut egui::Ui| {
        // `width` (F2): resolução TARDE (north-star risco 5). Cada unidade resolve
        // contra seu eixo, conhecido só AQUI: `%` = available_width (content-box do
        // pai, egui já descontou o inner/outer margin); `vw`/`vh` = viewport;
        // `em`/`rem` = font-size. `Auto`/`None` não toca (egui usa a disponível).
        // Aplica via `set_max_width` (encolhe sem quebrar o layout do pai).
        if let Some(d) = width {
            let screen = ui.ctx().screen_rect();
            let ctx = crate::style::ResolveCtx {
                parent_content_w: ui.available_width(),
                node_font_size: font_size,
                root_font_size: DEFAULT_FONT_SIZE, // rem ancora no default até cascade de :root
                viewport_w: screen.width(),
                viewport_h: screen.height(),
            };
            if let Some(w) = d.resolve(&ctx) {
                ui.set_max_width(w);
            }
        }
        // Recuo à esquerda (lista/blockquote) via `ui.indent`; senão direto.
        if def.indent > 0.0 {
            ui.indent(("blk", id), |ui| render_block_body(ui, dom, id, def, this_index));
        } else {
            render_block_body(ui, dom, id, def, this_index);
        }
    };

    if box_css.has_box() {
        block_frame(&box_css).show(ui, body);
    } else {
        body(ui);
    }
}

/// Mescla SÓ as propriedades de caixa de `src` sobre `dst` (`Some` sobrescreve).
/// Usado para o `style=""` inline sobrepor o estilo-de-tag na caixa do bloco.
fn merge_box_props(dst: &mut crate::style::ComputedStyle, src: &crate::style::ComputedStyle) {
    if src.bg.is_some() {
        dst.bg = src.bg;
    }
    if src.padding.is_some() {
        dst.padding = src.padding;
    }
    if src.margin.is_some() {
        dst.margin = src.margin;
    }
    if src.border_width.is_some() {
        dst.border_width = src.border_width;
    }
    if src.border_color.is_some() {
        dst.border_color = src.border_color;
    }
    if src.corner_radius.is_some() {
        dst.corner_radius = src.corner_radius;
    }
    if src.width.is_some() {
        dst.width = src.width;
    }
}

/// Monta um `egui::Frame` a partir do `ComputedStyle` de caixa. Mapeia padding→
/// inner_margin, margin→outer_margin, bg→fill, border→stroke, raio→corner_radius.
/// LIMITE DE PRODUTO (F2): `egui::Frame` NÃO é o box model do CSS — sem
/// margin-collapse, sem `box-sizing`, sem padding/margin por-lado (um valor para
/// os 4 lados). É o "card" pragmático, não conformidade CSS.
fn block_frame(css: &crate::style::ComputedStyle) -> egui::Frame {
    let mut frame = egui::Frame::new();
    if let Some(p) = css.padding {
        frame = frame.inner_margin(p);
    }
    if let Some(m) = css.margin {
        frame = frame.outer_margin(m);
    }
    if let Some(bg) = css.bg {
        frame = frame.fill(rgba_to_color32(bg));
    }
    if let Some(w) = css.border_width {
        let color = css.border_color.map(rgba_to_color32).unwrap_or(egui::Color32::GRAY);
        frame = frame.stroke(egui::Stroke::new(w, color));
    }
    if let Some(r) = css.corner_radius {
        frame = frame.corner_radius(r);
    }
    frame
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
        // Comentário (`<!-- -->`) está na árvore (DOM fiel) mas NÃO pinta.
        NodeKind::Comment(_) | NodeKind::Document => {}
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
