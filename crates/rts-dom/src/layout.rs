//! Motor de LAYOUT — calcula a geometria (x, y, largura, altura) de cada nó e
//! emite uma DISPLAY LIST plana que o backend de render só PINTA. EGUI-FREE.
//!
//! Esta é a virada arquitetural decidida em 2026-06-27 ("processar tudo no DOM e
//! o egui só lê e exibe"): o `rts-dom` deixa de só guardar a árvore/estilo e passa
//! a CALCULAR onde cada caixa fica, seguindo a lógica do CSS (fluxo normal, box
//! model content-box). O `rts-egui` (ou qualquer backend futuro: web/png/canvas)
//! recebe a [`DisplayList`] pronta — uma lista de "pinte retângulo/texto em
//! (x,y,w,h)" — e só desenha. **O backend nunca decide layout.**
//!
//! ## Modelo (fluxo normal block, fase 1)
//!
//! - **Block empilha vertical**, cada caixa ocupando a largura do container por
//!   padrão (MDN CSS Flow Layout). `width` explícito (px/%) encolhe; `%` resolve
//!   contra o content-box do PAI (containing block), TARDE, aqui no layout.
//! - **Box model content-box** (MDN): `outer_w = margin + border + padding +
//!   content_w`. O `width` do CSS é a largura do CONTENT; padding/border/margin
//!   somam por fora.
//! - **Texto** é medido por um [`TextMeasurer`] (a largura/altura do glifo é o
//!   único dado que o `rts-dom` não tem sozinho — o backend mede; ver o trait).
//!   Fase 1 usa uma medida aproximada ([`ApproxMeasurer`]); o egui pluga a real.
//!
//! Cortes da fase 1 (aditivos depois): inline-flow rico multi-run, margin-collapse
//! pai-filho, `display:grid`, float/position. O objetivo da fatia é provar a
//! TUBULAÇÃO DOM→layout→display-list→paint com box model block.
//!
//! ## Flexbox (gap/justify-content/align-items) — cortes CONSCIENTES
//!
//! Implementado: `display:flex` (row) + `flex-wrap`, `gap`/`row-gap`/`column-gap`,
//! `justify-content` (todas as formas, fiel à CSS Box Alignment L3 incl. fallback
//! de overflow), `align-items` (flex-start/center/flex-end). Cortes documentados:
//! - **`align-items:stretch` NÃO estica de fato** — trata como flex-start (cada
//!   item mantém sua altura natural). Stretch é o DEFAULT do flex, então um card
//!   sem `align-items` explícito não preenche a altura da linha (o browser
//!   esticaria). Esticar real exige passar altura imposta ao `layout_block`
//!   (fase futura — ver `align_offset`).
//! - **`flex-direction` só Row** — `column`/`row-reverse`/`column-reverse` são
//!   parseados e guardados (cascade pronta) mas o layout SEMPRE dispõe em row. Uma
//!   fatia futura generaliza `layout_children_horizontal` por eixo (`column` =
//!   main vertical, justify no Y). `flex-grow`/`shrink`/`basis` também fora.

use crate::dom::{Dom, IntrinsicWidthKey, LayoutMeasureKey, NodeIdx, NodeKind};
use crate::inline_box::AtomicKind;
use crate::style::{ComputedStyle, ResolveCtx};

/// Um retângulo em coordenadas de conteúdo (a origem é o canto da área de render;
/// o backend soma seu próprio offset de tela ao pintar). Unidade: pontos (f32).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
}

/// UM item da display list — uma instrução de pintura ATÔMICA e já posicionada. O
/// backend percorre a lista em ordem (a ordem É o z-order: o que vem depois pinta
/// por cima) e desenha cada item, sem nenhuma decisão de layout. Egui-free: cor é
/// `u32` RGBA, posição é `f32` — nenhum tipo de backend.
#[derive(Clone, PartialEq, Debug)]
pub enum DisplayItem {
    /// Retângulo preenchido (fundo de uma caixa). `radius` arredonda os cantos.
    SolidRect { rect: Rect, color: u32, radius: f32 },
    /// SOMBRA de caixa (`box-shadow`): pintada ATRÁS da caixa. `dx`/`dy` deslocam,
    /// `blur` amacia a borda, `spread` cresce/encolhe o rect, `color` é a cor (com
    /// alpha). O backend usa o blur real do egui (`epaint::Shadow`).
    Shadow { rect: Rect, dx: f32, dy: f32, blur: f32, spread: f32, color: u32, radius: f32 },
    /// Retângulo com GRADIENTE LINEAR (`background: linear-gradient(...)`). Interpola
    /// `c0`→`c1` ao longo do ângulo `angle_deg` (0=para cima, 90=para a direita, como
    /// o CSS). O backend pinta como mesh de 4 vértices coloridos. `radius` arredonda
    /// (aproximado — o mesh não recorta os cantos; suficiente p/ heros/botões).
    GradientRect { rect: Rect, c0: u32, c1: u32, angle_deg: f32, radius: f32 },
    /// Borda (contorno) de uma caixa, espessura `width`, na cor dada.
    Border { rect: Rect, width: f32, color: u32, radius: f32 },
    /// IMAGEM (`<img>` / background-image) — um bitmap RGBA8 já decodificado. O
    /// `pixels_handle` é um Buffer no HandleTable com `img_w*img_h*4` bytes RGBA
    /// (a partir do offset `pixels_off`); o backend sobe como textura e pinta no
    /// `rect` (escalando). Decodificação/download acontecem ANTES (no browser .ts,
    /// via fetchBytes+imgdec); o rts-dom só carrega o handle+dims — segue wasm-safe.
    Image { rect: Rect, pixels_handle: u64, pixels_off: u32, img_w: u32, img_h: u32 },
    /// PIXELS que o próprio documento carrega (um `<canvas>` que o programa
    /// pintou), RGBA8, `w*h*4` bytes.
    ///
    /// Variante separada da `Image` porque a fonte é outra: aquela aponta para
    /// um `Buffer` de fora por handle — o `<img>` que o mini-browser baixou e
    /// decodificou — e esta CARREGA os bytes, porque quem pintou foi o programa
    /// e o desenho não tem outro dono. Um `Rc` para que passar a lista adiante
    /// não copie a imagem.
    Pixels { rect: Rect, data: std::rc::Rc<Vec<u8>>, w: u32, h: u32 },
    /// Texto numa posição (canto superior-esquerdo). `mono` escolhe a família
    /// monoespaçada. `letter_spacing` = espaço extra entre glifos (px). `decoration`
    /// = linha decorativa (0=nenhuma, 1=underline, 2=line-through, 3=overline). O
    /// backend resolve a fonte/atlas; aqui só o necessário.
    Text {
        x: f32,
        y: f32,
        /// `Rc<str>` e não `String`: um item de texto é CLONADO toda vez que um
        /// fragmento de layout é reusado, e clonar a string por item era o custo
        /// dominante do reuso. Compartilhar o buffer torna o clone um
        /// incremento. O backend só lê.
        text: std::rc::Rc<str>,
        color: u32,
        size: f32,
        mono: bool,
        bold: bool,
        letter_spacing: f32,
        decoration: u8,
    },
    /// Começa a RECORTAR a um retângulo (scroll container interno): os itens
    /// seguintes, até o `EndClip`, só pintam DENTRO deste rect E são transladados por
    /// `(offset_x, offset_y)` (o quanto a região rolou). O backend aplica o clip
    /// (egui: `painter.with_clip_rect`) e soma o offset. `node` liga ao `ScrollRegion`
    /// (o backend injeta o offset aqui antes de pintar). Empilha — pode aninhar.
    BeginClip {
        rect: Rect,
        node: NodeIdx,
        offset_x: f32,
        offset_y: f32,
        /// Quantos fragmentos-filhos JÁ existiam na lista quando este clip foi
        /// aberto. Os de índice menor foram desenhados antes de o clip existir e
        /// portanto estão FORA dele, por muito que o `at` deles diga o
        /// contrário: inserir o marcador empurra o `at` de quem vinha depois, e
        /// o que era "antes do primeiro item" passa a cair dentro do clip.
        filhos_antes: usize,
    },
    /// Fecha o clip mais recente, restaurando o anterior.
    /// Fecha o clip aberto pelo `BeginClip` correspondente.
    ///
    /// Carrega QUANTOS fragmentos-filhos existiam quando foi emitido, e sem esse
    /// número o clip vaza. A saída é uma ÁRVORE: um filho entra por um índice
    /// (`ChildRef::at`, "antes do item nesta posição"), e vários filhos podem
    /// partilhar o mesmo índice — o do `EndClip` inclusive. Os que já existiam
    /// estão DENTRO do clip; os que os irmãos seguintes acrescentam no mesmo
    /// índice estão FORA, e o percurso não tinha como distinguir uns dos outros.
    ///
    /// O sintoma era uma página inteira em branco: a folha do MediaWiki tem a
    /// regra de acessibilidade `width:1px;height:1px;overflow:hidden`, e 30 325
    /// dos 30 528 itens da Wikipédia acabavam recortados a esse pixel.
    EndClip { filhos_dentro: usize },
}

/// Um CONTAINER ROLÁVEL interno (uma `<div>` com `overflow:auto/scroll` e tamanho
/// definido): o conteúdo é maior que a caixa, então o backend recorta no `visible`,
/// rola por um offset próprio e mostra barra(s) dentro dela. Produzido pelo layout,
/// consumido pelo backend. Distinto do scroll da PÁGINA (que é a viewport inteira).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollRegion {
    /// Qual nó é o container (chave do offset por-região no backend).
    pub node_idx: NodeIdx,
    /// Rect VISÍVEL (content-box do container, coords de conteúdo da página).
    pub visible: Rect,
    /// Largura REAL do conteúdo (pode exceder `visible.w` → rola em X).
    pub content_w: f32,
    /// Altura REAL do conteúdo (pode exceder `visible.h` → rola em Y).
    pub content_h: f32,
    /// overflow de cada eixo (auto/scroll rolam; hidden corta; visible não recorta).
    pub overflow_x: crate::scrollbar::Overflow,
    pub overflow_y: crate::scrollbar::Overflow,
}

/// A saída do layout: a lista plana de itens de pintura, em z-order. É o ÚNICO
/// que o backend de render consome. Sem nenhuma referência à árvore — o layout já
/// consumiu a topologia (herança/cascade/box model) ao produzir esta lista.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
    /// Subárvores emitidas por REFERÊNCIA, com a posição no meio dos itens
    /// próprios e o deslocamento a aplicar.
    ///
    /// É o que torna a saída uma ÁRVORE em vez de uma lista: um frame que mexe
    /// numa folha não reconstrói os 30 000 itens da página, aponta para os
    /// fragmentos que já existiam. Quem pinta anda a árvore ([`iter`]); quem
    /// precisa mutar ou comparar achata ([`materialize`]).
    pub children: Vec<ChildRef>,
    /// A cor do CANVAS — o fundo do `<body>`/`<html>` propagado, e BRANCO
    /// quando nenhum dos dois define um. Vive aqui e não como item da lista
    /// porque é a cor de LIMPEZA do backend; sem ela o que aparecia por trás de
    /// uma página era a cor padrão dele (quase preta), e uma página real cujo
    /// estilo mora num `<link>` externo saía texto preto sobre preto.
    pub canvas_background: u32,
    /// Altura total ocupada pelo conteúdo (para o backend dimensionar o scroll).
    pub content_height: f32,
    /// Geometria por NÓ (border-box, em coordenadas de conteúdo) — a base do
    /// `element.getBoundingClientRect()`/`offsetWidth`/etc. Preenchido durante o
    /// layout: cada bloco registra seu retângulo (margin EXCLUÍDA — border-box, como
    /// o `getBoundingClientRect` do browser); elementos inline recebem a união dos
    /// fragmentos de linha; nós de texto não entram.
    pub node_rects: crate::fasthash::FastMap<NodeIdx, Rect>,
    /// Containers roláveis internos (divs com `overflow`) — o backend gerencia o
    /// offset de cada região e recorta. Vazio quando a página não tem scroll interno.
    pub scroll_regions: Vec<ScrollRegion>,
    /// Nós em ordem de pintura para hit-test: ancestrais antes de descendentes,
    /// irmãos na ordem documental e elementos fora do fluxo por `z-index` crescente.
    /// O último nó que contém o ponto é o que está visualmente no topo.
    pub hit_order: Vec<NodeIdx>,
    /// A geometria completa, montada sob demanda a partir da árvore. Não entra
    /// no `PartialEq` nem no `Clone` lógico: é derivada.
    geometry_cache: std::cell::RefCell<Option<std::rc::Rc<Geometry>>>,
}

/// A geometria de uma passada de layout, já com as subárvores reusadas somadas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub rects: crate::fasthash::FastMap<NodeIdx, Rect>,
    pub hit_order: Vec<NodeIdx>,
    pub scroll_regions: Vec<ScrollRegion>,
}

/// Acumula a geometria de um fragmento e das subárvores dele, deslocada.
fn collect_geometry(fragment: &Fragment, dx: f32, dy: f32, out: &mut Geometry) {
    let moved = dx != 0.0 || dy != 0.0;
    for (idx, rect) in fragment.rects.iter() {
        let mut rect = *rect;
        if moved {
            rect.x += dx;
            rect.y += dy;
        }
        out.rects.insert(*idx, rect);
    }
    let mut next = 0usize;
    for child in &fragment.children {
        while next < child.hit_at && next < fragment.hit_order.len() {
            out.hit_order.push(fragment.hit_order[next]);
            next += 1;
        }
        collect_geometry(&child.fragment, dx + child.dx, dy + child.dy, out);
    }
    out.hit_order.extend_from_slice(&fragment.hit_order[next.min(fragment.hit_order.len())..]);
    for region in fragment.scroll_regions.iter() {
        let mut region = *region;
        if moved {
            region.visible.x += dx;
            region.visible.y += dy;
        }
        out.scroll_regions.push(region);
    }
}

impl DisplayList {
    /// Todos os itens a pintar, em z-order, cada um com o deslocamento a somar.
    ///
    /// Anda a ÁRVORE de fragmentos: um item de uma subárvore reusada sai daqui
    /// sem nunca ter sido copiado. Quem pinta já somava uma origem, então somar
    /// mais um deslocamento é grátis — foi o que permitiu a saída deixar de ser
    /// uma lista plana refeita por frame.
    pub fn walk(&self, mut f: impl FnMut(&DisplayItem, f32, f32)) {
        walk_items(&self.items, &self.children, 0.0, 0.0, &mut f);
    }

    /// A lista PLANA. Para quem precisa MUTAR itens (o `transform` do CSS, o
    /// offset de scroll no `BeginClip`) ou comparar duas listas.
    pub fn materialized(&self) -> Vec<DisplayItem> {
        let mut out = Vec::with_capacity(self.total_items());
        self.walk(|item, dx, dy| {
            let mut item = item.clone();
            if dx != 0.0 || dy != 0.0 {
                translate_item(&mut item, dx, dy);
            }
            out.push(item);
        });
        out
    }

    /// Achata esta lista em itens próprios, esquecendo a árvore.
    pub fn materialize(&mut self) {
        if self.children.is_empty() {
            return;
        }
        self.items = self.materialized();
        self.children.clear();
    }

    /// Quantos itens esta lista pinta ao todo.
    pub fn total_items(&self) -> usize {
        self.items.len()
            + self.children.iter().map(|c| c.fragment.total_items()).sum::<usize>()
    }

    /// A geometria COMPLETA desta lista: os retângulos próprios mais os das
    /// subárvores reusadas, já deslocados. Construída na primeira consulta e
    /// guardada — o layout mesmo só a pede quando há elemento fora do fluxo.
    pub fn geometry(&self) -> std::rc::Rc<Geometry> {
        if let Some(g) = self.geometry_cache.borrow().as_ref() {
            return std::rc::Rc::clone(g);
        }
        let g = std::rc::Rc::new(self.geometry_now());
        *self.geometry_cache.borrow_mut() = Some(std::rc::Rc::clone(&g));
        g
    }

    /// A geometria SEM cachear — para uso DURANTE a montagem da lista, quando
    /// ainda vão entrar itens (a passada de fora do fluxo). Cachear ali deixaria
    /// o hit-test lendo uma geometria anterior aos `position:absolute`, que foi
    /// exatamente o que um teste de `z-index` acusou.
    pub fn geometry_now(&self) -> Geometry {
        let mut g = Geometry {
            rects: self.node_rects.clone(),
            hit_order: Vec::with_capacity(self.hit_order.len()),
            scroll_regions: self.scroll_regions.clone(),
        };
        // Intercala a ordem de hit-test pelo ponto de entrada de cada subárvore:
        // a ordem É o z-order, e concatenar inverteria quem está por cima.
        let mut next = 0usize;
        for child in &self.children {
            while next < child.hit_at && next < self.hit_order.len() {
                g.hit_order.push(self.hit_order[next]);
                next += 1;
            }
            collect_geometry(&child.fragment, child.dx, child.dy, &mut g);
        }
        g.hit_order.extend_from_slice(&self.hit_order[next.min(self.hit_order.len())..]);
        g
    }

    /// O retângulo de um nó, se ele foi desenhado.
    pub fn rect_of(&self, node: NodeIdx) -> Option<Rect> {
        self.geometry().rects.get(&node).copied()
    }

    /// HIT-TEST: o nó sob o ponto `(x, y)` em COORDENADAS DE CONTEÚDO (o backend
    /// converte tela→conteúdo somando o offset de scroll antes de chamar). Quando a
    /// lista foi produzida por `layout_document`, a ordem de pintura respeita
    /// ancestrais/descendentes, irmãos e `z-index`; o ÚLTIMO retângulo que contém o
    /// ponto é o elemento visualmente no topo. Listas antigas sem `hit_order` usam o
    /// fallback por menor área para manter compatibilidade.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeIdx> {
        let g = self.geometry();
        if !g.hit_order.is_empty() {
            return g.hit_order.iter().rev().copied().find(|&idx| {
                g.rects.get(&idx).is_some_and(|r| {
                    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
                })
            });
        }
        let mut best: Option<(NodeIdx, f32)> = None;
        for (&idx, r) in &g.rects {
            if x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h {
                let area = r.w * r.h;
                if best.map(|(_, a)| area < a).unwrap_or(true) {
                    best = Some((idx, area));
                }
            }
        }
        best.map(|(idx, _)| idx)
    }
}

/// Abstração de MEDIÇÃO de texto (largura/altura de uma string num tamanho/peso).
/// Vive aqui (no `rts-dom`) e é IMPLEMENTADA pelo backend (o egui mede via galley);
/// reimplementar largura de glifo no `rts-dom` é a armadilha que o roadmap alertou.
/// O layout depende SÓ deste trait — continua egui-free e testável com um mock.
pub trait TextMeasurer {
    /// Largura em pontos de `text` renderizado em `size` (mono ou proporcional,
    /// regular ou `bold`). O peso importa: a fonte bold é mais larga — medir regular
    /// e pintar bold faz o texto estourar a linha (quebra a mais).
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool) -> f32;
    /// Altura de UMA linha em `size` (line-height). Aproximação aceitável: `size *
    /// fator`; o backend pode dar o valor exato da fonte.
    fn line_height(&self, size: f32) -> f32;

    /// IDENTIDADE deste medidor: dois medidores com a mesma identidade têm de
    /// dar a mesma largura para o mesmo texto.
    ///
    /// Entra na chave de todo cache de layout, porque a mesma árvore no mesmo
    /// viewport se dispõe diferente com outra fonte. Era o ENDEREÇO do `dyn`
    /// que servia de identidade — e um medidor construído na pilha por frame
    /// (o do egui é) pode mudar de endereço sem mudar de comportamento, ou
    /// reusar o endereço de outro que mudou: as duas falhas em direções
    /// opostas. O default `0` serve a um medidor sem estado; um backend cujo
    /// resultado dependa de fonte/escala DEVE derivar disto o que muda.
    fn identity(&self) -> u64 {
        0
    }
}

/// Medidor APROXIMADO, sem backend — para teste e para o caminho headless puro
/// (gerar layout sem janela). Largura ≈ `n_chars * size * 0.5` (média de fonte
/// proporcional latina); altura ≈ `size * 1.3`. Não é exato (o egui dá o real),
/// mas é determinístico e suficiente para block-flow (onde a largura do texto não
/// decide a da caixa — a caixa ocupa o container).
pub struct ApproxMeasurer;

impl TextMeasurer for ApproxMeasurer {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool) -> f32 {
        let mut per = if mono { 0.6 } else { 0.5 };
        if bold {
            per *= 1.06; // bold ~6% mais largo.
        }
        text.chars().count() as f32 * size * per
    }
    fn line_height(&self, size: f32) -> f32 {
        // 1.125 e não 1.3, e o número é uma APROXIMAÇÃO calibrada, não uma lei:
        // `line-height: normal` sai das métricas da fonte (ascent + descent +
        // line gap) e este medidor não tem fonte nenhuma. 1.125 é o que o Chrome
        // computa para a fonte padrão a 16px (18px), medido pelo corpus de
        // fixtures — o 1.3 anterior dava 20.8 e aparecia como o desvio mais
        // repetido do corpus, 43 vezes.
        //
        // Um backend COM métricas não usa isto: o `rts-egui` responde
        // `row_height` da fonte real. Este valor serve o layout headless, onde a
        // alternativa era não ter resposta nenhuma.
        //
        // A constante e a medição que a calibrou vivem em `style::line_metrics`,
        // porque `normal` é o valor INICIAL de uma propriedade CSS e não uma
        // preferência do medidor — e porque lá está o arredondamento para cima
        // que faz 20px dar 23 e 30px dar 34, os inteiros que o Chrome reporta
        // (sem ele saíam 22,5 e 33,75).
        crate::style::normal_line_height(size)
    }
}

/// Tamanho de fonte default (pontos) quando o estilo não especifica — base de
/// `em`/`rem` e do texto sem `font-size`. **16px, o default de todo browser**
/// (era 20, o que inflava cada `em`/`rem` em 25% — `max-width:42em` dava 840 em
/// vez dos 672 do Chrome; validado número-a-número no cover).
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

/// O contexto de uma passada de layout: o viewport (para `vw`/`vh` e largura
/// inicial) e o medidor de texto. Imutável durante a passada.
pub struct LayoutCtx<'a> {
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub measurer: &'a dyn TextMeasurer,
}

/// Mede um bloco sem emitir pintura. É usado apenas pelos pré-passos de flex/grid/
/// inline-block e pelo posicionamento out-of-flow. O resultado depende das constraints
/// e do estilo vigente, mas não da posição absoluta; por isso o cache não guarda uma
/// DisplayList e a chamada final continua responsável por pintar tudo no z-order certo.
#[allow(clippy::too_many_arguments)]
pub(crate) fn measure_block(
    dom: &Dom,
    id: NodeIdx,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    shrink_to_fit: bool,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let measurer = ctx.measurer.identity();
    let key = LayoutMeasureKey {
        tree: dom.cache_identity(),
        node_epoch: dom.layout_epoch(id),
        style_epoch: crate::style::props::style_epoch(),
        node: id,
        avail_w: avail_w.to_bits(),
        avail_h: avail_h.map(f32::to_bits),
        forced_outer_w: forced_outer_w.map(f32::to_bits),
        forced_outer_h: forced_outer_h.map(f32::to_bits),
        shrink_to_fit,
        viewport_w: ctx.viewport_w.to_bits(),
        viewport_h: ctx.viewport_h.to_bits(),
        measurer,
    };
    crate::bump!(measure_calls);
    if let Some(size) = dom.layout_measure_get(key) {
        crate::bump!(measure_hits);
        return size;
    }
    let mut scratch = DisplayList::default();
    let size = layout_block(
        dom,
        id,
        0.0,
        0.0,
        avail_w,
        avail_h,
        forced_outer_w,
        forced_outer_h,
        shrink_to_fit,
        ctx,
        &mut scratch,
    );
    dom.layout_measure_put(key, size);
    size
}

/// O layout de um `Dom`, REUSADO enquanto nada que o afete mudar.
///
/// Um browser não recalcula layout quando nada mudou, e o caminho headless
/// (`rts:dom` a partir do TS) chamava [`layout_document`] por consulta de
/// geometria — uma passada completa por `getBoundingClientRect`. O `rts-egui`
/// já tinha um cache assim, por frame, dentro dele: dois caches para a mesma
/// pergunta, e só um dos consumidores servido.
///
/// A chave é `(revisão de render, viewport, medidor)`. A revisão cobre árvore,
/// estilo e animação (todo mutador a incrementa); o viewport porque o layout
/// depende dele; e o MEDIDOR porque a mesma árvore no mesmo viewport se dispõe
/// diferente com uma fonte diferente — é o mesmo componente que já entra nas
/// chaves dos caches de medição.
///
/// Devolve `Rc` e não valor: uma `DisplayList` de página grande são 15 000
/// itens e milhares de `String`, e clonar isso por consulta desfaria o ganho.
pub fn layout_cached(dom: &Dom, ctx: &LayoutCtx) -> std::rc::Rc<DisplayList> {
    let key = (
        dom.render_revision(),
        ctx.viewport_w.to_bits(),
        ctx.viewport_h.to_bits(),
        ctx.measurer.identity(),
    );
    if let Some(hit) = dom.display_cache_get(key) {
        crate::bump!(display_cache_hits);
        return hit;
    }
    let fresh = std::rc::Rc::new(layout_document(dom, ctx));
    dom.display_cache_put(key, &fresh);
    fresh
}

/// Calcula o layout de um `Dom` inteiro e devolve a [`DisplayList`]. Ponto de
/// entrada do motor: percorre os filhos de `#document` como blocos empilhados na
/// largura do viewport, resolvendo box model e emitindo os itens de pintura.
pub fn layout_document(dom: &Dom, ctx: &LayoutCtx) -> DisplayList {
    crate::bump!(documents);
    let _phase = crate::metrics::phases::scope("layout");
    // informa o viewport à CASCADE (base de vw/vh no font-size fluido/calc; o
    // memo de estilo do Dom invalida sozinho se mudou).
    dom.set_viewport(ctx.viewport_w, ctx.viewport_h);
    let mut list = DisplayList::default();
    // PROPAGAÇÃO DO FUNDO do <body>/<html> (regra especial do CSS): o background
    // desses dois elementos "vaza" para o VIEWPORT inteiro, não só a caixa deles.
    // Pintamos PRIMEIRO (atrás de tudo) um retângulo do tamanho do viewport com a cor
    // do body. (Reserva uma altura generosa; o egui faz clip na sua área.)
    //
    // E BRANCO quando nenhum dos dois define fundo: é o que um browser pinta no
    // canvas de uma página sem `background`. Sem isto o que aparecia era a cor
    // de limpeza do backend (quase preta), e uma página real cujo estilo mora
    // num `<link>` externo ficava texto preto sobre preto — o sintoma parecia
    // "a cascata falhou" quando a cascata estava certa e o canvas é que não
    // tinha dono.
    // Vai no CAMPO e não como item da lista: quem pinta o canvas é o backend
    // (é a cor de limpeza dele), e um item a mais deslocaria todos os índices
    // que os testes de layout usam para nomear o que estão a verificar.
    list.canvas_background = body_background(dom).unwrap_or(0xFFFF_FFFF);
    let mut cursor_y = 0.0f32;
    let root = dom.node(dom.root);
    for &child in &root.children {
        // position:absolute/fixed não participa do fluxo, inclusive quando é filho
        // direto do documento; será layoutado na passada final por z-index.
        if is_out_of_flow(dom, child) {
            continue;
        }
        // o containing block da raiz é a VIEWPORT: `height:100%` no <html> resolve
        // contra a altura da janela (base do `h-100` de páginas reais).
        let (_, h) =
            layout_block(dom, child, 0.0, cursor_y, ctx.viewport_w, Some(ctx.viewport_h), None, None, false, ctx, &mut list);
        cursor_y += h;
    }
    list.content_height = cursor_y;
    // ── PASSADA OUT-OF-FLOW: `position:absolute/fixed` saíram do fluxo (não
    // ocuparam espaço); pinta cada um contra o VIEWPORT com top/right/bottom/left,
    // por cima do fluxo (apêndice da lista = z maior; sem z-index real). V1: o
    // containing block é sempre a viewport (o de `absolute` — ancestral positioned
    // — e o "fica fixo ao rolar" do `fixed` são a v2).
    let mut out_of_flow = Vec::new();
    // Só varre se a página PODE ter algum: a varredura pede o estilo computado
    // de cada nó da árvore, e era 78% de um frame de mutação numa página que não
    // tem um único posicionado.
    if dom.may_have_out_of_flow() {
        collect_out_of_flow(dom, dom.root, &mut out_of_flow);
    }
    // Z-INDEX: ordena por z-index (menor pinta primeiro = fica atrás). Sort ESTÁVEL:
    // z-index igual (ou ambos auto=0) preserva a ordem do documento. Cobre o caso
    // comum (modais/dropdowns/overlays posicionados que se sobrepõem).
    out_of_flow.sort_by_key(|&id| {
        dom.computed_style_idx(id).and_then(|c| c.z_index).unwrap_or(0)
    });
    // O rect do containing block de cada abs é lido do `node_rects` JÁ preenchido
    // pelo fluxo normal (o ancestral positioned já foi pintado). Clona antes do
    // empréstimo mutável de `list`.
    // A geometria COMPLETA (com as subárvores reusadas): o containing block de
    // um `absolute` pode ser um ancestral cujo retângulo veio de um fragmento.
    let flow_rects = list.geometry_now().rects;
    crate::bump!(out_of_flow, out_of_flow.len());
    for id in &out_of_flow {
        layout_out_of_flow(dom, *id, ctx, &flow_rects, &mut list);
    }
    // A HashMap não carrega ordem de pintura. Materializamos uma ordem explícita
    // para o hit-test: fluxo normal em pré-ordem e, depois, posicionados em ordem
    // crescente de z-index (o último pintado fica no topo).
    // A ordem de pintura já foi registrada durante as inserções de retângulos:
    // fluxo normal durante a descida e out-of-flow na ordem de z-index acima.
    crate::bump!(display_items, list.total_items());
    // As marcas de sujeira são POR PASSADA: quem as consome é este layout, e
    // acumulá-las entre frames faria a lista de filhos sujos de um container
    // crescer até o teto — e aí a costura desistiria sempre.
    dom.clear_dirty();
    crate::bump!(node_rects, list.node_rects.len());
    crate::bump!(scroll_regions, list.scroll_regions.len());
    list
}

/// O rect do CONTAINING BLOCK de um `position:absolute` = o ancestral mais próximo
/// com `position != static` (relative/absolute/fixed), lido do `node_rects` do
/// fluxo. `None` = nenhum ancestral positioned → o containing block é a viewport
/// (a raiz inicial). Um `fixed` sempre usa a viewport (tratado no caller).
fn containing_block_rect(
    dom: &Dom,
    id: NodeIdx,
    flow_rects: &crate::fasthash::FastMap<NodeIdx, Rect>,
) -> Option<Rect> {
    let mut cur = dom.node(id).parent;
    while let Some(p) = cur {
        let positioned = dom
            .computed_style_idx(p)
            .and_then(|c| c.position)
            .map(|pos| pos != crate::style::Position::Static)
            .unwrap_or(false);
        if positioned {
            if let Some(r) = flow_rects.get(&p) {
                return Some(*r);
            }
            // Um ancestral posicionado SEM caixa (não foi layoutado) não serve de
            // containing block, e continuar a subir escolhe um contentor que o
            // browser nunca escolheria — foi assim que um elemento de um ramo
            // escondido se ancorou num contentor com a altura do documento.
            //
            // Devolver `None` faz o chamador cair na viewport, que é o
            // containing block inicial. É uma aproximação, e a alternativa
            // (reconstruir a caixa do ancestral) não tem caso desde que o ramo
            // escondido deixou de ser layoutado.
            return None;
        }
        cur = dom.node(p).parent;
    }
    None
}

/// DFS que coleta os nós `position:absolute/fixed`. Não desce DENTRO de um
/// out-of-flow (os filhos dele pertencem ao layout dele; abs-dentro-de-abs = v2).
fn collect_out_of_flow(dom: &Dom, id: NodeIdx, out: &mut Vec<NodeIdx>) {
    for &child in &dom.node(id).children {
        // `display:none` num ANCESTRAL remove a subárvore inteira do layout, e o
        // fora de fluxo não é exceção: um `position:absolute` dentro de um ramo
        // escondido não gera caixa nenhuma no browser.
        //
        // Sem isto ele era medido e pintado, e — por o pai escondido não ter
        // caixa — a procura do containing block saltava-o e ia parar a um
        // ancestral posicionado muito acima: na Wikipédia, um
        // `<input type=checkbox height:100%>` de um menu escondido resolvia
        // contra um contentor com a altura do DOCUMENTO e vinha com 96 665px.
        if e_display_none(dom, child) {
            continue;
        }
        if is_out_of_flow(dom, child) {
            out.push(child);
        } else {
            collect_out_of_flow(dom, child, out);
        }
    }
}

/// `true` se este nó declara `display:none` — a pergunta que tira uma subárvore
/// inteira do layout. Só o próprio nó: quem varre a árvore de cima para baixo já
/// não desce nele, e é isso que a torna hereditária na prática.
fn e_display_none(dom: &Dom, id: NodeIdx) -> bool {
    matches!(&dom.node(id).kind, NodeKind::Element { .. })
        && dom
            .computed_style_idx(id)
            .and_then(|c| c.effective_display())
            == Some(crate::style::DisplayKind::None)
}

/// Layouta UM nó fora do fluxo contra o viewport: mede shrink-to-fit e posiciona
/// pelos offsets (`left` OU `right`−largura; `top` OU `bottom`−altura; sem nenhum
/// dos dois no eixo → 0).
fn layout_out_of_flow(
    dom: &Dom,
    id: NodeIdx,
    ctx: &LayoutCtx,
    flow_rects: &crate::fasthash::FastMap<NodeIdx, Rect>,
    list: &mut DisplayList,
) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    // CONTAINING BLOCK: `absolute` posiciona contra o ancestral positioned mais
    // próximo (o Google ancora os ícones no canto direito da CAIXA DE BUSCA, não
    // da tela); `fixed` sempre contra a viewport. Sem ancestral positioned →
    // viewport. `cb` = (origem_x, origem_y, largura, altura) do container.
    let is_fixed = matches!(css.position, Some(crate::style::Position::Fixed));
    let cb = if is_fixed {
        Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h)
    } else {
        containing_block_rect(dom, id, flow_rects)
            .unwrap_or_else(|| Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h))
    };
    let resolve = ResolveCtx {
        parent_content_w: cb.w,
        node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // mede (w, h) numa lista descartável para resolver right/bottom.
    let (w, h) = measure_block(dom, id, cb.w, Some(cb.h), None, None, true, ctx);
    let left = resolve_inset(css.inset_left, cb.w, &resolve);
    let right = resolve_inset(css.inset_right, cb.w, &resolve);
    let top = resolve_inset(css.inset_top, cb.h, &resolve);
    let bottom = resolve_inset(css.inset_bottom, cb.h, &resolve);
    // Os offsets são RELATIVOS ao container: soma a origem do containing block.
    let x = match (left, right) {
        (Some(l), _) => cb.x + l,
        (None, Some(r)) => cb.x + cb.w - w - r,
        (None, None) => cb.x,
    };
    let y = match (top, bottom) {
        (Some(t), _) => cb.y + t,
        (None, Some(b)) => cb.y + cb.h - h - b,
        (None, None) => cb.y,
    };
    layout_block(dom, id, x, y, cb.w, Some(cb.h), None, None, true, ctx, list);
}

/// Resolve um offset de posicionamento (`top`/`left`/…): px SEM clamp (negativo
/// desloca para fora — badges/tooltips); `%` contra o eixo do viewport dado.
fn resolve_inset(d: Option<crate::style::Dimension>, axis: f32, ctx: &ResolveCtx) -> Option<f32> {
    match d? {
        crate::style::Dimension::Px(v) => Some(v),
        crate::style::Dimension::Percent(p) => Some(axis * p / 100.0),
        other => other.resolve(ctx),
    }
}

/// `true` se o nó SAI do fluxo (`position: absolute/fixed`) — não ocupa espaço
/// entre os irmãos; pintado na passada out-of-flow de [`layout_document`].
pub(crate) fn is_out_of_flow(dom: &Dom, id: NodeIdx) -> bool {
    matches!(&dom.node(id).kind, NodeKind::Element { .. })
        && dom
            .computed_style_idx(id)
            .and_then(|c| c.position)
            .map(|p| p.out_of_flow())
            .unwrap_or(false)
}

/// Emite os retângulos da SCROLLBAR (track + thumb) na DisplayList — a BARRA é
/// preparada pelo DOM, não pelo backend (o egui só pinta `SolidRect`, mantendo-se
/// burro e substituível). Dados de geometria: `viewport_w/h` (área visível),
/// `content_h` (altura total do conteúdo), `offset_y` (quanto já rolou). Estilo:
/// `sb` (cor/largura/radius do CSS). Só emite a barra VERTICAL (a horizontal segue
/// o mesmo molde quando precisar). Coordenadas em espaço de CONTEÚDO já rolado: a
/// barra é desenhada FIXA na viewport, então some o `offset_y` (o backend translada
/// o conteúdo por -offset; somar offset à barra a mantém na tela).
///
/// Não faz nada se o conteúdo cabe (sem overflow) e a barra não é forçada.
pub fn emit_scrollbar(
    list: &mut DisplayList,
    viewport_w: f32,
    viewport_h: f32,
    content_h: f32,
    offset_y: f32,
    sb: &crate::scrollbar::ScrollbarStyle,
    force: bool,
) {
    use crate::scrollbar::BarWidth;
    // precisa rolar? (conteúdo maior que a viewport) ou barra forçada (overflow:scroll).
    let overflow = content_h > viewport_h + 0.5;
    if !overflow && !force {
        return;
    }
    // largura da barra (px): thin=8, none=0 (não desenha), px direto, senão 12.
    let bar_w = match sb.width {
        Some(BarWidth::None) => return,
        Some(BarWidth::Thin) => 8.0,
        Some(BarWidth::Px(px)) => px,
        _ => 12.0,
    };
    // cores default fiéis a um browser escuro (sobrescritas pelo CSS).
    let track_color = sb.track.unwrap_or(0x1e1e1eff);
    let thumb_color = sb.thumb.unwrap_or(0x6b6b6bff);
    let radius = sb.thumb_radius.unwrap_or(bar_w / 2.0);
    let bar_x = viewport_w - bar_w;
    // o thumb: tamanho proporcional à fração visível; posição proporcional ao offset.
    let frac = (viewport_h / content_h).clamp(0.0, 1.0);
    let thumb_h = (viewport_h * frac).max(24.0); // mínimo p/ pegar com o mouse
    let max_off = (content_h - viewport_h).max(1.0);
    let scroll_frac = (offset_y / max_off).clamp(0.0, 1.0);
    let thumb_y = scroll_frac * (viewport_h - thumb_h);
    // FIXA na viewport: soma offset_y (o backend translada tudo por -offset).
    let vy = offset_y;
    // track (faixa direita inteira) — atrás do thumb.
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(bar_x, vy, bar_w, viewport_h),
        color: track_color,
        radius: 0.0,
    });
    // thumb (handle).
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(bar_x, vy + thumb_y, bar_w, thumb_h),
        color: thumb_color,
        radius,
    });
}

/// Emite as barras (x e/ou y) DENTRO de um scroll container interno (#1744), no rect
/// visível dele (coords de conteúdo da página). Diferente de `emit_scrollbar` (que é
/// a viewport): aqui as barras ficam nas bordas da DIV. Emitidas APÓS o `EndClip`
/// (fora do recorte), então não rolam — ficam fixas na div. `offset_*` é o quanto a
/// região rolou (posiciona o thumb).
pub fn emit_scrollbar_in(
    list: &mut DisplayList,
    region: &ScrollRegion,
    offset_x: f32,
    offset_y: f32,
    sb: &crate::scrollbar::ScrollbarStyle,
) {
    use crate::scrollbar::BarWidth;
    let bar_w = match sb.width {
        Some(BarWidth::None) => return,
        Some(BarWidth::Thin) => 8.0,
        Some(BarWidth::Px(px)) => px,
        _ => 12.0,
    };
    let track_color = sb.track.unwrap_or(0x1e1e1eff);
    let thumb_color = sb.thumb.unwrap_or(0x6b6b6bff);
    let radius = sb.thumb_radius.unwrap_or(bar_w / 2.0);
    let v = region.visible;
    let need_y = region.overflow_y.scrollable() && region.content_h > v.h + 0.5;
    let need_x = region.overflow_x.scrollable() && region.content_w > v.w + 0.5;
    // barra VERTICAL (borda direita da div).
    if need_y {
        let track_h = if need_x { v.h - bar_w } else { v.h };
        let frac = (track_h / region.content_h).clamp(0.0, 1.0);
        let thumb_h = (track_h * frac).max(24.0);
        let max_off = (region.content_h - v.h).max(1.0);
        let thumb_y = (offset_y / max_off).clamp(0.0, 1.0) * (track_h - thumb_h);
        let bx = v.x + v.w - bar_w;
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(bx, v.y, bar_w, track_h), color: track_color, radius: 0.0 });
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(bx, v.y + thumb_y, bar_w, thumb_h), color: thumb_color, radius });
    }
    // barra HORIZONTAL (borda inferior da div).
    if need_x {
        let track_w = if need_y { v.w - bar_w } else { v.w };
        let frac = (track_w / region.content_w).clamp(0.0, 1.0);
        let thumb_w = (track_w * frac).max(24.0);
        let max_off = (region.content_w - v.w).max(1.0);
        let thumb_x = (offset_x / max_off).clamp(0.0, 1.0) * (track_w - thumb_w);
        let by = v.y + v.h - bar_w;
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(v.x, by, track_w, bar_w), color: track_color, radius: 0.0 });
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(v.x + thumb_x, by, thumb_w, bar_w), color: thumb_color, radius });
    }
}

/// O `background` do `<body>` (ou, se ausente, do `<html>`) — a cor que o CSS
/// PROPAGA para o viewport inteiro. `None` se nenhum dos dois tem fundo.
fn body_background(dom: &Dom) -> Option<u32> {
    // procura body e html entre os descendentes da raiz.
    for &child in &dom.node(dom.root).children {
        if let Some(bg) = bg_of_tag(dom, child, "body") {
            return Some(bg);
        }
        if let Some(bg) = bg_of_tag(dom, child, "html") {
            // o html pode ter o body dentro; tenta o body primeiro.
            if let Some(body_bg) = find_body_bg(dom, child) {
                return Some(body_bg);
            }
            return Some(bg);
        }
    }
    None
}

/// O bg de `idx` se sua tag é `tag` e tem background computado.
fn bg_of_tag(dom: &Dom, idx: NodeIdx, tag: &str) -> Option<u32> {
    match &dom.node(idx).kind {
        NodeKind::Element { tag: t } if t == tag => dom.computed_style_idx(idx).and_then(|c| c.bg),
        _ => None,
    }
}

/// Procura um `<body>` com bg na subárvore de `idx` (ex: html>body).
fn find_body_bg(dom: &Dom, idx: NodeIdx) -> Option<u32> {
    for &child in &dom.node(idx).children {
        if let Some(bg) = bg_of_tag(dom, child, "body") {
            return Some(bg);
        }
    }
    None
}

/// O retângulo (border-box) de um nó, computando o layout do documento na largura
/// dada — a base de `element.getBoundingClientRect()`. `None` se o nó não é
/// renderável (texto/`display:none`/metadata não têm rect próprio).
/// Roda o layout inteiro (O(n)); para várias consultas no mesmo frame, reuse a
/// `DisplayList` de `layout_document` e leia `node_rects` direto.
pub fn bounding_rect(dom: &Dom, node: NodeIdx, ctx: &LayoutCtx) -> Option<Rect> {
    layout_document(dom, ctx).rect_of(node)
}

/// Faz o layout de UM nó-bloco a partir de `(x, y)`, com `avail_w` de largura
/// disponível (a do container). Emite os itens (fundo/borda/texto/filhos) na
/// `list` e devolve o TAMANHO EXTERNO `(outer_w, outer_h)` da caixa (incluindo
/// padding/border/margin) — o pai usa a altura (empilhamento vertical) ou a
/// largura (horizontal) para posicionar o irmão seguinte. Texto solto e nós inline
/// são desenhados como linhas dentro do content-box.
pub(crate) fn layout_block(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    avail_w: f32,
    // Altura do CONTENT do containing block, quando DEFINIDA (height explícito no
    // pai / viewport na raiz): a base de `height: %` — que resolve contra a ALTURA
    // do pai (antes resolvia errado contra a largura; `h-100` não funcionava).
    // `None` = pai com altura auto → `height: %` vira auto (fiel ao browser).
    avail_h: Option<f32>,
    // Largura OUTER IMPOSTA (com margem) — o flex resolveu grow/shrink e DITA o
    // main size do item; vence width/min-max/shrink-to-fit (v1: clamp min/max no
    // resolve flex fica como corte documentado). `None` = fluxo normal.
    forced_outer_w: Option<f32>,
    // Altura OUTER IMPOSTA (com margem) — o `align-items/self: stretch` do flex.
    // O caller só passa para item SEM height explícito. `None` = altura natural.
    forced_outer_h: Option<f32>,
    // `shrink_to_fit`: quando true, um bloco SEM `width` explícito dimensiona pela
    // largura do CONTEÚDO (como `inline-block`/item flex), não ocupa a largura
    // disponível. É o que faz badges num container horizontal não esticarem para a
    // linha toda. No fluxo vertical normal é false (block ocupa a largura — MDN).
    shrink_to_fit: bool,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    crate::bump!(block_calls);
    // Nós não-elemento no nível de bloco (texto solto, comentário): trata o texto
    // como uma linha; comentário não pinta.
    let css = match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // Metadata não-renderável (`<head>` e seu conteúdo, `<style>`,
            // `<script>`): pula a subárvore inteira — não pinta nada. Permite
            // carregar um HTML COMPLETO (com <head><title><meta>) e renderizar só
            // o que é visível (o <body> e seus filhos).
            if is_non_rendered_tag(tag) {
                return (0.0, 0.0);
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            // `display:none` — não renderiza nem ocupa espaço (some da árvore visual).
            if e_display_none(dom, id) {
                return (0.0, 0.0);
            }
            // `<input>`/`<textarea>` editável (mini-browser): void, sem filhos — o
            // "conteúdo" é o texto do value/placeholder + cursor. Caminho próprio,
            // fora do fluxo de bloco genérico (que desceria em filhos inexistentes).
            if is_text_input_tag(tag) {
                let itype = dom
                    .node(id)
                    .attr("type")
                    .map(|t| t.to_ascii_lowercase())
                    .unwrap_or_default();
                // `type=hidden`: invisível e sem espaço (o form legado do google
                // tem 5 — viravam caixas de texto fantasmas).
                if itype == "hidden" {
                    return (0.0, 0.0);
                }
                // `type=submit/button/reset`: BOTÃO — caixa cinza UA com o value
                // como rótulo (não editável). O suficiente p/ o "Pesquisa Google".
                if matches!(itype.as_str(), "submit" | "button" | "reset") {
                    return layout_button(dom, id, &css, x, y, ctx, list);
                }
                return layout_input(dom, id, &css, x, y, avail_w, avail_h, forced_outer_w, ctx, list);
            }
            // `<img>` com pixels decodificados: emite a imagem no rect (tamanho do CSS
            // width/height, senão o natural da imagem). Void — sem filhos.
            // `<canvas>`: elemento REPLACED cujo conteúdo é uma superfície de
            // pixels. A caixa vem dos atributos `width`/`height` (ou do CSS), e
            // o desenho aparece quando o programa pinta — antes disso a caixa
            // existe e fica vazia, que é o que o browser também faz.
            if tag == "canvas" {
                if let Some(r) = layout_canvas(dom, id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
            }
            if tag == "img" {
                if let Some(img) = layout_image(dom, id, &css, x, y, avail_w, ctx, list) {
                    return img;
                }
                // sem pixels ainda (não baixou/decodificou): ocupa 0 (não pinta nada).
            }
            // `<svg>` é um REPLACED element: não desenhamos o vetor, mas RESERVAMOS
            // a caixa (dimensões do CSS width/height, dos atributos, ou da razão do
            // `viewBox`) e pintamos um placeholder cinza — assim a estrutura da
            // página fica correta mesmo sem o SVG (logo/ícones do google ocupam o
            // espaço certo em vez de colapsar pra 0×0).
            if tag == "svg" {
                if let Some(r) = layout_svg_placeholder(dom, id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
            }
            css
        }
        NodeKind::Text(t) => {
            // Whitespace estrutural é preservado no DOM, mas não cria uma linha
            // visual quando chega sozinho ao fluxo de blocos/root. Em contexto
            // inline, ele é tratado por `wrap_runs` e continua separando palavras.
            if t.trim().is_empty() {
                return (0.0, 0.0);
            }
            let size = DEFAULT_FONT_SIZE;
            let lh = ctx.measurer.line_height(size);
            let tw = ctx.measurer.text_width(t, size, false, false);
            list.items.push(DisplayItem::Text {
                x,
                y,
                text: t.as_str().into(),
                color: 0x000000FF,
                size,
                mono: false,
                bold: false,
                letter_spacing: 0.0,
                decoration: 0,
            });
            return (tw, lh);
        }
        _ => return (0.0, 0.0), // Comment / Document aninhado: não pinta.
    };

    // ── Box model (content-box): resolve as bordas/espaços absolutos ─────────────
    // O contexto de RESOLUÇÃO tardia primeiro (margens/paddings agora aceitam
    // unidades relativas — `p-3` = 1rem do Bootstrap — e resolvem AQUI, como width).
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Margin/padding POR LADO (Edges). O `margin_v` (UA-stylesheet, só vertical) é
    // somado ao top/bottom. Margens são SIGNED (negativa puxa — gutters `.row`);
    // padding é clampado ≥ 0 (padding negativo não existe no CSS).
    let m = &css.margin;
    let p = &css.padding;
    let mut margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let mut margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    // margin_v (UA-stylesheet) só vale no lado que o AUTOR NÃO declarou — um
    // `margin-top: 0` explícito ANULA o default da UA naquele lado (era o brand
    // do cover descendo 16px apesar do `h3 { margin-top: 0 }` do Bootstrap).
    let margin_v_extra = css.margin_v.unwrap_or(0.0);
    let mv_top = if m.top == crate::style::Side::Unset { margin_v_extra } else { 0.0 };
    let mv_bottom = if m.bottom == crate::style::Side::Unset { margin_v_extra } else { 0.0 };
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0) + mv_top;
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0) + mv_bottom;
    let pad_left = p.left.resolve(&resolve).unwrap_or(0.0).max(0.0);
    // RECUO DA LISTA (UA-stylesheet): `<ul>`/`<ol>` trazem `padding-inline-start:
    // 40px` em todo o browser, e é esse recuo que aloja o marcador do `<li>`.
    // Entra como PADDING e não como uma variável à parte porque é o que ele é:
    // assim conta na caixa de borda, no `content_x` e na largura disponível dos
    // filhos sem que nenhum desses três sítios precise de saber que existem
    // listas. Um `padding-left` do autor anula-o — é a camada mais fraca da
    // cascade, e o `list-style:none;padding-left:0` de um menu tem de vencer.
    let pad_left = pad_left + ua_list_indent(dom, id, p);
    let pad_right = p.right.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_top = p.top.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_bottom = p.bottom.resolve(&resolve).unwrap_or(0.0).max(0.0);
    // BORDA POR LADO: as larguras USADAS (um lado com `border-style: none` vale
    // zero, por mais que declare largura — ver `style::borders::used_widths`).
    // Era um ESCALAR `css.border_width` aplicado aos quatro lados, e isso não é
    // uma simplificação: `border-bottom: 5px` alargava a caixa nos quatro lados
    // ou em nenhum. Medido no corpus, era o maior desvio de uma fixture só —
    // `claude-border-lados`, 15 de 82.
    let [border_top, border_right, border_bottom, border_left] =
        crate::style::borders::used_widths(&css);
    // Atalhos para o eixo (horizontal = left+right): a maioria do box model usa o
    // total por eixo. (`margin_h`/`padding_h` = soma do eixo horizontal.)
    let border_h = border_left + border_right;
    let border_v = border_top + border_bottom;
    let margin_h = margin_left + margin_right;
    let padding_h = pad_left + pad_right;
    // `frame` horizontal = o que cerca o content no eixo X (margin+border+padding
    // dos DOIS lados); cada termo já é a soma do seu eixo.
    let frame = margin_h + border_h + padding_h;
    let font_for_content = font_px(&css, DEFAULT_FONT_SIZE);
    let border_box = css.border_box.unwrap_or(false);
    // O PAPEL da caixa (item de lista, parte de tabela) — decidido já aqui e não
    // junto do eixo dos filhos porque a `<table>` muda a resolução da SUA PRÓPRIA
    // largura, três linhas abaixo.
    let used = used_display(dom, id);
    // Uma `<table>` sem `width` é SHRINK-TO-FIT: encolhe ao conteúdo em vez de
    // ocupar o pai. É a diferença mais visível entre uma tabela e um `<div>`, e
    // sem ela cada tabela da página nasce com a largura da coluna inteira.
    let shrink_to_fit = shrink_to_fit || used == Some(crate::style::DisplayKind::Table);
    let content_w = if let Some(fw) = forced_outer_w {
        // main size do FLEX (grow/shrink já resolvidos): outer imposto → content =
        // outer - frame (o frame já soma margem+borda+padding dos dois lados).
        // Vence width/min-max (o clamp no resolve flex é corte documentado).
        (fw - frame).max(0.0)
    } else {
        let base = match css.width.and_then(|d| d.resolve(&resolve)) {
            // `width` explícito. Em `border-box`, o `width` INCLUI padding+border —
            // então o content é `width - (padding_h + 2*border)`. Em content-box
            // (default), o `width` JÁ é o content.
            Some(w) if border_box => (w - (padding_h + border_h)).max(0.0),
            Some(w) => w,
            // Sem width: shrink-to-fit → largura do conteúdo (limitada ao disponível);
            // senão (fluxo block normal) → ocupa a largura disponível.
            None if shrink_to_fit => {
                content_natural_width(dom, id, font_for_content, ctx).min((avail_w - frame).max(0.0))
            }
            None => (avail_w - frame).max(0.0),
        };
        // CLAMP min/max-width (#1751): `used = clamp(min, width, max)`. min/max são
        // sobre a CAIXA (border-box) na spec — descontamos o frame p/ aplicar ao
        // content quando border-box; em content-box já são do content.
        let mnw = css.min_width.and_then(|d| d.resolve(&resolve)).map(|v| {
            if border_box { (v - (padding_h + border_h)).max(0.0) } else { v }
        });
        let mxw = css.max_width.and_then(|d| d.resolve(&resolve)).map(|v| {
            if border_box { (v - (padding_h + border_h)).max(0.0) } else { v }
        });
        crate::style::clamp_size(base, mnw, mxw)
    };

    // `margin: 0 auto` (#1745): se o margin-left/right é `auto` E o bloco tem largura
    // definida (não ocupa o pai inteiro), o espaço livre se distribui pelos lados
    // auto — centralizando (ambos auto) ou empurrando (um só auto). Resolvido AQUI,
    // depois de saber o content_w. Só quando há largura explícita (senão o bloco já
    // ocupa avail_w e não há espaço a distribuir).
    let has_width = css.width.is_some() || css.max_width.is_some();
    if has_width {
        let box_outer = content_w + padding_h + border_h; // sem a margin
        let free = (avail_w - box_outer).max(0.0);
        match (m.left.is_auto(), m.right.is_auto()) {
            (true, true) => {
                margin_left = free / 2.0;
                margin_right = free / 2.0;
            }
            (true, false) => margin_left = (free - margin_right).max(0.0),
            (false, true) => margin_right = (free - margin_left).max(0.0),
            (false, false) => {}
        }
    }

    // Posição do content-box (canto sup-esq): deslocado pelo lado ESQUERDO/TOPO
    // (margin+border+padding daquele lado), não a soma do eixo.
    let content_x = x + margin_left + border_left + pad_left;
    let content_y = y + margin_top + border_top + pad_top;

    // Z-ORDER: o fundo/borda da caixa precisam ficar ATRÁS dos filhos. Como a
    // display list é pintada em ordem, reservamos AGORA o índice onde a caixa será
    // inserida (antes de qualquer filho), descemos nos filhos (que dão append no
    // fim), e só DEPOIS — conhecendo a altura — inserimos o fundo nesse índice.
    let box_index = list.items.len();
    // Quantas subárvores já existiam ANTES desta caixa começar. Só as que
    // vierem a seguir é que são empurradas quando o fundo for inserido — ver
    // [`insert_item`].
    let filhos_antes_da_caixa = list.children.len();
    // Reserva a posição do pai antes dos filhos; a geometria final é preenchida
    // depois que a altura natural do conteúdo for conhecida.
    reserve_node_order(list, id);

    // ── Filhos: o EIXO depende do `display` do bloco ─────────────────────────────
    // vertical (default): cada filho ABAIXO do anterior, ocupando a largura.
    // horizontal (`display:horizontal`/flex-row): cada filho À DIREITA do anterior,
    // a altura do content = a do filho mais alto (MDN flow: inline-axis stacking).
    let display = css_display(dom, id);
    let font_size = font_px(&css, DEFAULT_FONT_SIZE);

    // SCROLL CONTAINER (#1744): uma div com `overflow-x:auto/scroll` NÃO comprime os
    // filhos — eles transbordam e a div rola. Nesse caso layoutamos os filhos com a
    // largura NATURAL do conteúdo (intrinsic), não a do container. (overflow-y já não
    // comprime: o vertical empilha e a altura é a soma — só precisamos do clip+barra.)
    let ov_x = css.overflow_x.unwrap_or(crate::scrollbar::Overflow::Visible);
    let ov_y = css.overflow_y.unwrap_or(crate::scrollbar::Overflow::Visible);
    let scrolls_x = ov_x.scrollable() || ov_x == crate::scrollbar::Overflow::Hidden;
    // A inflação vale para o eixo do FLUXO HORIZONTAL, que é onde a compressão
    // aconteceria (o flex encolhe os itens até caberem). Nos demais layouts ela
    // vira base de PORCENTAGEM dos filhos, e aí está errada: `width:100%` dentro
    // de um container que rola é 100% da CAIXA, não do conteúdo transbordado.
    //
    // Medido na página real do WhatsApp Web, que aninha vários containers com
    // `overflow-y:auto`: cada nível multiplicava a largura do seguinte, e o
    // conteúdo terminava em x = 2300 numa janela de 1100 — a tela abria vazia
    // com tudo desenhado fora dela.
    let scroll_children_w = if scrolls_x {
        // largura que o conteúdo QUER (sem comprimir) — pode exceder content_w.
        intrinsic_content_width(dom, id, font_size, ctx).max(content_w)
    } else {
        content_w
    };
    let children_w = content_w;

    // `height` EXPLÍCITO resolve ANTES dos filhos (não depende deles): eles o
    // recebem como containing-block height (base do `height:%` deles), e o flex
    // COLUMN o usa como referência do eixo principal (justify/margin-auto).
    let frame_v = pad_top + pad_bottom + border_v;
    let explicit_content_h = resolve_height(css.height, avail_h, &resolve)
        .map(|h| if border_box { (h - frame_v).max(0.0) } else { h })
        // `aspect-ratio`: sem height explícito, a altura vem da largura / razão. Só
        // quando há largura resolvida (content_w) e uma razão > 0.
        .or_else(|| {
            css.aspect_ratio
                .filter(|r| *r > 0.0)
                .map(|r| (content_w / r).max(0.0))
        })
        // ALTURA IMPOSTA pelo flex (grow/stretch): o `forced_outer_h` é a altura
        // OUTER do item — o content-box é ela menos margem-v/frame. Vira o
        // containing block dos filhos (um filho `height:100%` resolve contra ela),
        // resolvendo o logo/caixa do google que crescem via flex-grow vertical.
        .or_else(|| {
            forced_outer_h.map(|oh| {
                let mv = margin_top + margin_bottom;
                (oh - mv - frame_v).max(0.0)
            })
        });

    // Altura que serve de CONTAINING BLOCK aos filhos (`height:%`): o height
    // explícito, senão um `max-height` conhecido (o Google dá ao container do
    // logo `height:calc(100% - 560px); max-height:290px` — o max é a altura
    // efetiva; sem isso o filho `height:100%` resolvia contra o conteúdo e
    // inflava). Calculado ANTES de layoutar os filhos (a resolução do `%` do
    // filho é top-down; a spec exige o CB conhecido).
    let mnh_pre = resolve_height(css.min_height, avail_h, &resolve)
        .map(|v| if border_box { (v - frame_v).max(0.0) } else { v });
    let mxh_pre = resolve_height(css.max_height, avail_h, &resolve)
        .map(|v| if border_box { (v - frame_v).max(0.0) } else { v });
    let avail_children = explicit_content_h.or(mxh_pre);

    // `flex-direction: column` — o eixo PRINCIPAL do flex vira o vertical: os itens
    // empilham (sem margin-collapse, que flex não tem), gap/justify/margin-auto
    // atuam no Y e align-items no X (stretch = ocupar a largura, o default).
    let is_column = css.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    let is_flex = display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP;
    let content_h = match display {
        // flex column (com ou sem wrap — multi-coluna do wrap é corte documentado).
        _ if is_flex && is_column => {
            layout_children_column(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, ctx, list)
        }
        // horizontal (flex-row sem wrap): lado a lado, encolhe pra caber, não quebra.
        d if d == crate::block::DISPLAY_HORIZONTAL => {
            layout_children_horizontal(dom, id, content_x, content_y, scroll_children_w, avail_children, &css, font_size, false, None, ctx, list)
        }
        // GRID REAL: track-sizing (px/fr/auto/%) + auto-placement row-by-row +
        // alinhamento de célula (align-items/justify-items). Só quando é
        // `display:grid` de fato; senão o wrap horizontal (inline-block flow).
        // TABELA: a grade inteira é construída antes de posicionar o que quer que
        // seja (a largura de uma célula vem da COLUNA, não dela). Fica antes do
        // grid porque uma `<table>` que o autor não tocou tem eixo vertical e
        // cairia no empilhamento de blocos, descendo por `<tr>` como se fossem
        // `<div>` — que é exatamente o que a página real mostrava.
        _ if used == Some(crate::style::DisplayKind::Table) => {
            crate::table::layout_table(dom, id, content_x, content_y, children_w, &css, font_size, ctx, list)
        }
        _ if css.effective_display() == Some(crate::style::DisplayKind::Grid) => {
            layout_children_grid(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, ctx, list)
        }
        // wrap (inline-block flow): lado a lado E QUEBRA linha quando enche.
        d if d == crate::block::DISPLAY_WRAP => {
            layout_children_horizontal(dom, id, content_x, content_y, scroll_children_w, avail_children, &css, font_size, true, None, ctx, list)
        }
        // vertical (block): empilha.
        _ => layout_children_vertical(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, ctx, list),
    };
    // MARCADOR do item de lista. Emitido DEPOIS dos filhos e com o content-box já
    // conhecido, e não desloca coisa nenhuma: `list-style-position: outside` (o
    // default, e o único que este motor desenha) põe o marcador FORA da caixa de
    // conteúdo, dentro do recuo que o `<ul>` já reservou.
    if used == Some(crate::style::DisplayKind::ListItem) {
        crate::listitem::emit_marker(dom, id, &css, content_x, content_y, font_size, ctx, list);
    }

    // a altura REAL do conteúdo (antes de `height` explícito a cortar) — p/ o scroll-Y.
    let content_h_natural = content_h;

    // `height` explícito SOBRESCREVE a altura do conteúdo (a caixa tem essa altura,
    // mesmo que o conteúdo seja menor) — já resolvido antes dos filhos.
    let content_h = explicit_content_h.unwrap_or(content_h);
    // CLAMP min/max-height (#1751): used = clamp(min, height, max) — eixo vertical
    // (`%` contra a ALTURA do containing block, como o height).
    let content_h = crate::style::clamp_size(content_h, mnh_pre, mxh_pre);
    // STRETCH do flex: altura OUTER imposta pelo container (align-items/self:
    // stretch) → content = outer - margens - frame_v; nunca ENCOLHE o conteúdo
    // (max com o natural — um item mais alto que a linha não é cortado).
    let content_h = match forced_outer_h {
        Some(fh) => (fh - margin_top - margin_bottom - frame_v).max(content_h),
        None => content_h,
    };

    // ── Insere a CAIXA (fundo + borda) no índice reservado, ATRÁS dos filhos ─────
    // O BORDER-BOX do nó: content + padding + border (NÃO a margin — esta é espaço
    // externo). É o retângulo que `getBoundingClientRect()` reporta.
    let box_rect = Rect::new(
        x + margin_left,
        y + margin_top,
        content_w + padding_h + border_h,
        content_h + pad_top + pad_bottom + border_v,
    );
    // Registra a geometria deste nó (base do getBoundingClientRect/offsetWidth).
    record_node_rect(list, id, box_rect);

    // Pinta a CAIXA (fundo/borda) ATRÁS dos filhos. `insert` no `box_index` põe o
    // fundo antes dos itens dos filhos (z-order).
    if css.has_box() {
        let radius = css.corner_radius.unwrap_or(0.0);
        // `opacity` do elemento: multiplica o ALPHA das cores próprias (fundo/borda).
        // Cobre o caso comum (card/botão/overlay com fade) sem grupo de compositing.
        // `visibility:hidden` zera o alpha de tudo o que ESTE elemento pinta. Não
        // salta o layout: o elemento continua a ocupar o espaço dele, que é
        // exatamente o que o distingue de `display:none` — e como a propriedade
        // é herdada, os descendentes chegam aqui já com ela.
        let op = if css.visibility == Some(crate::style::values::Visibility::Hidden) {
            0.0
        } else {
            css.opacity.unwrap_or(1.0)
        };
        // Insere na ordem: primeiro o fundo, depois a borda por cima dele (ambos
        // atrás dos filhos). `insert` desloca os filhos para a frente.
        let mut at = box_index;
        // SOMBRA primeiro (atrás de tudo): box-shadow.
        if let Some(sh) = css.box_shadow {
            insert_item(list, at, filhos_antes_da_caixa, DisplayItem::Shadow {
                rect: box_rect,
                dx: sh.dx,
                dy: sh.dy,
                blur: sh.blur,
                spread: sh.spread,
                color: apply_opacity(sh.color, op),
                radius,
            });
            at += 1;
        }
        // FUNDO: gradiente (se houver) OU cor sólida — a menos que uma MÁSCARA
        // dê a forma da caixa (ver `deve_suprimir_fundo`).
        let fundo = !deve_suprimir_fundo(&css);
        if let Some(g) = css.gradient.filter(|_| fundo) {
            insert_item(list, at, filhos_antes_da_caixa, DisplayItem::GradientRect {
                rect: box_rect,
                c0: apply_opacity(g.c0, op),
                c1: apply_opacity(g.c1, op),
                angle_deg: g.angle_deg,
                radius,
            });
            at += 1;
        } else if let Some(color) = css.bg.filter(|_| fundo) {
            let color = apply_opacity(color, op);
            insert_item(list, at, filhos_antes_da_caixa, DisplayItem::SolidRect { rect: box_rect, color, radius });
            at += 1;
        }
        for item in border_items(&css, box_rect, radius, op) {
            insert_item(list, at, filhos_antes_da_caixa, item);
            at += 1;
        }
    }

    // ── SCROLL CONTAINER interno (#1744): se a div rola (overflow-x/y) e o conteúdo
    // excede a caixa, (1) RECORTA os itens dos filhos ao content-box (BeginClip já
    // emitido depois da caixa, EndClip no fim), (2) registra a ScrollRegion p/ o
    // backend gerenciar o offset + pintar as barras. `hidden` também recorta (corta o
    // excesso, sem barra). `visible` não faz nada (transborda, como hoje).
    let clips = ov_x != crate::scrollbar::Overflow::Visible
        || ov_y != crate::scrollbar::Overflow::Visible;
    if clips {
        let content_rect = Rect::new(content_x, content_y, content_w, content_h);
        // BeginClip no índice onde os FILHOS começam (logo após os itens de caixa que
        // foram inseridos em `box_index`); EndClip no fim. Quantos itens de caixa:
        // fundo (se bg) + borda (se visível).
        let box_items = if css.has_box() {
            // MESMA contagem da emissão acima: sombra + (gradiente OU bg) + as
            // barras de borda/outline. Estas últimas vêm de `border_items`, a mesma
            // função que as emitiu — contar por outra regra é o que dessincroniza
            // o índice do clip quando uma borda por lado entra em jogo.
            css.box_shadow.is_some() as usize
                + (css.gradient.is_some() || css.bg.is_some()) as usize
                + border_items(&css, box_rect, css.corner_radius.unwrap_or(0.0), 1.0).len()
        } else {
            0
        };
        let children_start = box_index + box_items;
        // offset 0 aqui; o backend injeta o offset rolado por região antes de pintar.
        insert_item(
            list,
            children_start,
            filhos_antes_da_caixa,
            DisplayItem::BeginClip {
                rect: content_rect,
                node: id,
                offset_x: 0.0,
                offset_y: 0.0,
                filhos_antes: list.children.len(),
            },
        );
        list.items.push(DisplayItem::EndClip { filhos_dentro: list.children.len() });
        if std::env::var_os("RTS_CLIP_DEBUG").is_some() && content_w <= 2.0 {
            let filhos: Vec<(usize, f32)> = list.children.iter().map(|c| (c.at, c.dy)).collect();
            eprintln!(
                "[clip] no={id:?} box_index={box_index} children_start={children_start} end_at={} children={:?}",
                list.items.len() - 1,
                &filhos[filhos.len().saturating_sub(6)..]
            );
        }
        // só registra como rolável (com barra) se de fato rola (auto/scroll), não hidden.
        if ov_x.scrollable() || ov_y.scrollable() {
            list.scroll_regions.push(ScrollRegion {
                node_idx: id,
                visible: content_rect,
                content_w: scroll_children_w.max(content_w),
                content_h: content_h_natural,
                overflow_x: ov_x,
                overflow_y: ov_y,
            });
        }
    }

    // ── TRANSFORM (translate/scale/rotate): pós-processa os itens DESTE elemento e
    // seus descendentes (o range `[box_index..]`), em torno do CENTRO do border-box.
    // Aplicado por último (não afeta o fluxo/tamanho — como no CSS, transform é visual).
    if let Some(tf) = css.transform {
        if !tf.is_identity() {
            let cx = box_rect.x + box_rect.w / 2.0;
            let cy = box_rect.y + box_rect.h / 2.0;
            // translate em px + fração do tamanho do elemento (translate(-50%,-50%)).
            let tx = tf.tx + tf.tx_pct * box_rect.w;
            let ty = tf.ty + tf.ty_pct * box_rect.h;
            let (sin, cos) = tf.rot_deg.to_radians().sin_cos();
            // Um transform MUTA itens, e um item de subárvore reusada é
            // COMPARTILHADO — mutá-lo no lugar mudaria o desenho de todo mundo
            // que aponta para ele.
            //
            // Um TRANSLATE puro não precisa de achatar nada: a subárvore é
            // desenhada com um deslocamento que já existe no `ChildRef`, e somar
            // ao `dx`/`dy` dele é a mesma conta sem tocar no que é partilhado.
            //
            // Achatar aqui era um defeito com alcance muito além do elemento:
            // `materialize` reescreve `items` INTEIRO, e todos os índices que os
            // ancestrais reservaram para as caixas deles passam a apontar para
            // outro item. Um `position:absolute` com `transform:translateY(-50%)`
            // — uma regra de ícone, na folha do MediaWiki — punha a página
            // inteira da Wikipédia a zero: 16 813 elementos sem geometria porque
            // uma regra de 40 bytes casou com um `<span>`.
            let so_translate = tf.sx == 1.0 && tf.sy == 1.0 && tf.rot_deg == 0.0;
            if so_translate {
                for it in list.items[box_index..].iter_mut() {
                    translate_item(it, tx, ty);
                }
                for child in list.children.iter_mut().filter(|c| c.at >= box_index) {
                    child.dx += tx;
                    child.dy += ty;
                }
            } else {
                // Escala e rotação continuam a exigir os itens em mãos. Vale a
                // mesma ressalva de índices — por isso só quando não há
                // subárvore por referência para achatar.
                list.materialize();
                for it in list.items[box_index..].iter_mut() {
                    apply_transform_to_item(it, cx, cy, tx, ty, tf.sx, tf.sy, sin, cos);
                }
            }
        }
    }

    // Tamanho EXTERNO da caixa (outer = content + padding + border + margin) — cada
    // componente já é a SOMA do seu eixo (padding_h = left+right; margin_h idem;
    // border conta 2× pelos dois lados). Não multiplicar margin/padding por 2.
    let outer_w = content_w + padding_h + border_h + margin_h;
    let outer_h = content_h + pad_top + pad_bottom + border_v + margin_top + margin_bottom;
    (outer_w, outer_h)
}

/// Põe um filho-bloco do fluxo normal, REUSANDO o desenho dele quando nada que
/// o afete mudou.
///
/// É o layout incremental: `layout_epochs[nó]` sobe quando a subárvore muda (e
/// nos ancestrais dela), então um irmão intacto casa a chave e só precisa ser
/// deslocado. Numa lista de mil cartões em que um texto mudou, 999 cartões são
/// uma cópia de itens em vez de cascade + medição de texto + box model.
///
/// Só o fluxo VERTICAL normal entra aqui — sem `forced_outer_*` (flex) e sem
/// `shrink_to_fit`. Os outros caminhos dependem de negociação com os irmãos, e
/// um fragmento que ignorasse isso responderia errado.
#[allow(clippy::too_many_arguments)]
/// A chave do fragmento de um nó com certas constraints. Extraída porque o laço
/// do fluxo vertical CONSULTA o cache antes de classificar o filho: um fragmento
/// só existe para bloco-normal, então encontrá-lo já responde o que a
/// classificação responderia — e a classificação custa estilo computado,
/// `block::lookup` e a margem resolvida, mil vezes por frame.
fn fragment_key(
    dom: &Dom,
    id: NodeIdx,
    avail_w: f32,
    avail_h: Option<f32>,
    ctx: &LayoutCtx,
) -> crate::dom::FragmentKey {
    KeyBase::new(dom, avail_w, avail_h, ctx).key(dom, id)
}

/// A parte da chave de fragmento que NÃO varia entre os filhos de um container:
/// identidade da árvore, epochs globais, viewport, medidor e as constraints.
///
/// Montar a chave inteira por filho relia um `thread_local` (o epoch de estilo)
/// e refazia as conversões mil vezes por container — o laço do fluxo vertical
/// pergunta o mesmo a cada iteração e só o nó muda.
#[derive(Clone, Copy)]
struct KeyBase {
    tree: u64,
    style_epoch: u64,
    anim_epoch: u64,
    avail_w: u32,
    avail_h: Option<u32>,
    viewport_w: u32,
    viewport_h: u32,
    measurer: u64,
}

impl KeyBase {
    fn new(dom: &Dom, avail_w: f32, avail_h: Option<f32>, ctx: &LayoutCtx) -> KeyBase {
        KeyBase {
            tree: dom.cache_identity(),
            style_epoch: crate::style::props::style_epoch(),
            anim_epoch: dom.anim_epoch(),
            avail_w: avail_w.to_bits(),
            avail_h: avail_h.map(f32::to_bits),
            viewport_w: ctx.viewport_w.to_bits(),
            viewport_h: ctx.viewport_h.to_bits(),
            measurer: ctx.measurer.identity(),
        }
    }

    fn key(&self, dom: &Dom, id: NodeIdx) -> crate::dom::FragmentKey {
        crate::dom::FragmentKey {
            tree: self.tree,
            node_epoch: dom.layout_epoch(id),
            style_epoch: self.style_epoch,
            anim_epoch: self.anim_epoch,
            node: id,
            avail_w: self.avail_w,
            avail_h: self.avail_h,
            viewport_w: self.viewport_w,
            viewport_h: self.viewport_h,
            measurer: self.measurer,
        }
    }
}

/// Insere um item numa posição, corrigindo o ponto de entrada das SUBÁRVORES.
///
/// O box model emite os filhos primeiro e insere o fundo e a borda atrás deles;
/// as subárvores reusadas guardam o índice antes do qual entram, e sem esta
/// correção elas passariam a ser pintadas na frente do próprio fundo. Foi o que
/// um teste de altura percentual acusou, ao ver os retângulos na ordem trocada.
/// Insere um item em `at` e corrige o `at` das subárvores que ficam depois dele.
///
/// `filhos_antes` é quantas subárvores já existiam quando `at` foi RESERVADO, e
/// é o que distingue "esta subárvore é minha, empurra-a" de "esta subárvore já
/// cá estava, não lhe toques". Sem essa fronteira, um `at >= at` sozinho não
/// consegue separar os dois casos quando os índices coincidem — e coincidem
/// exatamente no caso que interessa: um `position:fixed`, pintado no fim do
/// documento, reserva o índice 0 da lista de topo, que é também o `at` do
/// fragmento onde vive a página inteira. O fixed empurrava a página para
/// depois de si e ficava ATRÁS dela; numa página real é o dropdown a
/// desaparecer por trás do conteúdo.
///
/// É a mesma distinção que o `BeginClip { filhos_antes }` já fazia, e pela mesma
/// razão: o índice sozinho não carrega a ordem de criação.
pub(crate) fn insert_item(
    list: &mut DisplayList,
    at: usize,
    filhos_antes: usize,
    item: DisplayItem,
) {
    list.items.insert(at, item);
    for child in list.children.iter_mut().skip(filhos_antes) {
        if child.at >= at {
            child.at += 1;
        }
    }
}

/// Reconstrói o fragmento de um container trocando SÓ as subárvores sujas.
///
/// Devolve `None` — e o chamador refaz tudo — quando alguma premissa não vale:
/// o próprio nó foi alvo da invalidação (o estilo DELE pode ter mudado); não há
/// desenho anterior ou ele não tinha subárvores; a sujeira não tem alvo ou está
/// espalhada demais; a lista de filhos mudou; ou a subárvore refeita mudou de
/// ALTURA ou de margem, e aí tudo abaixo dela desloca.
fn costurar(
    dom: &Dom,
    id: NodeIdx,
    key: crate::dom::FragmentKey,
    ctx: &LayoutCtx,
) -> Option<std::rc::Rc<Fragment>> {
    if dom.is_self_dirty(id) {
        return None;
    }
    let (antiga, anterior) = dom.last_fragment_of(id)?;
    // Só o epoch do nó pode diferir: viewport, constraints, estilo global e
    // animação mudam o desenho inteiro, não uma parte dele.
    if (antiga.tree, antiga.avail_w, antiga.avail_h, antiga.viewport_w, antiga.viewport_h)
        != (key.tree, key.avail_w, key.avail_h, key.viewport_w, key.viewport_h)
        || (antiga.style_epoch, antiga.anim_epoch, antiga.measurer)
            != (key.style_epoch, key.anim_epoch, key.measurer)
    {
        return None;
    }
    if anterior.children.is_empty() {
        return None;
    }
    let sujos = dom.dirty_children_of(id)?;
    // A SEQUÊNCIA de filhos precisa ser a mesma, não só o tamanho: inserção,
    // remoção e reordenação mudam quem desenha o quê, e trocar uma referência
    // não daria conta. Comparar índice a índice é uma passada de leitura.
    if !mesma_sequencia_de_filhos(dom, id, &anterior.children) {
        return None;
    }
    let _phase = crate::metrics::phases::scope("fragment-patch");

    let mut children = anterior.children.clone();
    let mut trocou = false;
    for child in &mut children {
        if !sujos.contains(&child.node) {
            continue;
        }
        let mut own = DisplayList::default();
        // Onde o filho FOI POSTO: a origem em que o fragmento dele foi calculado
        // mais o deslocamento com que entrou aqui. Somar à origem do PAI daria
        // uma posição sem sentido — foi o que o teste de equivalência mostrou,
        // com o texto reaparecendo em (0,16) em vez de (12, 67.4).
        let origem = (child.fragment.origin.0 + child.dx, child.fragment.origin.1 + child.dy);
        let margem = child.margin_top;
        let ((_, altura), nova_margem) = layout_block_reusing(
            dom,
            child.node,
            origem.0,
            origem.1,
            child.avail_w,
            child.avail_h,
            || margem,
            ctx,
            &mut own,
        );
        if (altura - child.height).abs() > 0.001 || (nova_margem - child.margin_top).abs() > 0.001 {
            return None;
        }
        // O `layout_block_reusing` emitiu numa lista própria; o que interessa é a
        // referência que ele acabou de registrar para este nó.
        let novo = own.children.first()?.fragment.clone();
        child.fragment = novo;
        trocou = true;
    }
    if !trocou {
        return None;
    }
    let fragment = std::rc::Rc::new(Fragment {
        node: id,
        // Compartilha o que NÃO mudou — só a lista de subárvores é nova.
        items: std::rc::Rc::clone(&anterior.items),
        children,
        rects: std::rc::Rc::clone(&anterior.rects),
        hit_order: std::rc::Rc::clone(&anterior.hit_order),
        scroll_regions: anterior.scroll_regions.clone(),
        origin: anterior.origin,
        size: anterior.size,
        margin_top: anterior.margin_top,
    });
    dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    Some(fragment)
}

/// `true` se os filhos-elemento do nó são exatamente os que o desenho anterior
/// referencia, na mesma ordem. Uma passada de leitura; o que não é barato é o
/// layout deles.
fn mesma_sequencia_de_filhos(dom: &Dom, id: NodeIdx, children: &[ChildRef]) -> bool {
    let mut esperados = children.iter().map(|c| c.node);
    let mut atuais = dom
        .node(id)
        .children
        .iter()
        .copied()
        .filter(|&c| matches!(dom.node(c).kind, NodeKind::Element { .. }));
    loop {
        match (esperados.next(), atuais.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) if a == b => continue,
            _ => return false,
        }
    }
}

fn emit_fragment(
    fragment: &std::rc::Rc<Fragment>,
    list: &mut DisplayList,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) {
    let _phase = crate::metrics::phases::scope("fragment-emit");
    fragment.emit_at(list, x, y, avail_w, avail_h);
}

fn layout_block_reusing(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    margem_de_topo: impl FnOnce() -> f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> ((f32, f32), f32) {
    let key = fragment_key(dom, id, avail_w, avail_h, ctx);
    if let Some(fragment) = dom.fragment_get(key) {
        crate::bump!(fragment_hits);
        emit_fragment(&fragment, list, x, y, avail_w, avail_h);
        return (fragment.size, fragment.margin_top);
    }
    // COSTURA: trocar no desenho anterior só a subárvore que ficou suja. Agora
    // que a saída é uma ÁRVORE, costurar é substituir uma REFERÊNCIA num vetor
    // de mil entradas de 48 bytes — a primeira versão disto (revertida) copiava
    // 3000 itens com String e por isso não ganhava nada.
    if let Some(fragment) = costurar(dom, id, key, ctx) {
        crate::bump!(fragment_patches);
        emit_fragment(&fragment, list, x, y, avail_w, avail_h);
        return (fragment.size, fragment.margin_top);
    }
    crate::bump!(fragment_misses);
    let _phase = crate::metrics::phases::scope("fragment-build");
    // Lista PRÓPRIA: o fragmento precisa saber exatamente quais itens são dele,
    // e a única forma de saber isso é não misturá-los com os dos irmãos.
    let mut own = DisplayList::default();
    let size = layout_block(dom, id, x, y, avail_w, avail_h, None, None, false, ctx, &mut own);
    let fragment = std::rc::Rc::new(Fragment {
        node: id,
        rects: std::rc::Rc::new(own.node_rects.iter().map(|(idx, rect)| (*idx, *rect)).collect()),
        hit_order: std::rc::Rc::new(std::mem::take(&mut own.hit_order)),
        scroll_regions: std::mem::take(&mut own.scroll_regions),
        items: std::rc::Rc::new(std::mem::take(&mut own.items)),
        children: std::mem::take(&mut own.children),
        origin: (x, y),
        size,
        margin_top: margem_de_topo(),
    });
    dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    fragment.emit_at(list, x, y, avail_w, avail_h);
    (fragment.size, fragment.margin_top)
}

/// Uma subárvore emitida por referência dentro de uma lista ou de outro
/// fragmento.
#[derive(Clone, Debug)]
pub struct ChildRef {
    /// O nó que esta subárvore desenha — a costura precisa saber quem é.
    pub node: NodeIdx,
    /// Altura externa que ele ocupou e a margem de topo resolvida: se qualquer
    /// uma mudar ao refazê-lo, tudo abaixo desloca e a costura não serve.
    pub height: f32,
    pub margin_top: f32,
    /// As CONSTRAINTS com que ele foi layoutado — as do CONTEÚDO do pai, não as
    /// do pai. Refazer um filho com a largura do container em vez da do conteúdo
    /// dá uma caixa larga demais pela soma do padding e da margem.
    pub avail_w: f32,
    pub avail_h: Option<f32>,
    /// Posição em `items` ANTES da qual esta subárvore é pintada.
    pub at: usize,
    /// Posição em `hit_order` antes da qual a ordem de hit-test dela entra.
    ///
    /// Separada do `at` porque as duas sequências crescem por motivos
    /// diferentes: nem todo item de pintura registra um nó, e nem todo nó
    /// registrado pinta um item. Montar a ordem de hit-test com os próprios
    /// primeiro e os das subárvores depois inverte o z-order — foi o que o teste
    /// de `z-index` acusou.
    pub hit_at: usize,
    pub fragment: std::rc::Rc<Fragment>,
    pub dx: f32,
    pub dy: f32,
}

impl PartialEq for ChildRef {
    /// Compara CONTEÚDO — duas listas equivalentes podem ter chegado ao mesmo
    /// desenho por caminhos diferentes.
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
            && self.dx == other.dx
            && self.dy == other.dy
            && self.fragment.items == other.fragment.items
            && self.fragment.children == other.fragment.children
    }
}

/// O DESENHO de uma subárvore posta com certas constraints, guardado para ser
/// reusado numa posição diferente.
///
/// Coordenadas ABSOLUTAS, como saíram do layout: a origem em que foi calculado
/// fica registrada em `origin`, e reusar é somar a diferença. Guardar já
/// relativo daria na mesma e custaria uma passada extra na hora de gravar — o
/// caso comum é justamente reusar na MESMA posição (nada acima dele mudou de
/// altura), e aí a soma é zero e nem se percorre.
#[derive(Clone, Debug, Default)]
pub struct Fragment {
    /// O nó que este fragmento desenha.
    pub node: NodeIdx,
    /// Itens de pintura PRÓPRIOS desta subárvore.
    ///
    /// Os três vetores grandes são `Rc`: quando um container é COSTURADO, só a
    /// lista de subárvores muda, e clonar retângulos e ordem de hit-test de um
    /// container de mil filhos custaria mais do que a costura economiza.
    pub items: std::rc::Rc<Vec<DisplayItem>>,
    /// As subárvores que ela reusou, por referência — o desenho é uma árvore.
    pub children: Vec<ChildRef>,
    /// Geometria por nó (o que alimenta `getBoundingClientRect`).
    pub rects: std::rc::Rc<Vec<(NodeIdx, Rect)>>,
    /// Ordem de pintura para o hit-test (ancestral antes de descendente).
    pub hit_order: std::rc::Rc<Vec<NodeIdx>>,
    /// Regiões roláveis internas descobertas dentro da subárvore.
    pub scroll_regions: Vec<ScrollRegion>,
    /// Onde este fragmento foi calculado.
    pub origin: (f32, f32),
    /// Tamanho externo devolvido pelo `layout_block` (o que o chamador usa para
    /// avançar o cursor).
    pub size: (f32, f32),
    /// A MARGEM DE TOPO resolvida deste bloco, para o colapso com o irmão
    /// anterior.
    ///
    /// Guardada junto porque o laço a calculava ANTES de descobrir que o
    /// fragmento servia: resolver a margem pede o estilo computado, o
    /// `font-size` do contexto e um `ResolveCtx` — por filho, mil vezes por
    /// frame, para um valor que não muda enquanto o epoch do nó não muda.
    pub margin_top: f32,
}

impl Fragment {
    /// Emite este fragmento numa `DisplayList`, deslocado para `(x, y)`.
    pub fn emit_at(
        self: &std::rc::Rc<Self>,
        list: &mut DisplayList,
        x: f32,
        y: f32,
        avail_w: f32,
        avail_h: Option<f32>,
    ) {
        let (dx, dy) = (x - self.origin.0, y - self.origin.1);
        // APONTA, não copia: os itens desta subárvore já existem e não mudaram.
        // Os RETÂNGULOS abaixo continuam sendo materializados, porque a consulta
        // de geometria é por nó e precisa do valor pronto — são 16 bytes contra
        // os 48 de um item, numa quantidade menor.
        list.children.push(ChildRef {
            node: self.node,
            height: self.size.1,
            margin_top: self.margin_top,
            avail_w,
            avail_h,
            at: list.items.len(),
            hit_at: list.hit_order.len(),
            fragment: std::rc::Rc::clone(self),
            dx,
            dy,
        });
        // A GEOMETRIA da subárvore (retângulos, ordem de hit-test, regiões
        // roláveis) também fica na referência: materializá-la aqui era metade do
        // custo de um frame parado — três inserções em mapa por fragmento, mil
        // fragmentos. Quem precisa dela chama `geometry()`, que percorre a
        // árvore uma vez e guarda o resultado.
    }
}

impl Fragment {
    /// Quantos itens este fragmento pinta, contando as subárvores que ele reusa.
    pub fn total_items(&self) -> usize {
        self.items.len()
            + self.children.iter().map(|c| c.fragment.total_items()).sum::<usize>()
    }
}

/// Percorre itens próprios e subárvores na ordem de pintura, acumulando o
/// deslocamento. Recursivo pela mesma razão que a estrutura é uma árvore: um
/// fragmento pode ter reusado outro.
fn walk_items(
    items: &[DisplayItem],
    children: &[ChildRef],
    dx: f32,
    dy: f32,
    f: &mut impl FnMut(&DisplayItem, f32, f32),
) {
    let mut next_child = 0usize;
    for (i, item) in items.iter().enumerate() {
        // Um `EndClip` só deixa passar à frente dele os filhos que JÁ existiam
        // quando foi emitido — ver a doc da variante. Para todo o resto o empate
        // no índice resolve-se a favor do filho, que é o que põe uma subárvore
        // reusada no meio dos itens próprios.
        // Um `BeginClip` empurra à sua FRENTE os filhos que já existiam antes
        // dele: o `at` deles foi deslocado pela inserção do marcador, e sem isto
        // o conteúdo inteiro da página cai dentro de um clip que não é dele.
        if let DisplayItem::BeginClip { filhos_antes, .. } = item {
            while next_child < *filhos_antes && next_child < children.len() {
                let c = &children[next_child];
                walk_items(&c.fragment.items, &c.fragment.children, dx + c.dx, dy + c.dy, f);
                next_child += 1;
            }
        }
        let teto = match item {
            DisplayItem::EndClip { filhos_dentro } => *filhos_dentro,
            _ => children.len(),
        };
        while next_child < teto.min(children.len()) && children[next_child].at <= i {
            let c = &children[next_child];
            walk_items(&c.fragment.items, &c.fragment.children, dx + c.dx, dy + c.dy, f);
            next_child += 1;
        }
        f(item, dx, dy);
    }
    for c in &children[next_child..] {
        walk_items(&c.fragment.items, &c.fragment.children, dx + c.dx, dy + c.dy, f);
    }
}

/// DESLOCA um item de pintura por `(dx, dy)`.
///
/// É a operação que torna um fragmento de layout REUSÁVEL: o desenho de uma
/// subárvore cujo conteúdo e constraints não mudaram é o mesmo desenho, na
/// posição nova. Tudo o que um item carrega é geometria absoluta em coordenadas
/// de conteúdo, então deslocar é somar — exceto o que é tamanho (`radius`,
/// `blur`, `size` do texto), que não se move.
fn translate_item(it: &mut DisplayItem, dx: f32, dy: f32) {
    let shift = |r: &mut Rect| {
        r.x += dx;
        r.y += dy;
    };
    match it {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::Shadow { rect, .. }
        | DisplayItem::GradientRect { rect, .. }
        | DisplayItem::Border { rect, .. }
        | DisplayItem::Image { rect, .. }
        | DisplayItem::Pixels { rect, .. }
        | DisplayItem::BeginClip { rect, .. } => shift(rect),
        DisplayItem::Text { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        DisplayItem::EndClip { .. } => {}
    }
}

/// Aplica um transform (translate `tx,ty` + escala `sx,sy` + rotação `sin,cos`) em
/// torno do centro `(cx,cy)` a um DisplayItem, mutando suas coords. Rects escalam de
/// tamanho; a rotação move o canto (aproximação: rotaciona a posição, não o próprio
/// rect — cobre o uso comum sem mesh rotacionado). Texto/pos: rotaciona+escala o ponto.
#[allow(clippy::too_many_arguments)]
fn apply_transform_to_item(
    it: &mut DisplayItem,
    cx: f32,
    cy: f32,
    tx: f32,
    ty: f32,
    sx: f32,
    sy: f32,
    sin: f32,
    cos: f32,
) {
    // transforma UM ponto: escala em torno do centro, rotaciona, translada.
    let xf = |px: f32, py: f32| -> (f32, f32) {
        let (mut dx, mut dy) = (px - cx, py - cy);
        dx *= sx;
        dy *= sy;
        let rx = dx * cos - dy * sin;
        let ry = dx * sin + dy * cos;
        (cx + rx + tx, cy + ry + ty)
    };
    match it {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::Border { rect, .. }
        | DisplayItem::GradientRect { rect, .. }
        | DisplayItem::Shadow { rect, .. }
        | DisplayItem::Image { rect, .. }
        | DisplayItem::Pixels { rect, .. } => {
            let (nx, ny) = xf(rect.x, rect.y);
            rect.x = nx;
            rect.y = ny;
            rect.w *= sx;
            rect.h *= sy;
        }
        DisplayItem::Text { x, y, size, .. } => {
            let (nx, ny) = xf(*x, *y);
            *x = nx;
            *y = ny;
            *size *= sy; // escala o texto na vertical (aproxima).
        }
        DisplayItem::BeginClip { rect, .. } => {
            let (nx, ny) = xf(rect.x, rect.y);
            rect.x = nx;
            rect.y = ny;
            rect.w *= sx;
            rect.h *= sy;
        }
        DisplayItem::EndClip { .. } => {}
    }
}

/// Largura NATURAL do conteúdo de um nó (sem `width` explícito): a maior largura
/// de uma linha de texto entre os descendentes. É o "preferred width" do
/// shrink-to-fit (item flex / inline-block). Para um filho-bloco com `width`, usa
/// esse width (+ frame); para texto, a largura medida. Aproximação do max-content
/// (o inline-flow exato — palavras quebrando — vem na fatia de inline).
fn content_natural_width(dom: &Dom, id: NodeIdx, font: f32, ctx: &LayoutCtx) -> f32 {
    intrinsic_content_width(dom, id, font, ctx)
}

/// LARGURA INTRÍNSECA do CONTEÚDO de um elemento (max-content): quanto o conteúdo
/// QUER de largura sem quebrar. É a BASE de toda medição (shrink-to-fit, item flex,
/// inline-block, container flex). CONSCIENTE DO DISPLAY dos filhos:
/// - flex-ROW (horizontal/wrap): SOMA as larguras outer dos filhos + os gaps (eles
///   ficam lado a lado). Era o bug do navbar: `.logo`/`.links` (flex) mediam pelo
///   MAX, dando ~0.
/// - block (vertical): MAX das larguras dos filhos (empilham).
/// - texto: a largura do texto concatenado.
/// Recursivo: a largura de um filho é a SUA intrínseca + frame (ou seu `width` fixo).
fn intrinsic_content_width(dom: &Dom, id: NodeIdx, font: f32, ctx: &LayoutCtx) -> f32 {
    let key = IntrinsicWidthKey {
        tree: dom.cache_identity(),
        node_epoch: dom.layout_epoch(id),
        style_epoch: crate::style::props::style_epoch(),
        node: id,
        font_size: font.to_bits(),
        viewport_w: ctx.viewport_w.to_bits(),
        viewport_h: ctx.viewport_h.to_bits(),
        measurer: ctx.measurer.identity(),
    };
    crate::bump!(intrinsic_calls);
    if let Some(hit) = dom.intrinsic_width_get(key) {
        crate::bump!(intrinsic_hits);
        return hit;
    }

    // folha de texto puro → largura do texto.
    let own_text = collect_text(dom, id);
    let only_text = !dom.node(id).children.is_empty()
        && dom.node(id).children.iter().all(|&c| matches!(dom.node(c).kind, NodeKind::Text(_)));
    if (dom.node(id).children.is_empty() || only_text) && !own_text.trim().is_empty() {
        let css = dom.computed_style_idx(id);
        let mono = css
            .as_ref()
            .and_then(|c| c.font_family.as_ref())
            .map(|f| crate::style::is_mono_family(f))
            .unwrap_or(false);
        // o peso importa p/ a largura natural: medir regular mas o wrap/paint usar bold
        // (mais largo) faz o conteúdo não caber na largura natural → quebra indevida.
        let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(false);
        let width = ctx.measurer.text_width(&own_text, font, mono, bold);
        dom.intrinsic_width_put(key, width);
        return width;
    }

    // TABELA: a largura que o conteúdo quer é a SOMA das colunas, e nenhuma das
    // duas regras abaixo a dá — o MAX (bloco) devolveria a linha mais larga e a
    // SOMA (flex) somaria linhas inteiras. Quem sabe é o algoritmo de colunas.
    if used_display(dom, id) == Some(crate::style::DisplayKind::Table) {
        let width = crate::table::max_content_width(dom, id, font, ctx);
        dom.intrinsic_width_put(key, width);
        return width;
    }

    // o EIXO em que os filhos se dispõem decide SOMA vs MAX.
    let display = css_display(dom, id);
    let is_row = display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP;
    let gap = if is_row {
        let resolve = ResolveCtx {
            parent_content_w: ctx.viewport_w,
            node_font_size: font,
            root_font_size: DEFAULT_FONT_SIZE,
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        dom.computed_style_idx(id)
            .and_then(|c| c.gap)
            .and_then(|d| d.resolve(&resolve))
            .unwrap_or(0.0)
            .max(0.0)
    } else {
        0.0
    };

    let mut sum = 0.0f32;
    let mut max = 0.0f32;
    let mut count: usize = 0;
    for &child in &dom.node(id).children {
        // fora do fluxo não contribui para a largura intrínseca do container.
        if is_out_of_flow(dom, child) {
            continue;
        }
        let w = intrinsic_outer_width(dom, child, font, ctx);
        if w > 0.0 {
            count += 1;
        }
        sum += w;
        max = max.max(w);
    }
    let width = if is_row {
        // soma + gaps entre os itens.
        sum + (count.saturating_sub(1)) as f32 * gap
    } else {
        max
    };
    dom.intrinsic_width_put(key, width);
    width
}

/// A largura OUTER intrínseca de UM filho (max-content): seu `width` fixo (+ frame),
/// senão a intrínseca do seu conteúdo (+ frame). Texto → largura do texto.
pub(crate) fn intrinsic_outer_width(dom: &Dom, id: NodeIdx, parent_font: f32, ctx: &LayoutCtx) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Element { .. } => {
            // metadata (head/style/script) não conta.
            if let NodeKind::Element { tag } = &dom.node(id).kind {
                if is_non_rendered_tag(tag) {
                    return 0.0;
                }
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            let f = font_px(&css, parent_font);
            let border_box = css.border_box.unwrap_or(false);
            let resolve = ResolveCtx {
                parent_content_w: ctx.viewport_w,
                node_font_size: f,
                root_font_size: DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // O frame conta em `resolve_h_intrinseco`: uma percentagem de padding
            // ou margem é contra a largura do containing block, que é o que esta
            // medição existe para ajudar a decidir.
            let frame = css.margin.resolve_h_intrinseco(&resolve)
                + 2.0 * css.border_width.unwrap_or(0.0)
                + css.padding.resolve_h_intrinseco(&resolve);
            // `width` fixo: a caixa tem essa largura. Um `width` em PERCENTAGEM
            // não é fixo — contribui como `auto`, e o conteúdo decide. Sem esta
            // distinção, um `width:50%` respondia metade da VIEWPORT: um item
            // flex com um filho assim ocupava a linha toda e empurrava o irmão
            // para a linha de baixo, que é a origem dos 120px de desvio do `<h1>`
            // da Wikipédia.
            if let Some(w) = crate::style::dimensao_absoluta(css.width.unwrap_or(crate::style::Dimension::Auto), &resolve) {
                return if border_box {
                    w + css.margin.resolve_h_intrinseco(&resolve)
                } else {
                    w + frame
                };
            }
            // senão: a intrínseca do conteúdo + frame.
            intrinsic_content_width(dom, id, f, ctx) + frame
        }
        NodeKind::Text(t) => ctx.measurer.text_width(t, parent_font, false, false),
        _ => 0.0,
    }
}

/// `true` se um nó-elemento deve ser tratado como BLOCO no layout (entra em
/// `layout_block`, com sua própria caixa/eixo) — em vez de inline (texto corrido).
/// É bloco se: tem `display` no CSS (qualquer um define caixa própria), OU tem um
/// default de display registrado (`block::lookup` = defineBlock, alimentado pela
/// UA-stylesheet `ua.ts` para div/p/… e pelo autor). Tags inline puras (sem nada
/// disso) fluem como texto. O motor NÃO nomeia tags HTML — os defaults são dados
/// do prelude TS.
fn is_block_level(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // `<img>` com imagem decodificada é um elemento REPLACED (conteúdo visual
            // intrínseco) → precisa de layout_block p/ emitir o DisplayItem::Image,
            // mesmo sem CSS de caixa.
            if tag == "img" && dom.image_of(id).is_some() {
                return true;
            }
            // `<canvas>` é REPLACED como o `<img>`: a caixa vem dos atributos
            // `width`/`height` e o conteúdo são pixels. Sem esta linha ele cai no
            // fluxo inline, onde não há quem emita a superfície — e um canvas
            // pintado não aparecia na tela, com o resto da página intacto.
            if tag == "canvas" {
                return true;
            }
            // `<svg>` é replaced: layout_svg_placeholder reserva a caixa (logo/
            // ícones do google ocupam o espaço certo em vez de colapsar).
            if tag == "svg" {
                return true;
            }
            let css = dom.computed_style_idx(id);
            css.as_ref().and_then(|c| c.effective_display()).is_some()
                || crate::block::lookup(tag).is_some()
                // INLINE-BLOCK de fato: um elemento inline (`<a>`/`<span>`/`<button>`)
                // que tem CAIXA própria (fundo/borda/padding/width/height) precisa de
                // layout_block p/ pintar essa caixa e respeitar o padding — senão o
                // botão fica sem fundo/borda. (`has_box` cobre bg/pad/margin/border/
                // radius/width; +height.)
                // Uma tag inline só vira bloco quando o estilo CRIA caixa — ver
                // `inline_box::cria_caixa_de_bloco` para porque não é `has_box`.
                // Uma tag inline só vira bloco quando o estilo CRIA caixa — ver
                // `inline_box::cria_caixa_de_bloco` para porque não é `has_box`.
                // Uma tag inline só vira bloco quando o estilo CRIA caixa — ver
                // `inline_box::cria_caixa_de_bloco` para porque não é `has_box`.
                || css.as_ref().map(|c| crate::inline_box::cria_caixa_de_bloco(c)).unwrap_or(false)
        }
        _ => false,
    }
}

/// `true` se o elemento é INLINE-BLOCK: tem caixa (vira bloco p/ pintar) MAS é inline
/// por natureza (`<a>`/`<span>`/`<button>`/etc., não uma tag block) e SEM width que
/// ocupe o pai → dimensiona pelo CONTEÚDO (shrink-to-fit), como o pill/botão. Tags
/// block conhecidas (div/p/section…) NÃO são inline-block (ocupam o pai).
fn is_inline_block(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            let css = dom.computed_style_idx(id);
            let explicit_block = css
                .as_ref()
                .and_then(|c| c.effective_display())
                .map(|d| d != crate::style::DisplayKind::Inline)
                .unwrap_or(false);
            // `<input>`/`<button>`/`<select>`/`<textarea>` são inline-block por
            // DEFAULT do browser (fluem lado a lado — os botões do google), a não
            // ser que o CSS do autor force um display de bloco. É o que faz os 2
            // `<input type=submit>` irmãos ficarem na MESMA linha.
            if matches!(tag.as_str(), "input" | "button" | "select" | "textarea") {
                return !explicit_block;
            }
            // tag block conhecida OU display de bloco explícito → NÃO é inline-block.
            if crate::block::lookup(tag).is_some() || explicit_block {
                return false;
            }
            // é inline-com-box (tem caixa mas é tag inline) → inline-block.
            css.as_ref().map(|c| crate::inline_box::cria_caixa_de_bloco(c)).unwrap_or(false)
        }
        _ => false,
    }
}

/// `true` quando o fundo do elemento NÃO deve ser pintado por a forma dele vir de
/// uma `mask-image` que não sabemos carregar.
///
/// Em CSS a máscara RECORTA o fundo: `background-color` mais `mask-image` é o modo
/// canónico de desenhar um ícone monocromático (o MediaWiki fá-lo em
/// `.cdx-button__icon`, e a Wikipédia traz 24 deles). Pintar o fundo sem a máscara
/// não é uma aproximação da forma — é o retângulo inteiro, um bloco cinzento onde
/// o browser mostra um glifo. Não pintar nada erra por omissão, que é o erro
/// menor, e é a mesma regra do CLAUDE.md sobre superfícies que não fazem o que o
/// nome diz: a ausência falha à vista, o oco engana.
///
/// SUBSTITUTO TEMPORÁRIO. Quando carregarmos e aplicarmos máscaras a sério, o
/// fundo volta a ser pintado e passa a ser recortado pela máscara — esta função
/// desaparece em vez de mudar de resposta.
fn deve_suprimir_fundo(css: &ComputedStyle) -> bool {
    css.mask_image.is_some()
}

/// Código de decoração de texto p/ o `DisplayItem::Text` a partir do estilo:
/// 0=nenhuma, 1=underline, 2=line-through, 3=overline.
fn decoration_code(css: &ComputedStyle) -> u8 {
    match css.text_decoration {
        Some(crate::style::values::TextDecoration::Underline) => 1,
        Some(crate::style::values::TextDecoration::LineThrough) => 2,
        Some(crate::style::values::TextDecoration::Overline) => 3,
        _ => 0,
    }
}

/// Multiplica o ALPHA de uma cor `0xRRGGBBAA` por `opacity` ∈ [0,1] (o RGB fica
/// A cor com que um elemento pinta, dado o seu `visibility`.
///
/// `visibility:hidden` não salta o layout — o elemento ocupa o espaço na mesma —,
/// só não é pintado. Zerar o alpha é como isso se exprime numa display list que
/// não tem grupos de compositing, e a propriedade ser HERDADA faz o resto: os
/// descendentes chegam ao seu próprio layout já com ela posta.
fn cor_visivel(css: &crate::style::ComputedStyle, cor: u32) -> u32 {
    if css.visibility == Some(crate::style::values::Visibility::Hidden) {
        cor & 0xFFFF_FF00
    } else {
        cor
    }
}

/// intacto; só o canal alpha escala). `opacity >= 1` devolve a cor inalterada.
fn apply_opacity(color: u32, opacity: f32) -> u32 {
    if opacity >= 1.0 {
        return color;
    }
    let op = opacity.clamp(0.0, 1.0);
    let a = (color & 0xFF) as f32;
    let new_a = (a * op).round().clamp(0.0, 255.0) as u32;
    (color & 0xFFFF_FF00) | new_a
}

/// `true` se a tag é um campo de TEXTO editável (mini-browser): `<input>` (tipos
/// textuais) ou `<textarea>`. Um `<input type=checkbox/radio/...>` não conta (v1
/// só faz texto). Sem `type` → texto (o default do HTML).
fn is_text_input_tag(tag: &str) -> bool {
    matches!(tag, "input" | "textarea")
}

/// Layout de um `<input>`/`<textarea>` editável: emite a CAIXA (fundo+borda), o
/// TEXTO (o valor digitado, ou o `placeholder` apagado se vazio) e, se o campo tem
/// o FOCO, um CURSOR (barrinha) após o texto. Void (sem filhos) — o egui só recebe
/// SolidRect+Text+SolidRect e pinta burramente. Retorna `(outer_w, outer_h)`.
#[allow(clippy::too_many_arguments)]
/// Layout de um `<img>` com pixels já decodificados: emite `DisplayItem::Image` no
/// rect. Tamanho: `width`/`height` do CSS se houver; senão o natural da imagem (mas
/// limitado à largura disponível, preservando a proporção). `None` se o `<img>` ainda
/// não tem imagem setada (nada a pintar). Retorna `(outer_w, outer_h)`.
#[allow(clippy::too_many_arguments)]
/// Reserva a CAIXA de um `<svg>` (replaced element) sem desenhar o vetor: usa
/// `width`/`height` do CSS ou dos atributos; se só um lado é dado e há `viewBox`,
/// deriva o outro pela razão de aspecto; se nada, cai numa proporção do viewBox
/// ou num tamanho default. Pinta um placeholder cinza-claro (a "caixa" do ícone/
/// logo) no rect. `None` se não dá pra dimensionar (colapsa como antes).
fn layout_svg_placeholder(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let node = dom.node(id);
    let attr_px = |name: &str| -> Option<f32> {
        node.attr(name).and_then(|v| {
            let v = v.trim().trim_end_matches("px");
            v.parse::<f32>().ok().filter(|n| *n > 0.0)
        })
    };
    // razão de aspecto do viewBox ("0 0 W H" → W/H).
    let vb_ratio = node.attr("viewBox").and_then(|vb| {
        let n: Vec<f32> = vb.split_whitespace().filter_map(|s| s.parse().ok()).collect();
        if n.len() == 4 && n[3] > 0.0 { Some(n[2] / n[3]) } else { None }
    });
    let css_w = css.width.and_then(|d| d.resolve(&resolve)).filter(|w| *w > 0.0);
    let css_h = css.height.and_then(|d| resolve_height(Some(d), None, &resolve)).filter(|h| *h > 0.0);
    let w0 = css_w.or_else(|| attr_px("width"));
    let h0 = css_h.or_else(|| attr_px("height"));
    // resolve (w, h): ambos dados usa-os; só um + viewBox deriva o outro; nada →
    // um ícone default (24×24) ou o viewBox escalado a 24 de altura.
    let (w, h) = match (w0, h0) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, vb_ratio.map(|r| w / r).unwrap_or(w)),
        (None, Some(h)) => (vb_ratio.map(|r| h * r).unwrap_or(h), h),
        (None, None) => {
            let h = 24.0;
            (vb_ratio.map(|r| h * r).unwrap_or(h), h)
        }
    };
    let w = w.min(avail_w.max(1.0));
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    // placeholder cinza-claro (a caixa do ícone) — só quando não é minúsculo demais.
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(x, y, w, h),
        color: 0xE8EAEDFF,
        radius: 2.0,
    });
    record_node_rect(list, id, Rect::new(x, y, w, h));
    Some((w, h))
}

fn layout_image(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let (handle, off, iw, ih) = dom.image_of(id)?;
    if handle == 0 || iw == 0 || ih == 0 {
        return None;
    }
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // margens (respeita o CSS); a imagem em si é o content (sem padding/borda v1).
    let m = &css.margin;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    // largura/altura: CSS explícito, senão o atributo HTML `width`/`height` (comum em
    // `<img width=100 height=100>`), senão o natural. Se só um é dado, mantém a razão.
    let attr_px = |name: &str| -> Option<f32> {
        dom.node(id)
            .attr(name)
            .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f32>().ok())
            .filter(|v| *v >= 0.0)
    };
    let css_w = css.width.and_then(|d| d.resolve(&resolve)).or_else(|| attr_px("width"));
    let css_h = css.height.and_then(|d| d.resolve(&resolve)).or_else(|| attr_px("height"));
    let (nat_w, nat_h) = (iw as f32, ih as f32);
    let (mut w, mut h) = match (css_w, css_h) {
        (Some(cw), Some(ch)) => (cw, ch),
        (Some(cw), None) => (cw, cw * nat_h / nat_w),
        (None, Some(ch)) => (ch * nat_w / nat_h, ch),
        (None, None) => (nat_w, nat_h),
    };
    // não estoura a largura disponível (encolhe mantendo a razão).
    let max_w = (avail_w - margin_left - margin_right).max(0.0);
    if w > max_w && w > 0.0 {
        h = h * max_w / w;
        w = max_w;
    }
    let rect = Rect::new(x + margin_left, y + margin_top, w, h);
    record_node_rect(list, id, rect);
    list.items.push(DisplayItem::Image {
        rect,
        pixels_handle: handle,
        pixels_off: off,
        img_w: iw,
        img_h: ih,
    });
    Some((w + margin_left + margin_right, h + margin_top + margin_bottom))
}

/// Layout de um `<canvas>`: a caixa dos atributos `width`/`height` (o padrão do
/// HTML é 300×150) ou do CSS, e o `DisplayItem::Pixels` quando há desenho.
///
/// Sem pixels a caixa é reservada e nada é pintado — um canvas em branco é um
/// canvas em branco, não um buraco no layout. É essa reserva que faz o resto da
/// página se dispor no lugar certo antes de o programa desenhar.
fn layout_canvas(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let attr_px = |name: &str| -> Option<f32> {
        dom.node(id)
            .attr(name)
            .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f32>().ok())
            .filter(|v| *v >= 0.0)
    };
    // 300×150 é o default do HTML para um canvas sem dimensões.
    let w = css.width.and_then(|d| d.resolve(&resolve)).or_else(|| attr_px("width")).unwrap_or(300.0);
    let h = css.height.and_then(|d| d.resolve(&resolve)).or_else(|| attr_px("height")).unwrap_or(150.0);
    let m = &css.margin;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    let rect = Rect::new(x + margin_left, y + margin_top, w, h);
    record_node_rect(list, id, rect);
    if let Some(color) = css.bg {
        list.items.push(DisplayItem::SolidRect { rect, color, radius: 0.0 });
    }
    if let Some((data, pw, ph)) = dom.pixel_data_of(id) {
        if pw > 0 && ph > 0 {
            list.items.push(DisplayItem::Pixels { rect, data, w: pw, h: ph });
        }
    }
    Some((w + margin_left + margin_right, h + margin_top + margin_bottom))
}

/// Layout de um `<input type=submit/button/reset>`: BOTÃO estilo UA — caixa
/// cinza-clara com borda e o `value` como rótulo (shrink-to-fit no texto). O CSS
/// do autor (bg/cor/padding) vence os defaults. Não editável, não focável (v1).
fn layout_button(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let font = font_px(css, DEFAULT_FONT_SIZE - 3.0);
    let label = dom.node(id).attr("value").unwrap_or("").to_string();
    let tw = ctx.measurer.text_width(&label, font, false, false);
    let lh = ctx.measurer.line_height(font);
    let (pad_h, pad_v) = (12.0, 5.0);
    let w = tw + 2.0 * pad_h;
    let h = lh + 2.0 * pad_v;
    let bg = css.bg.unwrap_or(0xF8F9FAFF); // cinza-claro UA (o do botão do google)
    let fg = css.color.unwrap_or(0x3C4043FF);
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(x, y, w, h),
        color: bg,
        radius: css.corner_radius.unwrap_or(4.0),
    });
    list.items.push(DisplayItem::Border {
        rect: Rect::new(x, y, w, h),
        width: css.border_width.unwrap_or(1.0),
        color: css.border_color.unwrap_or(0xDADCE0FF),
        radius: css.corner_radius.unwrap_or(4.0),
    });
    list.items.push(DisplayItem::Text {
        x: x + pad_h,
        y: y + pad_v,
        text: label.into(),
        color: fg,
        size: font,
        mono: false,
        bold: false,
        letter_spacing: 0.0,
        decoration: 0,
    });
    record_node_rect(list, id, Rect::new(x, y, w, h));
    (w + 6.0, h + 4.0) // margenzinha UA entre botões
}

/// O lado do quadrado de um `checkbox`/`radio` sem tamanho declarado. 13px é o
/// intrínseco que os browsers dão a estes controlos; não sai de fonte nenhuma,
/// por isso é uma constante e não uma medida.
const CAIXA_DE_MARCA: f32 = 13.0;

/// A caixa de um `<input>` de texto/marca: `(outer_w, outer_h)` e o frame com
/// que ela foi construída.
///
/// Existe porque a medida estava em DOIS sítios: o `layout_input`, que pinta, e
/// o `inline_widget_size`, que reserva o espaço na linha. O segundo dizia
/// espelhar o primeiro e não espelhava — um `checkbox` reservava 190x26 (um
/// campo de texto) e pintava outra coisa. Uma pergunta, uma resposta.
fn medida_do_input(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    ctx: &LayoutCtx,
) -> MedidaDoInput {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let m = &css.margin;
    let p = &css.padding;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    // CHECKBOX e RADIO são REPLACED: a caixa é um quadradinho de tamanho
    // intrínseco, não um campo de texto. E não levam o padding/borda com que a
    // UA veste um campo — no browser são 13x13 e mais nada, por isso os defaults
    // do frame são ZERO para eles (o CSS do autor continua a mandar).
    let quadrado = matches!(
        dom.node(id).attr("type").map(|t| t.to_ascii_lowercase()).as_deref(),
        Some("checkbox") | Some("radio")
    );
    let (pad_ua_h, pad_ua_v, borda_ua) = if quadrado { (0.0, 0.0, 0.0) } else { (4.0, 3.0, 1.0) };
    let pad_left = p.left.resolve(&resolve).unwrap_or(pad_ua_h).max(0.0);
    let pad_right = p.right.resolve(&resolve).unwrap_or(pad_ua_h).max(0.0);
    let pad_top = p.top.resolve(&resolve).unwrap_or(pad_ua_v).max(0.0);
    let pad_bottom = p.bottom.resolve(&resolve).unwrap_or(pad_ua_v).max(0.0);
    let border = css.border_width.unwrap_or(borda_ua).max(0.0);
    let padding_h = pad_left + pad_right;
    let frame = margin_left + margin_right + 2.0 * border + padding_h;
    let border_box = css.border_box.unwrap_or(false);
    let content_w = if let Some(fw) = forced_outer_w {
        (fw - frame).max(0.0)
    } else if let Some(w) = css.width.and_then(|d| d.resolve(&resolve)) {
        if border_box { (w - (padding_h + 2.0 * border)).max(0.0) } else { w }
    } else if quadrado {
        CAIXA_DE_MARCA
    } else {
        180.0_f32.min((avail_w - frame).max(0.0))
    };
    // `resolve_height` e não `resolve`: uma percentagem no eixo VERTICAL mede-se
    // contra a altura do containing block. Com o `resolve` genérico media-se
    // contra a LARGURA — os `<input type=checkbox>` do "checkbox hack" da
    // Wikipédia declaram `height:100%` e vinham com a largura da viewport de
    // altura, oito deles, o pior rácio de erro da página inteira.
    let content_h = resolve_height(css.height, avail_h, &resolve)
        .map(|h| if border_box { (h - (pad_top + pad_bottom + 2.0 * border)).max(0.0) } else { h })
        .unwrap_or(if quadrado { CAIXA_DE_MARCA } else { ctx.measurer.line_height(font) });
    MedidaDoInput {
        content_w,
        content_h,
        pad_left,
        pad_top,
        padding_v: pad_top + pad_bottom,
        padding_h,
        border,
        margin_left,
        margin_top,
        margin_h: margin_left + margin_right,
        margin_v: margin_top + margin_bottom,
        font,
    }
}

/// O que `medida_do_input` responde: a caixa e o frame com que foi construída.
struct MedidaDoInput {
    content_w: f32,
    content_h: f32,
    pad_left: f32,
    pad_top: f32,
    padding_v: f32,
    padding_h: f32,
    border: f32,
    margin_left: f32,
    margin_top: f32,
    margin_h: f32,
    margin_v: f32,
    font: f32,
}

impl MedidaDoInput {
    /// A caixa EXTERNA (com margens) — o que o fluxo reserva para o widget.
    fn outer(&self) -> (f32, f32) {
        (
            self.content_w + self.padding_h + 2.0 * self.border + self.margin_h,
            self.content_h + self.padding_v + 2.0 * self.border + self.margin_v,
        )
    }
}

fn layout_input(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    // Altura do containing block, para `height: %`. `None` = pai com altura auto,
    // e aí a percentagem vale `auto` — a mesma regra do `layout_block`.
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let med = medida_do_input(dom, id, css, avail_w, avail_h, forced_outer_w, ctx);
    let MedidaDoInput { content_w, content_h, pad_left, pad_top, padding_v, padding_h,
        border, margin_left, margin_top, margin_h, margin_v, font } = med;
    let pad_bottom = padding_v - pad_top;
    let margin_right = margin_h - margin_left;
    let margin_bottom = margin_v - margin_top;
    let line_h = ctx.measurer.line_height(font);
    let _ = (pad_bottom, margin_right, margin_bottom, line_h);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let _ = &resolve;
    let box_rect = Rect::new(
        x + margin_left,
        y + margin_top,
        content_w + padding_h + 2.0 * border,
        content_h + pad_top + pad_bottom + 2.0 * border,
    );
    record_node_rect(list, id, box_rect);

    // Fundo: o `background` do CSS, senão branco (campo de texto clássico).
    let radius = css.corner_radius.unwrap_or(0.0);
    // A OPACIDADE também vale aqui. Este era o único sítio que emite caixa sem
    // passar por `apply_opacity`, e o preço foi uma página inteira em branco: a
    // Wikipédia usa o "checkbox hack" — `<input type=checkbox>` com
    // `opacity: 0`, dimensionado à altura da página, para abrir menus sem
    // JavaScript. Oito deles, com fundo branco opaco e borda cinzenta, pintados
    // depois de tudo o resto. O layout estava certo, a lista de pintura estava
    // certa, e o que se via era o fundo de um controlo invisível.
    //
    // `unwrap_or(0xFFFFFFFF)` é o fundo que a UA dá a um campo de texto, e um
    // campo com `opacity: 0` não o pinta.
    let opacidade = css.opacity.unwrap_or(1.0);
    let bg = apply_opacity(css.bg.unwrap_or(0xFFFFFFFF), opacidade);
    list.items.push(DisplayItem::SolidRect { rect: box_rect, color: bg, radius });
    // Borda: sempre desenha (o input tem contorno por padrão). Cor do CSS ou cinza.
    // Se o campo tem foco, realça a borda (azul), como o browser.
    let focused = dom.focused_input() == Some(id);
    let border_color = if focused {
        0x3B82F6FF // azul de foco
    } else {
        css.border_color.unwrap_or(0x9AA0A6FF)
    };
    let border_color = apply_opacity(border_color, opacidade);
    let bw = if border > 0.0 { border } else { 1.0 };
    list.items.push(DisplayItem::Border { rect: box_rect, width: bw, color: border_color, radius });

    // Texto: o valor digitado, ou o placeholder apagado. Posicionado no content-box.
    let text_x = x + margin_left + bw + pad_left;
    let text_y = y + margin_top + bw + pad_top;
    let (shown, tcolor) = if dom.input_is_empty(id) {
        let ph = dom.node(id).attr("placeholder").unwrap_or("").to_string();
        (ph, 0x9AA0A6FF) // cinza apagado
    } else {
        (dom.input_value(id), css.color.unwrap_or(0x111111FF))
    };
    if !shown.is_empty() {
        list.items.push(DisplayItem::Text {
            x: text_x,
            y: text_y,
            text: shown.as_str().into(),
            color: tcolor,
            size: font,
            mono: false,
            bold: false,
            letter_spacing: 0.0,
            decoration: 0,
        });
    }
    // Cursor: barrinha vertical após o texto do VALOR (não do placeholder), só com foco.
    if focused {
        let val = dom.input_value(id);
        let caret_x = text_x + ctx.measurer.text_width(&val, font, false, false) + 1.0;
        let caret = Rect::new(caret_x, text_y, 1.5, line_h.min(content_h.max(line_h)));
        list.items.push(DisplayItem::SolidRect { rect: caret, color: 0x111111FF, radius: 0.0 });
    }

    (
        box_rect.w + margin_left + margin_right,
        box_rect.h + margin_top + margin_bottom,
    )
}

/// `true` se a tag NÃO é renderável — metadata do documento (`<head>` e o que vive
/// nele: `<title>`, `<meta>`, `<link>`, `<base>`) e os recursos `<style>`/`<script>`
/// (o CSS já virou stylesheet no parse; JS não executamos). Permite carregar um HTML
/// COMPLETO e pintar só o conteúdo visível (`<body>`). `<html>`/`<body>` SÃO
/// renderáveis (transparentes — fluxo block normal dos filhos).
pub(crate) fn is_non_rendered_tag(tag: &str) -> bool {
    matches!(tag, "head" | "title" | "meta" | "link" | "base" | "style" | "script")
}

/// O código de `display` de um nó: o CSS (`display:` parseado) VENCE; se não
/// declarado, cai no default da tag (`block::lookup`, a UA-stylesheet via
/// defineBlock); senão vertical. É o eixo de empilhamento dos filhos.
/// Códigos: 0=vertical/block, 1=wrap, 2=horizontal/flex, -1=none.
fn css_display(dom: &Dom, id: NodeIdx) -> i64 {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // 1) CSS explícito (display:flex/block/inline/none) tem prioridade.
            if let Some(css) = dom.computed_style_idx(id) {
                if let Some(kind) = css.effective_display() {
                    return kind.to_display_code();
                }
            }
            // 2) default da tag: defineBlock (UA-stylesheet do TS) tem prioridade;
            // senão as tags HTML block conhecidas (div/p/…) são vertical; o resto
            // também cai em vertical (default seguro para um container).
            crate::block::lookup(tag).map(|d| d.display).unwrap_or(crate::block::DISPLAY_VERTICAL)
        }
        _ => crate::block::DISPLAY_VERTICAL,
    }
}

/// O `display` USADO de um nó: o do CSS de autor, senão o default da UA para a
/// tag ([`crate::block::ua_display`]), senão `None` — e `None` aqui significa
/// "o fluxo de bloco genérico decide", não "sem display".
///
/// Existe ao lado de [`css_display`] e não em vez dele porque as duas respondem
/// a perguntas diferentes: aquela dá o EIXO em que os filhos empilham (um `i64`
/// que o TS também escreve), esta dá o PAPEL da caixa. Um `<tr>` tem eixo
/// vertical e papel de linha de tabela, e só a segunda pergunta o distingue de
/// um `<div>`.
/// O recuo default que uma caixa de lista (`<ul>`/`<ol>`) dá aos seus itens, ou
/// 0 quando o autor declarou o seu próprio `padding-left`.
fn ua_list_indent(dom: &Dom, id: NodeIdx, p: &crate::style::Edges) -> f32 {
    if p.left != crate::style::Side::Unset {
        return 0.0;
    }
    match &dom.node(id).kind {
        NodeKind::Element { tag } if crate::block::is_list_container(tag) => {
            crate::block::UA_LIST_INDENT
        }
        _ => 0.0,
    }
}

pub(crate) fn used_display(dom: &Dom, id: NodeIdx) -> Option<crate::style::DisplayKind> {
    let NodeKind::Element { tag } = &dom.node(id).kind else { return None };
    if let Some(css) = dom.computed_style_idx(id) {
        if let Some(k) = css.effective_display() {
            return Some(k);
        }
    }
    crate::block::ua_display(tag)
}

/// O `font-size` COMPUTADO em pontos: a CASCADE já resolveu a forma para `Px`
/// (dom.rs — base de em/% é o pai; rem/vw/vh contra root/viewport), então aqui é
/// só extrair; `fallback` cobre o não-declarado (herda) e formas não-resolvidas.
pub(crate) fn font_px(css: &ComputedStyle, fallback: f32) -> f32 {
    match css.font_size {
        Some(crate::style::Dimension::Px(v)) => v,
        _ => fallback,
    }
}

/// Resolve uma dimensão do EIXO VERTICAL (`height`/`min-height`/`max-height`):
/// `%` resolve contra a ALTURA do containing block (não a largura — era o bug que
/// fazia `height:100%` virar 100% da largura do pai); as demais unidades usam o
/// ctx normal. `avail_h = None` (pai com altura auto) → `%` vira auto (`None`),
/// fiel ao browser.
fn resolve_height(
    d: Option<crate::style::Dimension>,
    avail_h: Option<f32>,
    ctx: &ResolveCtx,
) -> Option<f32> {
    match d? {
        crate::style::Dimension::Percent(p) => avail_h.map(|h| (h * p / 100.0).max(0.0)),
        // `calc(...)` num contexto de ALTURA: o componente `%` resolve contra a
        // ALTURA do containing block (avail_h), NÃO a largura — o `resolve`
        // genérico usa parent_content_w e daria `calc(100% - 560px)` = 1000-560
        // (largura) em vez de 800-560 (altura). Reconstrói a soma no eixo certo.
        crate::style::Dimension::Calc(c) => {
            let h = avail_h?;
            let v = c.px + h * c.pct / 100.0
                + ctx.node_font_size * c.em
                + ctx.root_font_size * c.rem
                + ctx.viewport_w * c.vw / 100.0
                + ctx.viewport_h * c.vh / 100.0;
            Some(v.max(0.0))
        }
        other => other.resolve(ctx),
    }
}

/// `true` se este filho tem TEXTO (ou um inline puro) por vizinho — isto é, se
/// está dentro de uma linha em vez de estar sozinho entre blocos.
///
/// Serve para decidir se um inline-block flui na linha ou abre corrida própria.
/// A pergunta é a mesma que o whitespace faz, com um vizinho a mais: o texto
/// pode estar antes OU depois, e um `<span>` com fundo no fim de um parágrafo
/// pertence à linha do texto que o antecede.
fn em_contexto_inline(dom: &Dom, parent: NodeIdx, child: NodeIdx) -> bool {
    let irmaos = &dom.node(parent).children;
    let Some(pos) = irmaos.iter().position(|&c| c == child) else {
        return false;
    };
    let e_inline = |idx: NodeIdx| match &dom.node(idx).kind {
        NodeKind::Text(t) => !t.trim().is_empty(),
        NodeKind::Element { tag } => {
            !is_non_rendered_tag(tag)
                && !is_out_of_flow(dom, idx)
                && !is_block_level(dom, idx)
                && !is_inline_block(dom, idx)
        }
        _ => false,
    };
    irmaos[..pos].iter().rev().any(|&c| e_inline(c)) || irmaos[pos + 1..].iter().any(|&c| e_inline(c))
}

/// Retorna se um whitespace entre irmãos deve participar do contexto inline. O
/// parser preserva o nó de texto por fidelidade ao DOM, mas whitespace entre dois
/// blocos/floats não cria uma linha visual; whitespace adjacente a texto/inline sim.
fn whitespace_is_inline_separator(dom: &Dom, parent: NodeIdx, child: NodeIdx) -> bool {
    let children = &dom.node(parent).children;
    let Some(pos) = children.iter().position(|&c| c == child) else { return false };
    let is_inline_candidate = |idx: NodeIdx| match &dom.node(idx).kind {
        NodeKind::Text(t) => !t.trim().is_empty(),
        NodeKind::Element { tag } => {
            !is_non_rendered_tag(tag)
                && !is_block_level(dom, idx)
                && !is_inline_block(dom, idx)
                && float_of(dom, idx) == crate::style::FloatSide::None
        }
        _ => false,
    };
    let previous = children[..pos]
        .iter()
        .rev()
        .copied()
        .find(|&idx| !matches!(&dom.node(idx).kind, NodeKind::Text(t) if t.trim().is_empty()));
    let next = children[pos + 1..]
        .iter()
        .copied()
        .find(|&idx| !matches!(&dom.node(idx).kind, NodeKind::Text(t) if t.trim().is_empty()));
    previous.is_some_and(is_inline_candidate) || next.is_some_and(is_inline_candidate)
}

/// Empilha os filhos VERTICAL (cada um abaixo do anterior), ocupando a largura do
/// content. Devolve a altura TOTAL do content (soma das alturas dos filhos).
/// `avail_h` = altura do content DESTE container quando explícita (containing
/// block dos filhos p/ `height:%`).
// as macros de estado (close_floats!/flush_inline!) resetam as variáveis a cada
// fechamento — a ÚLTIMA atribuição (no flush final) é estruturalmente morta, o
// que dispara unused_assignments sem haver bug.
/// As duas margens adjacentes colapsadas numa só, pela regra do CSS 2.1 §8.3.1.
///
/// Não é `max(a, b)`: essa é a regra apenas quando as DUAS são positivas.
/// - as duas ≥ 0 → a maior;
/// - as duas < 0 → a mais negativa (a que puxa mais);
/// - uma de cada sinal → a SOMA, e é por isso que uma margem negativa cancela
///   uma positiva em vez de ser ignorada por ela.
fn colapso_de_margens(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a < 0.0 && b < 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

/// Quanto é que a soma das duas margens excede a colapsada — o que há a
/// descontar ao cursor, já que cada bloco traz a sua margem dentro da altura.
///
/// A forma antiga era `min(a, b)`, que dá o mesmo resultado enquanto as duas
/// forem positivas e o resultado ERRADO assim que uma é negativa: com `a = 0` e
/// `b = -10px` descontava −10, ou seja SOMAVA 10 ao cursor, e a margem negativa
/// que devia puxar o bloco para cima empurrava-o para baixo. É o `margin-top`
/// negativo dos gutters `.row` do Bootstrap, e a razão de um teste que o pinava
/// ter começado a falhar.
fn excesso_de_margens(a: f32, b: f32) -> f32 {
    a + b - colapso_de_margens(a, b)
}

#[allow(unused_assignments)]
fn layout_children_vertical(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut child_y = content_y;
    // A base da chave de fragmento é a mesma para todos os filhos deste
    // container — só o nó e o epoch dele mudam.
    let key_base = KeyBase::new(dom, content_w, avail_h, ctx);
    // MARGIN-COLLAPSE: as margens verticais de blocos ADJACENTES colapsam numa
    // só, não somam. Como o `outer_h` de cada bloco já inclui a sua margem dos
    // dois lados, ao empilhar dois blocos a soma conta
    // `margin_bottom_anterior + margin_top_atual`; subtrai-se o excesso para
    // ficar a colapsada ([`colapso_de_margens`]). `prev_margin` guarda a margem
    // do último bloco posto.
    let mut prev_margin = 0.0f32;
    // ── FLOAT LINE (v1): floats CONSECUTIVOS dividem a mesma linha — left encosta
    // à esquerda, right à direita (o header brand+nav do Bootstrap). Um irmão
    // NÃO-float fecha a linha (começa abaixo do float mais alto). Ver FloatSide.
    let mut float_top: Option<f32> = None; // y do topo da linha de floats
    let mut float_left_x = content_x;
    let mut float_right_x = content_x + content_w;
    let mut float_h = 0.0f32; // altura da linha (max dos floats)
    // fecha a float line corrente (chamado antes de um não-float e no fim).
    macro_rules! close_floats {
        ($y:expr) => {
            if let Some(top) = float_top.take() {
                $y = $y.max(top + float_h);
                float_left_x = content_x;
                float_right_x = content_x + content_w;
                float_h = 0.0;
            }
        };
    }
    // ── CONTEXTO INLINE (P4): irmãos inline CONSECUTIVOS (texto + <a>/<b>/<span>)
    // fluem JUNTOS numa sequência de linhas — acumulados aqui e descarregados por
    // `flush_inline!` quando um bloco/float/fim interrompe o fluxo.
    let mut inline_group: Vec<NodeIdx> = Vec::new();
    // Corrida de INLINE-BLOCKS consecutivos (botões/pills lado a lado). Pintada
    // por `flush_ib` — mede cada um (shrink), põe lado a lado quebrando linha ao
    // encher, e alinha a linha pelo text-align do pai (center do google).
    let mut ib_run: Vec<NodeIdx> = Vec::new();
    macro_rules! flush_ib {
        ($y:expr) => {
            if !ib_run.is_empty() {
                $y = layout_inline_block_line(
                    dom, &ib_run, content_x, $y, content_w, avail_h, css, ctx, list,
                );
                ib_run.clear();
                prev_margin = 0.0;
            }
        };
    }
    macro_rules! flush_inline {
        ($y:expr) => {
            if !ib_run.is_empty() {
                flush_ib!($y);
            }
            if !inline_group.is_empty() {
                close_floats!($y);
                $y = layout_inline_flow(
                    dom, id, &inline_group, content_x, $y, content_w, css, font_size, ctx, list,
                );
                inline_group.clear();
                prev_margin = 0.0; // texto quebra a sequência de margin-collapse
            }
        };
    }
    for &child in &dom.node(id).children {
        // CAMINHO RÁPIDO: se existe fragmento para este filho com estas
        // constraints, ele já foi classificado como BLOCO NORMAL quando foi
        // criado — é o único caminho que produz fragmento. Encontrá-lo responde
        // a classificação inteira, que custaria estilo computado,
        // `block::lookup` e a margem resolvida por filho: mil vezes por frame
        // numa lista, para redescobrir o que não mudou.
        if matches!(dom.node(child).kind, NodeKind::Element { .. }) {
            let key = key_base.key(dom, child);
            if let Some(fragment) = dom.fragment_get(key) {
                crate::bump!(fragment_hits);
                flush_inline!(child_y);
                close_floats!(child_y);
                child_y -= excesso_de_margens(prev_margin, fragment.margin_top);
                emit_fragment(&fragment, list, content_x, child_y, content_w, avail_h);
                child_y += fragment.size.1;
                prev_margin = fragment.margin_top;
                continue;
            }
        }
        let child_css = match &dom.node(child).kind {
            NodeKind::Element { .. } => Some(dom.computed_style_idx(child).unwrap_or_default()),
            _ => None,
        };
        let child_out = child_css
            .as_ref()
            .and_then(|c| c.position)
            .map(|p| p.out_of_flow())
            .unwrap_or(false);
        let child_float = child_css
            .as_ref()
            .and_then(|c| c.float_side)
            .unwrap_or(crate::style::FloatSide::None);
        // `clear` — o par do `float`: este filho começa ABAIXO dos floats
        // correntes. Fica ANTES do dispatch por tipo de caixa porque vale para
        // qualquer um deles: o caminho de bloco já fechava a linha de floats
        // sempre, mas um inline-block ou um texto com `clear` não fechava nada e
        // acabava por cima do float. Os três valores agem como `both` (ver
        // `style::text::Clear` para porquê).
        if child_css.as_ref().and_then(|c| c.clear).map(|c| c.clears()).unwrap_or(false) {
            flush_inline!(child_y);
            close_floats!(child_y);
        }
        let (child_block, child_inline_block) = match &dom.node(child).kind {
            NodeKind::Element { tag } => {
                let replaced = (tag == "img" && dom.image_of(child).is_some())
                    || tag == "svg"
                    || tag == "canvas";
                let effective = child_css.as_ref().and_then(|c| c.effective_display());
                let explicit_block = effective
                    .map(|d| d != crate::style::DisplayKind::Inline)
                    .unwrap_or(false);
                let block = replaced
                    || effective.is_some()
                    || crate::block::lookup(tag).is_some()
                    || child_css.as_ref().map(|c| c.has_box() || c.height.is_some()).unwrap_or(false);
                let inline_block = if matches!(tag.as_str(), "input" | "button" | "select" | "textarea") {
                    !explicit_block
                } else if crate::block::lookup(tag).is_some() || explicit_block {
                    false
                } else {
                    child_css.as_ref().map(|c| c.has_box() || c.height.is_some()).unwrap_or(false)
                };
                (block, inline_block)
            }
            _ => (false, false),
        };
        match &dom.node(child).kind {
            // Metadata não-renderável (`<head>`/`<title>`/`<style>`/`<script>`):
            // pula — NÃO coleta seu texto como inline (senão o título e o CSS cru
            // vazam pra tela). Checado ANTES do caminho inline.
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            // Fora do fluxo (`position:absolute/fixed`): não ocupa espaço aqui —
            // pintado na passada out-of-flow de layout_document.
            NodeKind::Element { .. } if child_out => {}
            // FLOAT left/right: shrink-to-fit na linha de floats corrente.
            NodeKind::Element { .. } if child_float != crate::style::FloatSide::None => {
                flush_inline!(child_y);
                let side = child_float;
                let top = *float_top.get_or_insert(child_y);
                let w = child_outer_width(dom, child, content_w, font_size, ctx);
                let h = child_outer_height(dom, child, content_w, avail_h, css, font_size, ctx);
                let x = if side == crate::style::FloatSide::Left {
                    let x = float_left_x;
                    float_left_x += w;
                    x
                } else {
                    float_right_x -= w;
                    float_right_x
                };
                layout_block(dom, child, x, top, content_w, avail_h, None, None, true, ctx, list);
                float_h = float_h.max(h);
                prev_margin = 0.0; // float quebra a sequência de collapse
            }
            NodeKind::Element { .. } if child_block && !child_inline_block => {
                flush_inline!(child_y);
                close_floats!(child_y);
                // margin VERTICAL TOP do filho (para o collapse com o anterior):
                // margin.top + margin_v da UA.
                let m = child_css.as_ref().map(|c| {
                        // margem TOP do filho p/ o collapse (unidades relativas
                        // resolvem contra o content deste container).
                        let r = ResolveCtx {
                            parent_content_w: content_w,
                            node_font_size: font_px(&c, font_size),
                            root_font_size: DEFAULT_FONT_SIZE,
                            viewport_w: ctx.viewport_w,
                            viewport_h: ctx.viewport_h,
                        };
                        let mv = if c.margin.top == crate::style::Side::Unset {
                            c.margin_v.unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        c.margin.top.resolve(&r).unwrap_or(0.0) + mv
                    })
                    .unwrap_or(0.0);
                // Colapsa com o bloco anterior: recua o overlap antes de posicionar.
                child_y -= excesso_de_margens(prev_margin, m);
                let ((_, h), _) = layout_block_reusing(
                    dom, child, content_x, child_y, content_w, avail_h, || m, ctx, list,
                );
                child_y += h;
                prev_margin = m;
            }
            // INLINE-BLOCK (pill/botão solto): NÃO pinta agora — acumula na
            // "linha de inline-blocks" corrente (irmãos consecutivos fluem LADO A
            // LADO, quebrando quando enche). Os botões 'Pesquisa Google'/'Estou
            // com sorte' do google são 2 inline-block irmãos que compartilham a
            // linha. Um texto/inline entre eles fecha a corrida (flush_inline).
            // Um inline-block RODEADO DE TEXTO é conteúdo de linha, não uma
            // corrida própria: entra no grupo inline e o `wrap_runs` trata-o como
            // palavra inquebrável. A corrida (`ib_run`) fica para o que ela
            // existe — inline-blocks IRMÃOS sem texto à volta, os botões do
            // google. Sem esta distinção um `<span>` com fundo no meio de um
            // parágrafo fechava o fluxo e abria linha nova.
            NodeKind::Element { .. } if child_inline_block && em_contexto_inline(dom, id, child) => {
                flush_ib!(child_y);
                inline_group.push(child);
            }
            NodeKind::Element { .. } if child_inline_block => {
                // descarrega só o TEXTO inline pendente (não o ib_run — este b
                // continua a acumular os inline-blocks IRMÃOS na mesma corrida).
                if !inline_group.is_empty() {
                    close_floats!(child_y);
                    child_y = layout_inline_flow(
                        dom, id, &inline_group, content_x, child_y, content_w, css, font_size, ctx, list,
                    );
                    inline_group.clear();
                }
                ib_run.push(child);
                prev_margin = 0.0;
            }
            // Whitespace estrutural continua no DOM, mas não cria uma linha entre
            // blocos/floats. Quando está perto de texto/inline, entra no grupo e o
            // `wrap_runs` o colapsa como um espaço normal.
            NodeKind::Text(t)
                if t.trim().is_empty() && !whitespace_is_inline_separator(dom, id, child) => {}
            // Texto / elemento inline: entra no CONTEXTO INLINE corrente — flui
            // com os irmãos inline adjacentes (o flush pinta o grupo inteiro).
            _ => {
                flush_ib!(child_y); // fecha a corrida de inline-blocks
                inline_group.push(child);
            }
        }
    }
    // descarrega o fluxo inline pendente e fecha a float line: os floats
    // CONTRIBUEM para a altura (v1 = comportamento de BFC — correto p/ flex
    // items, o caso do header do cover).
    flush_inline!(child_y);
    close_floats!(child_y);
    (child_y - content_y).max(0.0)
}

/// Pinta uma CORRIDA de inline-blocks consecutivos (botões/pills irmãos) numa
/// sequência de linhas horizontais: mede cada um (shrink, numa lista descartável),
/// põe lado a lado enquanto cabe na `content_w`, quebra linha quando enche, e
/// alinha CADA linha pelo `text-align` do pai (center do google centra os botões).
/// Devolve o novo `y` (abaixo da última linha). Vazio → devolve `y`.
#[allow(clippy::too_many_arguments)]
fn layout_inline_block_line(
    dom: &Dom,
    run: &[NodeIdx],
    content_x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    parent_css: &ComputedStyle,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // 1) mede a largura+altura desejada (shrink) de cada item numa lista descartável.
    let mut sizes: Vec<(NodeIdx, f32, f32)> = Vec::with_capacity(run.len());
    for &child in run {
        let (w, h) = measure_block(dom, child, content_w, avail_h, None, None, true, ctx);
        sizes.push((child, w, h));
    }
    // 2) agrupa em LINHAS (soma das larguras ≤ content_w). Cada linha guarda os
    //    itens + a largura total (p/ o alinhamento).
    let mut lines: Vec<(Vec<(NodeIdx, f32, f32)>, f32)> = Vec::new();
    let mut cur: Vec<(NodeIdx, f32, f32)> = Vec::new();
    let mut cur_w = 0.0f32;
    for (child, w, h) in sizes {
        if !cur.is_empty() && cur_w + w > content_w {
            lines.push((std::mem::take(&mut cur), cur_w));
            cur_w = 0.0;
        }
        cur_w += w;
        cur.push((child, w, h));
    }
    if !cur.is_empty() {
        lines.push((cur, cur_w));
    }
    // 3) pinta cada linha: x inicial pelo text-align do pai, itens lado a lado;
    //    y avança pela ALTURA da linha (o item mais alto).
    let mut cy = y;
    for (items, line_w) in &lines {
        let free = (content_w - line_w).max(0.0);
        let mut x = match parent_css.text_align {
            Some(crate::style::TextAlign::Center) => content_x + free / 2.0,
            Some(crate::style::TextAlign::Right) => content_x + free,
            _ => content_x,
        };
        // `line_h` tem de ser conhecida ANTES de posicionar, porque é contra ela
        // que o `vertical-align` alinha — daí a passada de altura separada.
        let line_h = items.iter().fold(0.0f32, |acc, &(_, _, h)| acc.max(h));
        for &(child, w, h) in items {
            // `vertical-align`: a caixa desce dentro da altura da linha. O default
            // (`baseline`, e o não-declarado) mantém o topo, que é o que este
            // motor sempre fez — ver o corte em `style::text::VerticalAlign`.
            let dy = match dom.computed_style_idx(child).and_then(|c| c.vertical_align) {
                Some(crate::style::VerticalAlign::Middle) => (line_h - h) / 2.0,
                Some(crate::style::VerticalAlign::Bottom) => line_h - h,
                _ => 0.0,
            };
            layout_block(dom, child, x, cy + dy, content_w, avail_h, None, None, true, ctx, list);
            x += w;
        }
        cy += line_h;
    }
    cy
}

/// Os itens de BORDA de uma caixa: a moldura uniforme, as barras por lado, e o
/// `outline`. Uma função só porque a lista é emitida num sítio e CONTADA noutro
/// (o índice onde o clip de scroll começa) — duas regras para a mesma lista foi o
/// que já dessincronizou o clip antes.
///
/// Duas formas, e a escolha é por fidelidade:
/// - Sem nada declarado por lado, sai UM `DisplayItem::Border` — o caminho que já
///   existia, e o único que respeita o `border-radius` (o backend desenha a
///   moldura arredondada).
/// - Com um lado declarado (`border-bottom: 1px solid #ccc`, o separador de 17
///   ocorrências na folha da Wikipédia), sai uma BARRA por lado visível, como
///   `SolidRect`. Emitir a moldura uniforme neste caso desenharia os quatro lados
///   onde a página pediu um: errado de forma mais visível do que ignorar.
///
/// ⚠️ CORTE declarado: a largura por lado NÃO entra na geometria da caixa — o box
/// model do motor tem um `border` escalar, usado em ~15 sítios (content_w,
/// content_x, outer_h, measure, flex basis…). Uma `border-bottom: 1px` pinta a
/// barra mas não empurra o conteúdo 1px para cima. Trocar o escalar por quatro
/// valores é uma mudança de box model, não desta propriedade.
///
/// O `outline` sai por último (por cima) e por FORA do border-box, inflado pelo
/// `outline-offset` — é o que o distingue da borda: não ocupa espaço nenhum.
pub(crate) fn border_items(css: &ComputedStyle, box_rect: Rect, radius: f32, op: f32) -> Vec<DisplayItem> {
    let mut out = Vec::new();
    let sides = crate::style::borders::resolved_sides(css);
    if crate::style::borders::has_per_side(css) {
        let (x, y, w, h) = (box_rect.x, box_rect.y, box_rect.w, box_rect.h);
        // top, right, bottom, left — a ordem de `resolved_sides`. Cada barra ocupa
        // a aresta INTEIRA; os cantos ficam sobrepostos em vez de mitrados, que é
        // invisível enquanto as cores dos lados adjacentes coincidem e é o que um
        // separador (um lado só) precisa.
        let bars = [
            (Rect::new(x, y, w, sides[0].width), sides[0]),
            (Rect::new(x + w - sides[1].width, y, sides[1].width, h), sides[1]),
            (Rect::new(x, y + h - sides[2].width, w, sides[2].width), sides[2]),
            (Rect::new(x, y, sides[3].width, h), sides[3]),
        ];
        for (rect, side) in bars {
            if side.paints() {
                out.push(DisplayItem::SolidRect {
                    rect,
                    color: apply_opacity(side.color, op),
                    radius: 0.0,
                });
            }
        }
    } else {
        // A borda uniforme só pinta se tem largura E um `border-style` VISÍVEL. O
        // default CSS de border-style é `none` → sem `border-style` declarado, NÃO
        // pinta (fiel ao Chrome: `border-width:2px` sozinho dá borda invisível).
        if sides[0].paints() {
            out.push(DisplayItem::Border {
                rect: box_rect,
                width: sides[0].width,
                color: apply_opacity(sides[0].color, op),
                radius,
            });
        }
    }
    let ow = css.outline_width.unwrap_or(0.0);
    let visible = css.outline_style.map(|s| s.is_visible()).unwrap_or(false);
    if ow > 0.0 && visible {
        let off = css.outline_offset.unwrap_or(0.0) + ow / 2.0;
        out.push(DisplayItem::Border {
            rect: Rect::new(
                box_rect.x - off,
                box_rect.y - off,
                box_rect.w + 2.0 * off,
                box_rect.h + 2.0 * off,
            ),
            width: ow,
            // `outline-color` ausente = `currentColor` (a cor do texto).
            color: apply_opacity(css.outline_color.or(css.color).unwrap_or(0x000000FF), op),
            // O outline é sempre RETANGULAR aqui (o Chrome moderno segue o
            // border-radius) — ver `style::borders`.
            radius: 0.0,
        });
    }
    out
}

/// O `float` computado de um nó-elemento (None p/ não-elemento/sem estilo).
fn float_of(dom: &Dom, id: NodeIdx) -> crate::style::FloatSide {
    dom.computed_style_idx(id)
        .and_then(|c| c.float_side)
        .unwrap_or(crate::style::FloatSide::None)
}

/// Um item do flex (pré-pass), com a BASE no eixo principal (flex-basis/width/
/// conteúdo, outer com margem), o MAIN size final (após grow/shrink) e os
/// fatores de flexibilidade lidos do estilo.
struct FlexItem {
    node: NodeIdx,
    /// tamanho BASE outer no eixo principal (antes de grow/shrink).
    base: f32,
    /// main size FINAL outer (após grow/shrink) — começa igual à base.
    main: f32,
    /// altura outer (cross) — re-medida com o main final quando ele muda.
    h: f32,
    /// `true` se é um nó de texto solto (pintado direto, não via layout_block).
    is_text: bool,
    /// `flex-grow` (0 = não cresce).
    grow: f32,
    /// `flex-shrink` (1 = default do CSS; texto solto não encolhe).
    shrink: f32,
    /// `align-self` do item (None = usa o align-items do container).
    align_self: Option<crate::style::AlignItems>,
    /// `order` (menor primeiro; empate = ordem do documento — sort estável).
    order: i32,
    /// o item PODE ser esticado pelo stretch (sem `height` explícito).
    can_stretch: bool,
}

/// A BASE outer de um item flex no eixo principal: `flex-basis` explícita
/// (resolvida como o width — respeita box-sizing) + margens; `auto`/ausente →
/// width/conteúdo ([`child_outer_width`]). O `.col` do Bootstrap tem basis `0%`
/// → a base é só o frame (e o grow distribui o espaço).
fn flex_base_outer(dom: &Dom, id: NodeIdx, container_w: f32, parent_font: f32, ctx: &LayoutCtx) -> f32 {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let basis = css.flex_basis.and_then(|d| match d {
        crate::style::Dimension::Auto => None,
        other => other.resolve(&resolve),
    });
    let Some(basis) = basis else {
        return child_outer_width(dom, id, container_w, parent_font, ctx);
    };
    let margin_h = css.margin.resolve_h(&resolve);
    if css.border_box.unwrap_or(false) {
        basis + margin_h // border-box: a basis JÁ é a caixa (pad+borda inclusos)
    } else {
        basis
            + margin_h
            + 2.0 * css.border_width.unwrap_or(0.0)
            + css.padding.resolve_h(&resolve)
    }
}

/// Dispõe os filhos HORIZONTAL (flex-row). Implementa gap, justify-content (eixo
/// principal) e align-items (eixo cruzado). Devolve a altura total do content.
///
/// - `wrap = false` (flex sem wrap): tudo numa linha; justify distribui o espaço
///   livre; em overflow, cai para flex-start (transborda no fim).
/// - `wrap = true` (inline-block/flex-wrap): quebra para a próxima linha quando não
///   cabe; justify/align aplicam POR LINHA.
fn layout_children_horizontal(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // altura do CONTENT do container quando explícita (já resolvida pelo caller,
    // no eixo certo) — referência do cross-axis p/ align-items e containing block
    // dos filhos.
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    wrap: bool,
    // `Some(N)` quando `display:grid`: cada item vira uma coluna de largura fixa
    // `(content_w - (N-1)*gap)/N` e a linha quebra a cada N. `None` = flex/wrap normal.
    grid_cols: Option<i32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // gap/row-gap resolvidos do CSS (px/%/… contra o content do container).
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let gap = css.gap.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);
    let row_gap = css.row_gap.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);
    let justify = css.justify.unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // `0` = sem height explícito (o cross-size da linha usa o max dos itens).
    let container_cross_h = container_content_h.unwrap_or(0.0);

    // ── PRÉ-PASS: coleta cada filho renderável com a BASE flex + fatores ─────────
    let mut items: Vec<FlexItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        // BLOCKIFICAÇÃO: um filho de flex é um item de nível BLOCO, mesmo sendo
        // um `<span>` (a spec blockifica os itens de flex; o Chrome reporta
        // `display:block` neles). Só um NÓ DE TEXTO é item anónimo.
        //
        // A condição era `!is_block_level`, e por isso um `<span>` filho de flex
        // caía no ramo de texto: era achatado para uma string, pintado com o
        // estilo do CONTAINER, e não registava caixa nenhuma — 345 dos 351
        // elementos `display:block` sem caixa da Wikipédia eram exatamente isto.
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            // texto solto: largura medida; vazio é ignorado. Não cresce nem encolhe.
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let w = ctx.measurer.text_width(&text, font_size, false, false);
            let h = crate::inline_box::altura_da_linha(css, font_size, ctx.measurer);
            items.push(FlexItem {
                node: child,
                base: w,
                main: w,
                h,
                is_text: true,
                grow: 0.0,
                shrink: 0.0,
                align_self: None,
                order: 0,
                can_stretch: false,
            });
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        let base = flex_base_outer(dom, child, content_w, font_size, ctx);
        let h = child_outer_height(dom, child, content_w, container_content_h, css, font_size, ctx);
        items.push(FlexItem {
            node: child,
            base,
            main: base,
            h,
            is_text: false,
            grow: ccss.flex_grow.unwrap_or(0.0),
            shrink: ccss.flex_shrink.unwrap_or(1.0), // 1 é o default do CSS
            align_self: ccss.align_self,
            order: ccss.order.unwrap_or(0),
            can_stretch: ccss.height.is_none(),
        });
    }
    // `order` reordena ANTES do wrap (sort estável: empate = ordem do documento).
    items.sort_by_key(|it| it.order);

    // GRID: cada item (não-texto) vira uma coluna de largura fixa. Fixa base=main=col_w
    // e zera grow/shrink (a coluna não flui) → o wrap abaixo quebra a cada N colunas.
    if let Some(n) = grid_cols {
        let n = n.max(1) as f32;
        let col_w = ((content_w - (n - 1.0) * gap) / n).max(0.0);
        for it in items.iter_mut() {
            if it.is_text {
                continue;
            }
            it.base = col_w;
            it.main = col_w;
            it.grow = 0.0;
            it.shrink = 0.0;
        }
    }

    // agrupa em LINHAS pela BASE (o wrap decide pelas bases; grow/shrink POR linha).
    let mut lines: Vec<Vec<FlexItem>> = vec![Vec::new()];
    let mut line_w = 0.0f32;
    for it in items {
        let cur = lines.last_mut().unwrap();
        let with_gap = if cur.is_empty() { 0.0 } else { gap };
        if wrap && !cur.is_empty() && line_w + with_gap + it.base > content_w {
            lines.push(Vec::new());
            line_w = it.base;
        } else {
            line_w += with_gap + it.base;
        }
        lines.last_mut().unwrap().push(it);
    }

    // ── RESOLVE + POSICIONA por linha: grow/shrink (main), justify, align ────────
    let mut line_y = content_y;
    for line in &mut lines {
        if line.is_empty() {
            continue;
        }
        let n = line.len();
        let total_gap = (n.saturating_sub(1)) as f32 * gap;

        // GROW/SHRINK (spec flexbox §9.7 simplificada): espaço livre positivo
        // distribui ∝ flex-grow (o `.col { flex:1 0 0% }` divide igual); negativo
        // encolhe ∝ shrink × base (itens maiores cedem mais), clamp ≥ 0.
        let sum_base: f32 = line.iter().map(|it| it.base).sum();
        let free_pre = content_w - sum_base - total_gap;
        let sum_grow: f32 = line.iter().map(|it| it.grow).sum();
        if free_pre > 0.0 && sum_grow > 0.0 {
            for it in line.iter_mut() {
                it.main = it.base + free_pre * it.grow / sum_grow;
            }
        } else if free_pre < 0.0 {
            let weighted: f32 = line.iter().map(|it| it.shrink * it.base).sum();
            if weighted > 0.0 {
                for it in line.iter_mut() {
                    it.main = (it.base + free_pre * (it.shrink * it.base) / weighted).max(0.0);
                }
            }
        }
        // re-mede a ALTURA com o main final (mais largura → menos linhas de texto);
        // só quando o main mudou (senão a medição do pré-pass vale).
        for it in line.iter_mut() {
            if !it.is_text && (it.main - it.base).abs() > 0.5 {
                let (_, h) = measure_block(
                    dom, it.node, content_w, container_content_h, Some(it.main), None, true, ctx,
                );
                it.h = h;
            }
        }

        // Cross-size de referência da linha = max das alturas dos itens, MAS se o
        // container tem `height` explícito e a linha é única (no-wrap), o cross-size
        // é a ALTURA DO CONTENT do container (fiel ao Chrome). Em wrap, cada linha
        // usa seu próprio max (repartir o height entre linhas — corte documentado).
        let items_h = line.iter().fold(0.0f32, |a, it| a.max(it.h));
        let line_h = if !wrap && container_cross_h > items_h {
            container_cross_h
        } else {
            items_h
        };

        // justify-content sobre o espaço restante PÓS-grow (com grow>0 o free é 0
        // e o justify é neutro — correto). Em overflow, ver justify_offsets.
        let sum_main: f32 = line.iter().map(|it| it.main).sum();
        let free = content_w - sum_main - total_gap;
        let (leading, between) = justify_offsets(justify, free, n);

        let mut x = content_x + leading;
        for (j, it) in line.iter().enumerate() {
            if j > 0 {
                x += gap + between;
            }
            // align por item: `align-self` vence o `align-items` do container;
            // STRETCH real: item sem height explícito ganha a ALTURA DA LINHA
            // (forced_outer_h) — os cards `.col` preenchem a linha.
            let item_align = it.align_self.unwrap_or(align);
            let stretches = item_align == crate::style::AlignItems::Stretch
                && it.can_stretch
                && !it.is_text
                && line_h > it.h;
            let off_cross = if stretches { 0.0 } else { align_offset(item_align, line_h, it.h) };
            let item_y = line_y + off_cross;
            if it.is_text {
                let text = collect_text(dom, it.node);
                let color = cor_visivel(&css, css.color.unwrap_or(0x000000FF));
                list.items.push(DisplayItem::Text {
                    x,
                    y: item_y,
                    text: text.into(),
                    color,
                    size: font_size,
                    mono: false,
                    bold: css.bold.unwrap_or(false),
                    letter_spacing: css.letter_spacing.unwrap_or(0.0),
                    decoration: decoration_code(css),
                });
            } else {
                // o main resolvido é IMPOSTO ao item (grow/shrink venceram o
                // width); stretch impõe a altura da linha.
                let forced_h = if stretches { Some(line_h) } else { None };
                layout_block(
                    dom, it.node, x, item_y, content_w, container_content_h,
                    Some(it.main), forced_h, true, ctx, list,
                );
            }
            x += it.main;
        }
        line_y += line_h + row_gap;
    }
    // desconta o último row_gap (só ENTRE linhas, não após a última).
    let total_h = (line_y - row_gap - content_y).max(0.0);
    total_h
}

/// Dispõe os filhos como FLEX COLUMN (`display:flex; flex-direction:column`): o
/// eixo PRINCIPAL é o vertical. Diferenças do block vertical: SEM margin-collapse
/// (flex não colapsa margens), `gap` entre itens (em column o espaçamento main é o
/// `row-gap`; o shorthand `gap:` seta ambos), `justify-content` distribui o espaço
/// livre VERTICAL (só quando o container tem altura explícita), `margin-top/bottom:
/// auto` de um item ABSORVE o espaço livre (spec flexbox §8.1 — é o `mb-auto`/
/// `mt-auto` do Bootstrap empurrando header/footer para as pontas), e `align-items`
/// atua no X: `stretch` (default) = item ocupa a largura; start/center/end = item
/// shrink-to-fit deslocado. Devolve a altura natural do content.
/// ⚠️ Cortes: `column-reverse` dispõe como `column` (sem inverter); `flex-wrap` em
/// column (multi-coluna) trata como coluna única; `flex-grow/shrink/basis` ainda
/// fora (fatia própria).
/// GRID real (css-grid track-sizing simplificado): resolve as trilhas de coluna
/// (px/%/fr/auto) e de linha, faz auto-placement dos itens célula-a-célula
/// (row-by-row), e posiciona cada item na sua célula com `justify-items`
/// (horizontal) / `align-items` (vertical). Suporta o subset do MDN:
/// grid-template-columns/rows, grid-auto-rows, gap, repeat(N,...), minmax(→max),
/// fr. NÃO suporta: grid-column/row-span explícito, areas, auto-fill/fit reais,
/// dense. Um item sem placement explícito preenche a próxima célula livre.
#[allow(clippy::too_many_arguments)]
fn layout_children_grid(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let col_gap = css.gap.or(css.row_gap).and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);
    let row_gap = css.row_gap.or(css.gap).and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);

    // ── COLUNAS: resolve as trilhas ──────────────────────────────────────────────
    // Sem grid-template-columns explícito → 1 coluna 1fr (o container-do-logo do
    // google: single-column grid). Com N colunas do grid_columns legado (repeat) →
    // N trilhas 1fr.
    let areas = css.grid_template_areas.clone();
    let col_tracks: Vec<crate::style::GridTrack> = match &css.grid_template_columns {
        Some(t) => (**t).clone(),
        // Sem trilhas declaradas mas COM áreas, é a matriz que diz quantas colunas
        // existem — cair no default de 1 coluna empilharia lado e conteúdo, que é
        // exatamente o sintoma que as áreas existem para resolver.
        None => {
            let n = match &areas {
                Some(a) => a.cols,
                None => css.grid_columns.unwrap_or(1).max(1) as usize,
            };
            vec![crate::style::GridTrack::Fr(1.0); n]
        }
    };
    // O número de colunas vem da LISTA de trilhas e não dos tamanhos: os
    // tamanhos ainda não estão decididos, porque uma trilha intrínseca precisa de
    // saber que itens lhe calham — e para isso é preciso ter colocado os itens.
    // A ordem é: quantas colunas → colocar os itens → medir → dimensionar.
    let ncols = col_tracks.len().max(1);

    // ── ITENS: os filhos renderizáveis (auto-placement row-by-row) ───────────────
    let mut children: Vec<NodeIdx> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        if is_out_of_flow(dom, child) {
            continue;
        }
        if !is_block_level(dom, child) && collect_text(dom, child).trim().is_empty() {
            continue;
        }
        children.push(child);
    }
    if children.is_empty() {
        return 0.0;
    }
    let cells = place_grid_items(dom, &children, areas.as_deref(), ncols);

    // A largura INTRÍNSECA por coluna — só medida quando alguma trilha é
    // intrínseca, porque medir custa uma travessia por item e a esmagadora
    // maioria das grades é só `fr` e px.
    let precisa_medir = col_tracks.iter().any(|t| matches!(t, crate::style::GridTrack::Auto));
    let conteudo: Option<Vec<f32>> = precisa_medir.then(|| {
        let mut w = vec![0.0f32; ncols];
        for c in &cells {
            // Um item que ATRAVESSA colunas não dita nenhuma delas sozinho: a
            // repartição do que ele pede pelas colunas que ocupa é a mesma
            // pergunta da tabela com `colspan`, e aqui não vale a complicação —
            // o que uma grade real tem em trilha intrínseca é a barra lateral,
            // que ocupa uma coluna só.
            if c.c1 - c.c0 != 1 || c.c0 >= ncols {
                continue;
            }
            w[c.c0] = w[c.c0].max(intrinsic_outer_width(dom, c.child, font_size, ctx));
        }
        w
    });
    let col_sizes = resolve_tracks(&col_tracks, content_w, col_gap, conteudo.as_deref(), &resolve);
    // Uma linha DECLARADA pela matriz existe mesmo sem item nela (ela ainda empurra
    // as linhas seguintes pelo gap), daí o max com `areas.rows`.
    let nrows = cells
        .iter()
        .map(|c| c.r1)
        .max()
        .unwrap_or(1)
        .max(areas.as_ref().map(|a| a.rows).unwrap_or(0))
        .max(1);

    // ── LINHAS: altura de cada linha ─────────────────────────────────────────────
    // grid-template-rows explícito (px/%/fr/auto), senão grid-auto-rows, senão a
    // altura do conteúdo mais alto da linha. `fr`/`%` de linha precisam da altura
    // do container (container_content_h).
    let explicit_rows: Vec<crate::style::GridTrack> = css
        .grid_template_rows
        .as_ref()
        .map(|t| (**t).clone())
        .unwrap_or_default();
    // mede a altura de conteúdo de cada linha (o item mais alto medido em shrink).
    // Um item que ATRAVESSA linhas reparte a sua altura IGUALMENTE pelas linhas do
    // span. O algoritmo da spec (§12.5) distribui pela contribuição de cada trilha;
    // a repartição igual foi escolhida por não precisar de uma segunda medição e por
    // errar sempre para MAIS espaço, nunca para item cortado.
    let mut content_row_h = vec![0.0f32; nrows];
    for cell in &cells {
        let cw = span_size(&col_sizes, cell.c0, cell.c1, col_gap);
        let (_, h) = measure_block(dom, cell.child, cw, container_content_h, None, None, true, ctx);
        let each = h / cell.rows() as f32;
        for r in cell.r0..cell.r1.min(nrows) {
            content_row_h[r] = content_row_h[r].max(each);
        }
    }
    let auto_row = css.grid_auto_rows;
    let has_explicit_row_track = |r: usize| {
        explicit_rows.get(r).is_some() || auto_row.is_some()
    };
    let mut row_sizes: Vec<f32> = (0..nrows)
        .map(|r| {
            let track = explicit_rows.get(r).copied().or(auto_row);
            match track {
                Some(crate::style::GridTrack::Fixed(d)) => resolve_height(Some(d), container_content_h, &resolve).unwrap_or(content_row_h[r]),
                _ => content_row_h[r], // Auto/None/Fr → conteúdo por ora (ajuste abaixo)
            }
        })
        .collect();
    // Se o container tem ALTURA definida e as linhas NÃO têm track explícita (auto),
    // as linhas DIVIDEM a altura do container (uma row auto num grid de altura fixa
    // preenche o espaço — é o que dá a track de 240 pro logo centrar). Distribui o
    // espaço livre igualmente entre as linhas auto (aproximação; fr real seria por
    // peso — mas grid sem template-rows usa 1fr implícito quando há altura).
    if let Some(ch) = container_content_h {
        let auto_rows: Vec<usize> = (0..nrows).filter(|&r| !has_explicit_row_track(r)).collect();
        if !auto_rows.is_empty() {
            let fixed: f32 = (0..nrows).filter(|r| has_explicit_row_track(*r)).map(|r| row_sizes[r]).sum();
            let total_gap = (nrows.saturating_sub(1)) as f32 * row_gap;
            let free = (ch - fixed - total_gap).max(0.0);
            let each = free / auto_rows.len() as f32;
            for r in auto_rows {
                row_sizes[r] = row_sizes[r].max(each);
            }
        }
    }

    // ── POSICIONA cada item na sua célula ────────────────────────────────────────
    let justify = css.grid_justify_items.unwrap_or(crate::style::AlignItems::Stretch);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // x acumulado de cada coluna, y de cada linha.
    let mut col_x = vec![content_x; ncols + 1];
    for c in 0..ncols {
        col_x[c + 1] = col_x[c] + col_sizes[c.min(col_sizes.len() - 1)] + col_gap;
    }
    let mut row_y = vec![content_y; nrows + 1];
    for r in 0..nrows {
        row_y[r + 1] = row_y[r] + row_sizes[r] + row_gap;
    }
    for cell in &cells {
        let child = cell.child;
        let cell_x = col_x[cell.c0];
        let cell_y = row_y[cell.r0];
        let cell_w = span_size(&col_sizes, cell.c0, cell.c1, col_gap);
        let cell_h = span_size(&row_sizes, cell.r0, cell.r1.min(nrows), row_gap);
        // mede o tamanho natural do item (shrink) p/ o alinhamento não-stretch.
        let stretch_x = justify == crate::style::AlignItems::Stretch;
        let stretch_y = align == crate::style::AlignItems::Stretch;
        let (nat_w, nat_h) = measure_block(dom, child, cell_w, Some(cell_h), None, None, true, ctx);
        let iw = if stretch_x { cell_w } else { nat_w.min(cell_w) };
        let ih = if stretch_y { cell_h } else { nat_h.min(cell_h) };
        let x = cell_x + cell_align_offset(justify, cell_w, iw);
        let y = cell_y + cell_align_offset(align, cell_h, ih);
        // pinta o item: stretch no eixo → forced size; senão shrink-to-fit.
        let forced_w = if stretch_x { None } else { Some(iw) };
        let forced_h = if stretch_y { Some(cell_h) } else { None };
        layout_block(dom, child, x, y, cell_w, Some(cell_h), forced_w, forced_h, !stretch_x, ctx, list);
    }
    // altura total = soma das linhas + gaps.
    let total_h: f32 = row_sizes.iter().sum::<f32>() + (nrows.saturating_sub(1)) as f32 * row_gap;
    total_h.max(0.0)
}

/// Onde UM item do grid vive: a célula inicial e o span, em índices de trilha com
/// o fim exclusivo. É o resultado da colocação — nomeada ou automática — e o único
/// que o resto do layout de grid consome, o que é o que permite às duas colocações
/// coexistirem sem um segundo caminho de posicionamento.
struct GridCell {
    child: NodeIdx,
    r0: usize,
    c0: usize,
    r1: usize,
    c1: usize,
}

impl GridCell {
    fn rows(&self) -> usize {
        (self.r1 - self.r0).max(1)
    }
}

/// Coloca os filhos: quem tem `grid-area: <nome>` presente na matriz do container
/// vai para o retângulo daquele nome; o resto preenche a próxima célula LIVRE em
/// row-major.
///
/// Os nomeados são colocados ANTES (spec §8.5 passo 1) por uma razão concreta e não
/// por fidelidade: se os automáticos fossem primeiro, um item nomeado para a coluna
/// da direita encontraria a célula já ocupada e ou sobrepunha ou empurrava — que é o
/// empilhamento que as áreas existem para evitar.
fn place_grid_items(
    dom: &Dom,
    children: &[NodeIdx],
    areas: Option<&crate::style::GridAreas>,
    ncols: usize,
) -> Vec<GridCell> {
    let mut cells: Vec<GridCell> = Vec::with_capacity(children.len());
    // ocupação row-major, crescida sob demanda (o nº de linhas não é conhecido antes
    // de saber quantos itens sobram para a colocação automática).
    let mut taken: Vec<bool> = Vec::new();
    let mut mark = |taken: &mut Vec<bool>, r0: usize, c0: usize, r1: usize, c1: usize| {
        let need = r1 * ncols;
        if taken.len() < need {
            taken.resize(need, false);
        }
        for r in r0..r1 {
            for c in c0..c1.min(ncols) {
                taken[r * ncols + c] = true;
            }
        }
    };

    let mut auto: Vec<NodeIdx> = Vec::new();
    for &child in children {
        let name = dom
            .computed_style_idx(child)
            .and_then(|s| s.grid_area.clone());
        match name.and_then(|n| areas.and_then(|a| a.area(&n))) {
            Some(a) => {
                mark(&mut taken, a.r0, a.c0, a.r1, a.c1);
                cells.push(GridCell { child, r0: a.r0, c0: a.c0, r1: a.r1, c1: a.c1.min(ncols) });
            }
            None => auto.push(child),
        }
    }

    // As linhas declaradas pela matriz contam como existentes mesmo sem item: um
    // automático não deve cair numa célula vazia RESERVADA (o `.` da matriz) antes
    // das linhas implícitas... mas cair nela é o comportamento da spec, então só as
    // células realmente ocupadas bloqueiam.
    let mut cursor = 0usize;
    for &child in &auto {
        while taken.get(cursor).copied().unwrap_or(false) {
            cursor += 1;
        }
        let (r, c) = (cursor / ncols, cursor % ncols);
        mark(&mut taken, r, c, r + 1, c + 1);
        cells.push(GridCell { child, r0: r, c0: c, r1: r + 1, c1: c + 1 });
        cursor += 1;
    }
    cells
}

/// Soma os tamanhos das trilhas `start..end` mais os gaps entre elas — o tamanho de
/// uma célula, que para span 1 é a trilha e para span N inclui os gaps que o span
/// atravessa (um span de 2 colunas cobre o gap do meio, não o perde).
fn span_size(sizes: &[f32], start: usize, end: usize, gap: f32) -> f32 {
    if sizes.is_empty() {
        return 0.0;
    }
    let end = end.max(start + 1).min(sizes.len());
    let start = start.min(sizes.len() - 1);
    let n = end.saturating_sub(start);
    sizes[start..end].iter().sum::<f32>() + (n.saturating_sub(1)) as f32 * gap
}
/// A LARGURA (ou altura) de cada trilha de uma grade.
///
/// A ordem das três passadas é a regra, e não um detalhe de implementação: uma
/// trilha intrínseca (`auto`/`min-content`) é dimensionada pelo CONTEÚDO antes
/// de qualquer espaço livre ser repartido, porque o espaço livre só existe
/// depois de se saber o que o conteúdo pede. Inverter as duas é o que fazia a
/// grade do `<main>` da Wikipédia dar 948px à coluna de conteúdo e empurrar a
/// barra lateral para fora da janela.
///
/// `conteudo[i]` é a largura intrínseca dos itens da trilha `i` — `None` quando
/// quem chama não a mediu (nenhuma trilha intrínseca na lista, e aí ela não é
/// precisa).
fn resolve_tracks(
    tracks: &[crate::style::GridTrack],
    container: f32,
    gap: f32,
    conteudo: Option<&[f32]>,
    ctx: &ResolveCtx,
) -> Vec<f32> {
    use crate::style::GridTrack as T;
    let n = tracks.len().max(1);
    let total_gap = (n.saturating_sub(1)) as f32 * gap;
    let dim = |d: &crate::style::Dimension| -> f32 {
        match d {
            // % de trilha resolve contra o container (largura p/ colunas).
            crate::style::Dimension::Percent(p) => container * p / 100.0,
            other => other.resolve(ctx).unwrap_or(0.0),
        }
        .max(0.0)
    };

    // 1ª passada: a BASE de cada trilha — o que ela pede antes de haver sobra.
    let mut sizes = vec![0.0f32; tracks.len()];
    let mut sum_fr = 0.0f32;
    for (i, t) in tracks.iter().enumerate() {
        sizes[i] = match t {
            T::Fixed(d) => dim(d),
            T::Bounded { min, .. } => dim(min),
            T::Auto => conteudo.and_then(|c| c.get(i)).copied().unwrap_or(0.0).max(0.0),
            T::Fr(f) => {
                sum_fr += f.max(0.0);
                0.0
            }
        };
    }
    let free = (container - sizes.iter().sum::<f32>() - total_gap).max(0.0);

    // 2ª passada: o espaço livre. `fr` come-o todo quando existe — é o que a
    // unidade significa —, e nesse caso uma trilha limitada ou intrínseca fica
    // pela sua base.
    if sum_fr > 0.0 {
        for (i, t) in tracks.iter().enumerate() {
            if let T::Fr(f) = t {
                sizes[i] = free * f.max(0.0) / sum_fr;
            }
        }
        return sizes;
    }

    // 3ª passada, sem `fr`: primeiro as trilhas LIMITADAS crescem até ao seu
    // máximo (é o que `minmax` pede), e só o que sobrar depois disso é que
    // estica as intrínsecas — `align-content: stretch`, o default.
    let mut sobra = free;
    let limitadas: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Bounded { .. }))
        .map(|(i, _)| i)
        .collect();
    if !limitadas.is_empty() && sobra > 0.0 {
        // Reparte por igual e não em proporção: a proporção seria contra as
        // bases, que num `minmax(0, x)` são todas zero.
        let quota = sobra / limitadas.len() as f32;
        for i in limitadas {
            if let T::Bounded { max, .. } = &tracks[i] {
                let teto = dim(max);
                let novo = (sizes[i] + quota).min(teto);
                sobra -= novo - sizes[i];
                sizes[i] = novo;
            }
        }
    }
    let autos: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Auto))
        .map(|(i, _)| i)
        .collect();
    if !autos.is_empty() && sobra > 0.0 {
        let cada = sobra / autos.len() as f32;
        for i in autos {
            sizes[i] += cada;
        }
    }
    sizes
}

/// Offset de alinhamento de um item de tamanho `item` dentro de uma célula de
/// tamanho `cell` (start=0, center=(cell-item)/2, end=cell-item; stretch=0).
fn cell_align_offset(a: crate::style::AlignItems, cell: f32, item: f32) -> f32 {
    match a {
        crate::style::AlignItems::Center => ((cell - item) / 2.0).max(0.0),
        crate::style::AlignItems::FlexEnd => (cell - item).max(0.0),
        _ => 0.0, // FlexStart / Stretch
    }
}

fn layout_children_column(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // altura do CONTENT do container quando explícita — a referência do eixo
    // principal (justify/margin-auto) e o containing block dos filhos (height:%).
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Em column, o espaço entre itens no eixo principal é o ROW-gap; o shorthand
    // `gap: X` seta os dois, então row_gap cobre o caso comum. (Fallback ao `gap`
    // — column-gap — só quando row_gap não veio, cobrindo `column-gap` usado
    // "errado" sem quebrar o shorthand.)
    let main_gap = css
        .row_gap
        .or(css.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let justify = css.justify.unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);

    // ── PASSO 1: mede a altura outer desejada de cada filho + margens auto ───────
    struct ColItem {
        node: NodeIdx,
        h: f32,
        is_text: bool,
        mt_auto: bool,
        mb_auto: bool,
        grow: f32,
    }
    let mut items: Vec<ColItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        // Blockificação, como no eixo horizontal — ver o comentário lá.
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            items.push(ColItem {
                node: child,
                h: crate::inline_box::altura_da_linha(css, font_size, ctx.measurer),
                is_text: true,
                mt_auto: false,
                mb_auto: false,
                grow: 0.0,
            });
            continue;
        }
        let h = child_outer_height(dom, child, content_w, container_content_h, css, font_size, ctx);
        let (mt_auto, mb_auto, grow) = dom
            .computed_style_idx(child)
            .map(|c| (c.margin.top.is_auto(), c.margin.bottom.is_auto(), c.flex_grow.unwrap_or(0.0)))
            .unwrap_or((false, false, 0.0));
        items.push(ColItem { node: child, h, is_text: false, mt_auto, mb_auto, grow });
    }
    if items.is_empty() {
        return 0.0;
    }

    // ── PASSO 2: distribui o espaço livre do eixo principal (Y) ──────────────────
    let n = items.len();
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let total_gap = (n.saturating_sub(1)) as f32 * main_gap;
    let free = container_content_h.map(|ch| ch - sum_h - total_gap).unwrap_or(0.0);
    // FLEX-GROW no eixo principal (css-flexbox §9.7): quando há espaço livre
    // positivo e algum item tem flex-grow, cada um cresce em proporção
    // `grow / soma_dos_grows * free` — dando ALTURA aos containers que os filhos
    // com `height:100%` resolvem (o logo/caixa do google centram assim). Consome
    // o `free` (o justify/margin-auto abaixo vê 0). margin:auto tem prioridade.
    let sum_grow: f32 = items.iter().map(|it| it.grow).sum();
    let any_auto = items.iter().any(|it| it.mt_auto || it.mb_auto);
    if free > 0.0 && sum_grow > 0.0 && !any_auto {
        for it in &mut items {
            if it.grow > 0.0 {
                it.h += it.grow / sum_grow * free;
            }
        }
    }
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let free = container_content_h.map(|ch| ch - sum_h - total_gap).unwrap_or(0.0);
    let auto_count: usize = items.iter().map(|it| it.mt_auto as usize + it.mb_auto as usize).sum();
    // margin:auto no eixo main absorve TODO o espaço livre (o justify vira no-op) —
    // spec css-flexbox §8.1. Sem autos, o justify distribui.
    let auto_size = if free > 0.0 && auto_count > 0 { free / auto_count as f32 } else { 0.0 };
    let (leading, between) = if auto_count > 0 {
        (0.0, 0.0)
    } else {
        justify_offsets(justify, free, n)
    };

    // ── PASSO 3: posiciona e pinta ────────────────────────────────────────────────
    let mut y = content_y + leading;
    for (j, it) in items.iter().enumerate() {
        if j > 0 {
            y += main_gap + between;
        }
        if it.mt_auto {
            y += auto_size;
        }
        if it.is_text {
            let text = collect_text(dom, it.node);
            list.items.push(DisplayItem::Text {
                x: content_x,
                y,
                text: text.into(),
                color: cor_visivel(&css, css.color.unwrap_or(0x000000FF)),
                size: font_size,
                mono: false,
                bold: css.bold.unwrap_or(false),
                letter_spacing: css.letter_spacing.unwrap_or(0.0),
                decoration: decoration_code(css),
            });
        } else {
            // CROSS (X): stretch (default) → o item ocupa a largura do container
            // (layout normal de bloco); start/center/end → shrink-to-fit + offset.
            let stretch = align == crate::style::AlignItems::Stretch;
            let child_x = if stretch {
                content_x
            } else {
                let (w, _) = measure_block(
                    dom, it.node, content_w, container_content_h, None, None, true, ctx,
                );
                let free_x = (content_w - w).max(0.0);
                content_x + align_offset(align, content_w, content_w - free_x)
            };
            // Um item que CRESCEU por flex-grow tem altura MAIOR que o conteúdo —
            // passa essa altura como containing block (avail_h) E como outer forçada
            // (forced_outer_h) para os filhos com `height:100%` resolverem contra ela.
            let (avail, forced_h) = if it.grow > 0.0 {
                (Some(it.h), Some(it.h))
            } else {
                (container_content_h, None)
            };
            layout_block(dom, it.node, child_x, y, content_w, avail, None, forced_h, !stretch, ctx, list);
        }
        y += it.h;
        if it.mb_auto {
            y += auto_size;
        }
    }
    (y - content_y).max(0.0)
}

/// Calcula (leading, between) do justify-content dado o espaço livre `free` e o nº
/// de itens `n`. `leading` = offset inicial; `between` = espaço EXTRA entre itens
/// (além do gap).
///
/// OVERFLOW (free<=0): VALIDADO contra o Chrome (com `flex-shrink:0` para forçar
/// overflow real — sem isso o flex-shrink encolhe os itens e não há overflow). Os
/// três distribuidores `space-*` caem para FLEX-START ([0,100,200] no teste), e só
/// `center`/`flex-end` mantêm o leading (negativo = transborda dos dois lados/start).
/// NB: a verificação adversarial sugeriu around/evenly→center, mas o Chrome real os
/// trata como flex-start — a medição no browser desempatou.
fn justify_offsets(j: crate::style::JustifyContent, free: f32, n: usize) -> (f32, f32) {
    use crate::style::JustifyContent as J;
    if free <= 0.0 {
        return match j {
            J::Center => (free / 2.0, 0.0), // leading negativo = transbordo centrado
            J::FlexEnd => (free, 0.0),      // todo o overflow no start
            // flex-start E os space-* → flush no start (fiel ao Chrome em overflow).
            J::FlexStart | J::SpaceBetween | J::SpaceAround | J::SpaceEvenly => (0.0, 0.0),
        };
    }
    match j {
        J::FlexStart => (0.0, 0.0),
        J::FlexEnd => (free, 0.0),
        J::Center => (free / 2.0, 0.0),
        J::SpaceBetween => {
            if n > 1 { (0.0, free / (n - 1) as f32) } else { (0.0, 0.0) }
        }
        J::SpaceAround => {
            if n >= 1 { (free / (2 * n) as f32, free / n as f32) } else { (0.0, 0.0) }
        }
        J::SpaceEvenly => (free / (n + 1) as f32, free / (n + 1) as f32),
    }
}

/// Offset no eixo cruzado de um item, dado o align-items, a altura da linha `line_h`
/// e a altura outer do item `item_h`. (stretch é tratado como flex-start aqui — o
/// esticar real exige passar altura imposta ao layout_block, fase futura.)
fn align_offset(a: crate::style::AlignItems, line_h: f32, item_h: f32) -> f32 {
    use crate::style::AlignItems as A;
    let free = line_h - item_h;
    match a {
        A::Stretch | A::FlexStart => 0.0,
        A::FlexEnd => free,
        A::Center => free / 2.0,
    }
}

/// Altura OUTER que um filho QUER, para o align-items/cross-axis. Para nós-bloco,
/// MEDE chamando o `layout_block` real numa `DisplayList` DESCARTÁVEL — assim a
/// altura medida é EXATAMENTE a que será pintada (inclui height explícito, frame,
/// recursão nos filhos, %). Sem aproximação: a verificação adversarial pegou que a
/// estimativa por "nº de linhas × line-height" divergia da pintura quando o filho
/// tinha frame próprio ou múltiplas linhas, errando a centralização cross-axis.
fn child_outer_height(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    container_h: Option<f32>,
    parent_css: &ComputedStyle,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match &dom.node(id).kind {
        // Como no eixo horizontal: qualquer elemento renderável mede-se pela sua
        // caixa real, porque um inline blockificado (item de flex) tem uma.
        NodeKind::Element { tag } if !is_non_rendered_tag(tag) => {
            // layout de teste numa lista descartável: o (_, outer_h) é a altura real.
            let (_, outer_h) = measure_block(dom, id, container_w, container_h, None, None, true, ctx);
            outer_h
        }
        // A MESMA altura que o fluxo dará a esta linha — medir com o default do
        // medidor enquanto o pai declara `line-height` fazia a medida do
        // cross-axis discordar da pintura, que é o erro que este comentário
        // acima diz que a verificação adversarial já apanhou uma vez.
        NodeKind::Text(_) => crate::inline_box::altura_da_linha(parent_css, parent_font, ctx.measurer),
        _ => 0.0,
    }
}

/// Largura OUTER que um filho QUER (sem pintar), para decidir a quebra de linha no
/// modo wrap. Bloco com `width`: esse width (+ frame); sem width: largura natural
/// do conteúdo (+ frame); texto solto: a largura do texto.
fn child_outer_width(dom: &Dom, id: NodeIdx, container_w: f32, parent_font: f32, ctx: &LayoutCtx) -> f32 {
    match &dom.node(id).kind {
        // QUALQUER elemento renderável, e não só os de nível bloco: um `<span>`
        // BLOCKIFICADO (item de flex, float) tem largura natural como qualquer
        // outra caixa. Com o guard antigo caía no `_ => 0.0` e era medido como
        // tendo largura ZERO — a caixa existia e não tinha tamanho.
        NodeKind::Element { tag } if !is_non_rendered_tag(tag) => {
            let css = dom.computed_style_idx(id).unwrap_or_default();
            let font = font_px(&css, parent_font);
            let resolve = ResolveCtx {
                parent_content_w: container_w,
                node_font_size: font,
                root_font_size: DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // frame horizontal = margin_h + 2*border + padding_h (cada já é o eixo;
            // unidades relativas resolvidas contra o container).
            let frame = css.margin.resolve_h(&resolve)
                + 2.0 * css.border_width.unwrap_or(0.0)
                + css.padding.resolve_h(&resolve);
            // Em border-box, o `width` declarado JÁ é a caixa (outer sem margin) —
            // não soma pad/border de novo; só a margin. Em content-box, soma o frame.
            match css.width.and_then(|d| d.resolve(&resolve)) {
                Some(w) if css.border_box.unwrap_or(false) => {
                    w + css.margin.resolve_h(&resolve)
                }
                Some(w) => w + frame,
                None => content_natural_width(dom, id, font, ctx) + frame,
            }
        }
        NodeKind::Text(t) => ctx.measurer.text_width(t, parent_font, false, false),
        _ => 0.0,
    }
}

/// Desenha um nó como linha(s) de texto (texto solto ou inline simples), herdando
/// cor/tamanho do bloco pai, e devolve o `y` abaixo. Caso de UM nó do fluxo
/// inline — o caminho geral (irmãos inline fluindo juntos) é
/// [`layout_inline_flow`].
fn layout_inline_line(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    layout_inline_flow(dom, id, &[id], x, y, content_w, parent_css, font_size, ctx, list)
}

/// O FLUXO INLINE RICO (P4): um GRUPO de irmãos inline consecutivos (nós de texto
/// + elementos inline como `<a>`/`<b>`/`<span>`) flui como UM contexto — os runs
/// de todos concatenam, quebram por palavra na largura, e cada pedaço pinta com a
/// SUA cor/peso. É o que faz `<p>texto <a>link</a>, fim</p>` virar UMA linha
/// (antes cada filho virava uma linha própria — o footer do Bootstrap cover saía
/// em 5 linhas).
fn layout_inline_flow(
    dom: &Dom,
    // O elemento DONO deste fluxo — de quem são as caixas geradas
    // (`::before`/`::after`) que envolvem o grupo. Ver `pseudo_run`.
    dono: NodeIdx,
    group: &[NodeIdx],
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let _phase = crate::metrics::phases::scope("layout-inline");
    // coleta os RUNS (cada pedaço de texto com a SUA cor/bold herdada do span que
    // o contém) de TODOS os nós do grupo, em ordem de documento.
    let mut runs = Vec::new();
    // A caixa gerada do DONO envolve todo o conteúdo dele — e só existe como run
    // aqui quando este grupo É todo o conteúdo. Com filhos de bloco pelo meio, o
    // conteúdo do dono parte-se em vários grupos e a caixa gerada teria de virar
    // um bloco anónimo, que é maquinaria de árvore de caixas que este layout não
    // tem; nesse caso não se gera nada, que é o estado anterior, em vez de a pôr
    // num pedaço arbitrário do conteúdo.
    // "este grupo é TODO o conteúdo do dono?" — contado sobre os filhos que
    // geram conteúdo. Os nós de texto só com espaços não contam: um HTML
    // indentado põe um antes e outro depois de cada elemento, e compará-los
    // fazia um `<div>` com o `<span>` numa linha indentada parecer conteúdo
    // partido, e perdia a caixa gerada em quase toda a página real.
    let filhos_com_conteudo = dom
        .node(dono)
        .children
        .iter()
        .filter(|&&c| !matches!(&dom.node(c).kind, NodeKind::Text(t) if t.trim().is_empty()))
        .count();
    let dono_inteiro = group
        .iter()
        .filter(|&&c| !matches!(&dom.node(c).kind, NodeKind::Text(t) if t.trim().is_empty()))
        .count()
        == filhos_com_conteudo;
    let cor_base = cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF));
    if dono_inteiro {
        runs.extend(pseudo_run(dom, dono, crate::style::PseudoElement::Before, cor_base));
    }
    for &id in group {
        runs.extend(collect_runs(dom, id, parent_css, content_w, ctx));
    }
    if dono_inteiro {
        runs.extend(pseudo_run(dom, dono, crate::style::PseudoElement::After, cor_base));
    }
    // Um MARKER (elemento inline vazio) não conta como conteúdo: um `<span></span>`
    // sozinho num bloco não cria linha nenhuma no browser, e criá-la aqui mudaria
    // a altura do bloco — o oposto de "acrescenta geometria, não muda a pintura".
    if runs
        .iter()
        .all(|r| r.text.trim().is_empty() && !matches!(r.atomic, Some((_, AtomicKind::Widget | AtomicKind::Replaced | AtomicKind::Block | AtomicKind::Break))))
    {
        return y;
    }
    let mono = parent_css
        .font_family
        .as_deref()
        .map(crate::style::is_mono_family)
        .unwrap_or(false);
    // line-height: do CSS (multiplicador ou px), senão o default do measurer —
    // #1749. O medidor é também quem responde por `line-height: normal`, porque
    // esse valor sai das MÉTRICAS DA FONTE e não de uma constante: sem isto, o
    // elemento sem declaração e o que declara `normal` — a spec diz que são o
    // mesmo valor — davam alturas diferentes.
    let lh = crate::inline_box::altura_da_linha(parent_css, font_size, ctx.measurer);
    let nowrap = matches!(
        parent_css.white_space,
        Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre)
    );
    let wrap_w = if nowrap { f32::INFINITY } else { content_w };
    // quebra os runs em LINHAS, cada linha = sequência de pedaços coloridos (word).
    let lines = wrap_runs(&runs, wrap_w, font_size, mono, ctx.measurer);
    // `text-indent`: recuo da PRIMEIRA linha (MDN). ⚠️ CORTE: recua o início da
    // linha mas NÃO encurta a largura de quebra dela — a quebra já foi calculada
    // acima, e refazê-la só para a primeira linha exigia partir o `wrap_runs` em
    // duas passadas. O erro fica no ponto de quebra da 1ª linha; o recuo, que é o
    // efeito que a página pede, está certo. Negativo é aceite (o truque de
    // esconder texto atrás da margem).
    let indent = parent_css
        .text_indent
        .and_then(|d| {
            d.resolve_signed(&ResolveCtx {
                parent_content_w: content_w,
                node_font_size: font_size,
                root_font_size: DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            })
        })
        .unwrap_or(0.0);
    let mut first_line = true;
    let mut cy = y;
    // CONSUMINDO as linhas: o texto de cada segmento vai direto para o
    // `DisplayItem`, em vez de ser clonado. Eram milhares de `String` alocadas
    // por passada de layout, uma por segmento, para copiar algo que ninguém mais
    // usaria depois.
    for line in lines {
        // largura total da linha (texto no SEU peso + widgets) p/ text-align.
        let line_w: f32 = line
            .iter()
            .map(|seg| match seg.atomic {
                Some(_) => seg.ww,
                None => seg.text_width,
            })
            .sum();
        // altura da linha: o texto (lh) ou o widget mais alto nela.
        let line_h = line
            .iter()
            .filter(|s| matches!(s.atomic, Some((_, AtomicKind::Widget | AtomicKind::Replaced | AtomicKind::Block | AtomicKind::Break))))
            .map(|s| s.wh)
            .fold(lh, f32::max);
        // A CAIXA de cada inline desta linha: a content area da fonte, centrada na
        // linha pela meia-entrelinha. A linha continua a avançar `line_h` — quem
        // decide o espaçamento é o `line-height`, quem decide a caixa é a fonte.
        let conteudo = crate::inline_box::altura_do_conteudo(font_size, ctx.measurer);
        let meia = crate::inline_box::meia_entrelinha(line_h, conteudo);
        let free = (content_w - line_w).max(0.0);
        let mut seg_x = match parent_css.text_align {
            Some(crate::style::TextAlign::Right) => x + free,
            Some(crate::style::TextAlign::Center) => x + free / 2.0,
            _ => x, // left/justify
        };
        if first_line {
            seg_x += indent;
            first_line = false;
        }
        // pinta cada pedaço NA SUA COR e PESO, avançando o x.
        for seg in line {
            let seg: Segment = seg;
            if let Some((a_idx, kind)) = seg.atomic {
                match kind {
                    AtomicKind::Widget => {
                        // WIDGET inline: pinta a caixa no lugar (botão via layout_button;
                        // campo de texto via layout_input com o avail da linha).
                        let wcss = dom.computed_style_idx(a_idx).unwrap_or_default();
                        let itype = dom
                            .node(a_idx)
                            .attr("type")
                            .map(|t| t.to_ascii_lowercase())
                            .unwrap_or_default();
                        if matches!(itype.as_str(), "submit" | "button" | "reset") {
                            layout_button(dom, a_idx, &wcss, seg_x, cy, ctx, list);
                        } else {
                            // `None` de altura disponível: uma caixa atómica numa
                            // linha não tem containing block de altura definida, e
                            // é isso que faz `height:%` valer `auto` — como no
                            // browser.
                            layout_input(dom, a_idx, &wcss, seg_x, cy, seg.ww, None, None, ctx, list);
                        }
                    }
                    AtomicKind::Replaced => {
                        // REPLACED inline (um `<img>` no meio do texto): a caixa é o
                        // tamanho já medido. Só se pinta quando há pixels — e aí é
                        // `layout_image` que o faz, o mesmo caminho do fluxo de bloco,
                        // em vez de um segundo emissor de imagem só para o inline.
                        if dom.image_of(a_idx).is_some() {
                            let icss = dom.computed_style_idx(a_idx).unwrap_or_default();
                            layout_image(dom, a_idx, &icss, seg_x, cy, seg.ww.max(1.0), ctx, list);
                        }
                    }
                    AtomicKind::Block => {
                        // Um inline-block PINTA-SE como bloco (fundo, borda,
                        // padding) mas na posição que a linha lhe deu. É o mesmo
                        // `layout_block` da corrida de inline-blocks irmãos —
                        // não um segundo emissor — só que o x/y vem do fluxo.
                        layout_block(
                            dom, a_idx, seg_x, cy, seg.ww.max(1.0), None, None, None, true, ctx,
                            list,
                        );
                    }
                    AtomicKind::Marker | AtomicKind::Break => {}
                }
                // A CAIXA DO PRÓPRIO: a de uma caixa atómica é o seu tamanho; a
                // de um vazio/quebra é a fatia de linha que ele ocupa.
                let propria = match kind {
                    AtomicKind::Marker | AtomicKind::Break => {
                        Rect::new(seg_x, cy + meia, 0.0, conteudo)
                    }
                    _ => Rect::new(seg_x, cy, seg.ww, seg.wh),
                };
                crate::inline_box::union_rect(list, a_idx, propria);
                // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
                // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
                // 528px de altura mede 17px no browser, não 528. É a mesma regra
                // que já vale para o texto, aplicada ao que não é texto.
                let na_linha = Rect::new(seg_x, cy + meia, seg.ww, conteudo);
                for &owner in &seg.owners {
                    crate::inline_box::union_rect(list, owner, na_linha);
                }
                // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
                // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
                // 528px de altura mede 17px no browser, não 528. É a mesma regra
                // que já vale para o texto, aplicada ao que não é texto.

                seg_x += seg.ww;
                continue;
            }
            let ls = parent_css.letter_spacing.unwrap_or(0.0);
            let w = seg.text_width + ls * seg.text.chars().count() as f32;
            list.items.push(DisplayItem::Text {
                x: seg_x,
                y: cy + meia,
                text: seg.text.into(),
                color: seg.color,
                size: font_size,
                mono,
                bold: seg.bold,
                letter_spacing: ls,
                decoration: seg.deco,
            });
            let text_fragment = Rect::new(seg_x, cy + meia, w.max(0.0), conteudo);
            for &owner in &seg.owners {
                crate::inline_box::union_rect(list, owner, text_fragment);
            }
            seg_x += w;
        }
        cy += line_h;
    }
    cy
}

/// Um pedaço de texto inline com seu estilo resolvido (cor/peso herdados do span pai).
/// `atomic: Some((idx, kind))` = uma CAIXA em vez de texto — um widget de
/// formulário, um replaced element (`<img>`), ou o marcador de um inline vazio.
/// As duas primeiras fluem como uma "palavra" inquebrável de `ww × wh` pontos
/// (item 8 do handoff #1793; os botões 'Pesquisa Google' do google legado vivem
/// em span>span>input); o marcador não ocupa nada.
struct InlineRun {
    text: String,
    color: u32,
    bold: bool,
    /// decoração do RUN (0=none 1=underline 2=line-through) — vem do <a>/<span>
    /// que contém o texto, não do bloco pai (um <a> sublinha só o seu texto).
    deco: u8,
    /// Elementos inline ancestrais deste run. Cada um recebe a união dos fragmentos.
    owners: Vec<NodeIdx>,
    atomic: Option<(NodeIdx, AtomicKind)>,
    ww: f32,
    wh: f32,
}

/// O run de texto de uma caixa gerada (`::before`/`::after`) de `id`, ou vazio
/// se a cascata não manda gerar nenhuma.
///
/// Entregar conteúdo gerado como um `InlineRun` é o que faz esta funcionalidade
/// caber sem reescrever o fluxo: um run é "texto com um estilo, pertencente a
/// estes elementos inline", e é exatamente o que um `::before` de texto é. Em
/// particular ele quebra linha, herda e é medido pelo mesmo caminho do resto —
/// nada disto precisou de um segundo caminho.
///
/// `owners` recebe o elemento ORIGINANTE: no browser a caixa gerada está DENTRO
/// da caixa do elemento e um clique nela atinge o elemento. Como o pseudo não
/// tem `NodeIdx`, é a única resposta possível — e é a certa.
///
/// CORTE DECLARADO: só o texto e as propriedades que um run carrega (cor, peso,
/// decoração) chegam à pintura. `background`, `padding`, `border` e `width` do
/// pseudo são ignorados, e `display:block`/`inline-block`/`position:absolute`
/// nele são tratados como o inline que a maioria é. Medido na folha da
/// Wikipédia: 88 das 100 regras com pseudo-elemento são inline por omissão.
fn pseudo_run(
    dom: &Dom,
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    // A cor já resolvida do contexto — a caixa gerada herda-a quando não
    // declara `color`.
    cor_herdada: u32,
) -> Option<InlineRun> {
    let caixa = dom.pseudo_box(id, pe)?;
    crate::bump!(inline_runs);
    Some(InlineRun {
        text: caixa.texto,
        color: cor_visivel(&caixa.css, caixa.css.color.unwrap_or(cor_herdada)),
        bold: caixa.css.bold.unwrap_or(false),
        deco: decoration_code(&caixa.css),
        owners: vec![id],
        atomic: None,
        ww: 0.0,
        wh: 0.0,
    })
}

/// Um elemento inline de texto não cria caixa própria no fluxo, mas ainda assim
/// deve receber um retângulo união dos fragmentos que seus descendentes pintam.
fn is_inline_text_container(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            !is_non_rendered_tag(tag)
                && !is_out_of_flow(dom, id)
                && !is_block_level(dom, id)
                && !is_inline_block(dom, id)
        }
        _ => false,
    }
}

/// Reserva uma posição de pintura antes de layoutar os descendentes. Um retângulo
/// placeholder fica invisível para o hit-test até ser preenchido por `record_node_rect`.
pub(crate) fn reserve_node_order(list: &mut DisplayList, idx: NodeIdx) {
    if !list.node_rects.contains_key(&idx) {
        list.node_rects.insert(idx, Rect::new(0.0, 0.0, 0.0, 0.0));
        list.hit_order.push(idx);
    }
}

/// Registra uma caixa e sua geometria. Se o nó já foi reservado como ancestral,
/// apenas substitui o placeholder sem duplicar a ordem de hit-test.
pub(crate) fn record_node_rect(list: &mut DisplayList, idx: NodeIdx, rect: Rect) {
    if list.node_rects.insert(idx, rect).is_none() {
        list.hit_order.push(idx);
    }
}

/// Coleta os RUNS de texto de `id` em ordem de documento, cada um com a COR efetiva
/// do elemento inline que o contém (um `<span style=color:x>` muda a cor do seu
/// texto). Aplica text-transform por run. A cor vem do `computed_style_idx` do nó
/// inline (que já herda do pai via a cascade) — é por isso que o style do span passa
/// a valer no texto.
fn collect_runs(
    dom: &Dom,
    id: NodeIdx,
    parent_css: &ComputedStyle,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    let _phase = crate::metrics::phases::scope("collect-runs");
    let mut runs = Vec::new();
    walk(
        dom,
        ctx,
        avail_w,
        id,
        cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF)),
        decoration_code(parent_css),
        parent_css.text_transform,
        parent_css.bold.unwrap_or(false),
        &[],
        &mut runs,
    );
    return runs;

    fn walk(
        dom: &Dom,
        ctx: &LayoutCtx,
        avail_w: f32,
        id: NodeIdx,
        inherited_color: u32,
        inherited_deco: u8,
        inherited_tt: Option<crate::style::TextTransform>,
        inherited_bold: bool,
        inherited_owners: &[NodeIdx],
        out: &mut Vec<InlineRun>,
    ) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => {
                let text = match inherited_tt {
                    Some(tt) => tt.apply(t),
                    None => t.clone(),
                };
                crate::bump!(inline_runs);
                out.push(InlineRun {
                    text,
                    color: inherited_color,
                    bold: inherited_bold,
                    deco: inherited_deco,
                    owners: inherited_owners.to_vec(),
                    atomic: None,
                    ww: 0.0,
                    wh: 0.0,
                });
            }
            NodeKind::Element { tag } => {
                // `<script>`/`<style>`/head-etc DENTRO de um contexto inline (um
                // script dentro de <td>/<center> — google.com faz isso): o texto
                // cru NÃO é conteúdo renderável — sem este skip, o código JS era
                // PINTADO na página.
                if is_non_rendered_tag(tag) {
                    return;
                }
                // WIDGET inline: um `<input>` no meio do fluxo (botão/campo) vira
                // um run-widget com o tamanho pré-medido — o wrap o trata como
                // palavra inquebrável e a emissão pinta a caixa no lugar.
                if is_text_input_tag(tag) {
                    let itype = dom
                        .node(id)
                        .attr("type")
                        .map(|t| t.to_ascii_lowercase())
                        .unwrap_or_default();
                    if itype == "hidden" {
                        return;
                    }
                    let (ww, wh) = inline_widget_size(dom, id, &itype, avail_w, ctx);
                    // Os ANCESTRAIS inline não engolem a caixa deste widget: no
                    // browser a caixa de um inline tem a largura do que ele
                    // contém e a altura da FONTE. Quem recebe `ww × wh` é só o
                    // próprio elemento, na emissão.
                    let owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Widget)),
                        ww,
                        wh,
                    });
                    return;
                }
                // `<br>`: uma QUEBRA no meio do fluxo. Não é texto nem caixa — é o
                // fim da linha corrente, e o browser dá-lhe na mesma posição e
                // altura de linha. Sem isto as duas linhas que ele separa saíam
                // como uma só, e tudo o que vinha abaixo subia uma linha.
                if tag == "br" {
                    let mut owners = inherited_owners.to_vec();
                    owners.push(id);
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Break)),
                        ww: 0.0,
                        wh: 0.0,
                    });
                    return;
                }
                // REPLACED inline (`<img>` dentro de um `<a>`, `<video>`, …): não é
                // texto e não tem filhos que o descrevam, por isso não produzia run
                // nenhum e ficava sem caixa. Flui como palavra inquebrável.
                let rcss = dom.computed_style_idx(id).unwrap_or_default();
                if let Some((ww, wh)) =
                    crate::inline_box::replaced_inline_size(dom, id, &rcss, avail_w, ctx)
                {
                    // Como no widget: a caixa do replaced é dele; os ancestrais
                    // inline recebem só a linha que ele ocupa.
                    let owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Replaced)),
                        ww,
                        wh,
                    });
                    return;
                }
                // INLINE COM CAIXA: mede-se como bloco shrink-to-fit e entra na
                // linha como palavra inquebrável. Antes fechava o fluxo inline e
                // abria linha própria — um `<p>texto <span com fundo>x</span>
                // texto</p>` saía em TRÊS linhas em vez de uma, e numa página
                // real isso multiplicava a altura do documento por ~2,7.
                if is_inline_block(dom, id) {
                    let (bw, bh) = measure_block(dom, id, avail_w, None, None, None, true, ctx);
                    let mut owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        deco: 0,
                        owners: std::mem::take(&mut owners),
                        atomic: Some((id, AtomicKind::Block)),
                        ww: bw,
                        wh: bh,
                    });
                    return;
                }
                // a cor/text-transform/peso/decoração DESTE inline (se declarar)
                // vence p/ os filhos (o <a> sublinha só o próprio texto).
                let css = dom.computed_style_idx(id);
                let color = css.as_ref().and_then(|c| c.color).unwrap_or(inherited_color);
                let tt = css.as_ref().and_then(|c| c.text_transform).or(inherited_tt);
                let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(inherited_bold);
                let deco = match css.as_deref().map(decoration_code) {
                    Some(d) if d != 0 => d,
                    _ => inherited_deco,
                };
                let mut owners = inherited_owners.to_vec();
                let is_container = is_inline_text_container(dom, id);
                if is_container {
                    owners.push(id);
                }
                // As caixas geradas de um elemento INLINE (`a::after`) entram
                // aqui, à volta do conteúdo próprio dele. O dono de um fluxo
                // inteiro é tratado em `layout_inline_flow`, que é onde ele se
                // sabe dono; os dois casos não se sobrepõem.
                let before = out.len();
                out.extend(pseudo_run(dom, id, crate::style::PseudoElement::Before, color));
                for &c in &dom.node(id).children {
                    walk(dom, ctx, avail_w, c, color, deco, tt, bold, &owners, out);
                }
                out.extend(pseudo_run(dom, id, crate::style::PseudoElement::After, color));
                // Um inline VAZIO (`<source>`, `<br>`, `<span></span>`) não gerou run
                // e ficaria sem caixa. O marker dá-lhe a posição na linha sem lhe dar
                // largura nem altura próprias — que é a caixa que o browser reporta.
                if is_container && out.len() == before {
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Marker)),
                        ww: 0.0,
                        wh: 0.0,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Tamanho OUTER de um widget inline (`<input>`): o MESMO cálculo que a emissão
/// usa (layout_button / layout_input), para o wrap reservar a largura exata.
fn inline_widget_size(
    dom: &Dom,
    id: NodeIdx,
    itype: &str,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    if matches!(itype, "submit" | "button" | "reset") {
        let font = font_px(&css, DEFAULT_FONT_SIZE - 3.0);
        let label = dom.node(id).attr("value").unwrap_or("").to_string();
        let tw = ctx.measurer.text_width(&label, font, false, false);
        let lh = ctx.measurer.line_height(font);
        return (tw + 24.0 + 6.0, lh + 10.0 + 4.0); // espelha layout_button
    }
    // Campo de texto ou marca: a MESMA medida que a emissão vai usar, pedida à
    // mesma função. Estava aqui uma cópia com números à mão (190 x lh+8) que
    // dizia espelhar o `layout_input` e não espelhava — um `checkbox` reservava
    // um campo de texto e pintava um quadrado.
    //
    // `None` de altura disponível: uma caixa numa linha não tem containing block
    // de altura definida, logo `height:%` vale `auto`, como no browser.
    medida_do_input(dom, id, &css, avail_w, None, None, ctx).outer()
}

/// Um segmento de texto colorido/pesado posicionado numa linha (após o wrap).
/// `atomic: Some((idx, kind))` = uma caixa de `ww × wh` (pintada pela emissão),
/// ou um marcador de largura zero que só existe para receber a sua geometria.
struct Segment {
    text: String,
    text_width: f32,
    color: u32,
    bold: bool,
    deco: u8,
    owners: Vec<NodeIdx>,
    atomic: Option<(NodeIdx, AtomicKind)>,
    ww: f32,
    wh: f32,
}

/// Quebra uma sequência de RUNS coloridos em LINHAS por palavra (word-wrap), juntando
/// runs adjacentes na mesma linha. Cada linha é um vetor de [`Segment`] (pedaços
/// coloridos contíguos). Uma palavra que não cabe começa nova linha. FIEL AOS
/// ESPAÇOS do fonte: um espaço só entra entre duas palavras quando o texto
/// ORIGINAL tinha whitespace ali (colapsado p/ 1) — inclusive ATRAVÉS de runs
/// (`<a>Bootstrap</a>, by` NÃO ganha espaço antes da vírgula; antes toda palavra
/// ganhava espaço e a pontuação descolava).
/// Colapsa o whitespace de um run como o fluxo inline faz: sequências viram um
/// espaço só, o do fim some (o separador seguinte o recria) e o do início só
/// entra quando havia palavra antes na linha. É a normalização que o scanner
/// palavra-a-palavra produz implicitamente — o fast path precisa dela explícita
/// para que os dois caminhos gerem o MESMO texto.
fn collapse_ws(text: &str, leading_space: bool) -> std::borrow::Cow<'_, str> {
    // O caso comum é o texto JÁ normalizado (uma palavra, ou palavras separadas
    // por um espaço só, sem borda) — devolver emprestado evita uma alocação por
    // run, e um relayout de página grande são milhares deles.
    let needs_work = leading_space && text.starts_with(char::is_whitespace)
        || text.starts_with(char::is_whitespace)
        || text.ends_with(char::is_whitespace)
        || text.contains("  ")
        || text.chars().any(|c| c.is_whitespace() && c != ' ');
    if !needs_work {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    if leading_space && text.starts_with(char::is_whitespace) {
        out.push(' ');
    }
    let mut first = true;
    for word in text.split_whitespace() {
        if !first {
            out.push(' ');
        }
        out.push_str(word);
        first = false;
    }
    std::borrow::Cow::Owned(out)
}

/// Acrescenta texto à linha, juntando ao último segmento quando o estilo é o
/// mesmo (é o que evita um segmento por palavra na hora de pintar).
fn push_segment(cur: &mut Vec<Segment>, run: &InlineRun, text: &str, width: f32) {
    if let Some(last) = cur.last_mut() {
        if last.atomic.is_none()
            && last.color == run.color
            && last.bold == run.bold
            && last.deco == run.deco
            && last.owners == run.owners
        {
            last.text.push_str(text);
            last.text_width += width;
            return;
        }
    }
    cur.push(Segment {
        text: text.to_string(),
        text_width: width,
        color: run.color,
        bold: run.bold,
        deco: run.deco,
        owners: run.owners.clone(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
    });
}

fn wrap_runs(
    runs: &[InlineRun],
    max_w: f32,
    font_size: f32,
    mono: bool,
    m: &dyn TextMeasurer,
) -> Vec<Vec<Segment>> {
    let _phase = crate::metrics::phases::scope("wrap-runs");
    // A largura do espaço só interessa ao caminho palavra-a-palavra. Medida
    // sempre, era metade de todas as medições de texto de um relayout — uma por
    // chamada, mesmo quando o fast path respondia sozinho.
    let mut space_w_memo: Option<f32> = None;
    let mut space_w = |m: &dyn TextMeasurer| -> f32 {
        *space_w_memo.get_or_insert_with(|| m.text_width(" ", font_size, mono, false))
    };
    let mut lines: Vec<Vec<Segment>> = Vec::new();
    let mut cur: Vec<Segment> = Vec::new();
    let mut cur_w = 0.0f32;
    let mut at_line_start = true;
    // havia whitespace no ORIGINAL desde a última palavra? (carrega entre runs)
    let mut pending_space = false;

    for run in runs {
        // WIDGET: uma "palavra" inquebrável de run.ww pontos, segmento próprio.
        if let Some((a_idx, kind)) = run.atomic {
            // BREAK: entra na linha (para receber a sua caixa) e FECHA-A.
            if kind == AtomicKind::Break {
                cur.push(Segment {
                    text: String::new(),
                    text_width: 0.0,
                    color: run.color,
                    bold: false,
                    deco: 0,
                    owners: run.owners.clone(),
                    atomic: Some((a_idx, AtomicKind::Break)),
                    ww: 0.0,
                    wh: 0.0,
                });
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
                pending_space = false;
                continue;
            }
            // MARKER: largura zero, não quebra a linha, não consome o espaço
            // pendente — só marca uma posição para quem lhe quiser a caixa.
            if kind == AtomicKind::Marker {
                cur.push(Segment {
                    text: String::new(),
                    text_width: 0.0,
                    color: run.color,
                    bold: false,
                    deco: 0,
                    owners: run.owners.clone(),
                    atomic: Some((a_idx, AtomicKind::Marker)),
                    ww: 0.0,
                    wh: 0.0,
                });
                continue;
            }
            let with_space = pending_space && !at_line_start;
            let need = if with_space { space_w(m) + run.ww } else { run.ww };
            if !at_line_start && cur_w + need > max_w {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
            }
            cur.push(Segment {
                text: String::new(),
                text_width: 0.0,
                color: run.color,
                bold: false,
                deco: 0,
                owners: run.owners.clone(),
                atomic: Some((a_idx, kind)),
                ww: run.ww,
                wh: run.wh,
            });
            cur_w += if pending_space && !at_line_start { space_w(m) + run.ww } else { run.ww };
            at_line_start = false;
            pending_space = false;
            continue;
        }
        // FAST PATH: o run INTEIRO cabe na linha corrente.
        //
        // O scanner abaixo mede palavra por palavra — e medir texto é a única
        // coisa que o layout pede ao backend, que com uma fonte real faz shaping
        // em vez de multiplicar um contador de caracteres. Medido no medidor
        // barato: `wrap-runs` era 38% de um relayout de página grande, com 11 000
        // `text_width` por frame. Quando o run cabe (o caso comum: quase todo
        // texto de página não quebra), uma medição responde por todas.
        //
        // Medir a string inteira também é o que um browser faz, então a largura
        // resultante é MAIS fiel e não menos: soma de palavras ignora o que a
        // fonte faz nas junções.
        if run.atomic.is_none() {
            let leading = pending_space && !at_line_start;
            let normalized = collapse_ws(&run.text, leading);
            if normalized.is_empty() {
                // só whitespace: vira separador pendente e não abre segmento.
                if run.text.chars().any(char::is_whitespace) {
                    pending_space = true;
                }
                continue;
            }
            let w = m.text_width(&normalized, font_size, mono, run.bold);
            // `at_line_start ||` estava aqui e fazia um run que NÃO CABE passar
            // inteiro quando era o primeiro da linha — um parágrafo cujo texto
            // vem num único nó (o caso comum de uma página real) nunca quebrava,
            // saía numa linha de milhares de pontos e levava consigo a caixa de
            // todos os inlines dentro dele. O scanner palavra-a-palavra abaixo já
            // trata o início de linha corretamente (não quebra ANTES da primeira
            // palavra), por isso a condição certa é só "cabe".
            if cur_w + w <= max_w {
                let trailing_space = run.text.ends_with(char::is_whitespace);
                push_segment(&mut cur, run, &normalized, w);
                cur_w += w;
                at_line_start = false;
                pending_space = trailing_space;
                continue;
            }
        }
        // scanner ws/palavra preservando a fronteira original.
        let mut rest = run.text.as_str();
        while !rest.is_empty() {
            if rest.starts_with(char::is_whitespace) {
                pending_space = true;
                rest = rest.trim_start();
                continue;
            }
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let word = &rest[..end];
            rest = &rest[end..];

            let ww = m.text_width(word, font_size, mono, run.bold);
            let with_space = pending_space && !at_line_start;
            let need = if with_space { space_w(m) + ww } else { ww };
            if !at_line_start && cur_w + need > max_w {
                // não cabe: fecha a linha.
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
            }
            let sep = pending_space && !at_line_start;
            let piece_w = if sep { space_w(m) + ww } else { ww };
            let mut piece = String::with_capacity(word.len() + usize::from(sep));
            if sep {
                piece.push(' ');
            }
            piece.push_str(word);
            // junta no último segmento se mesma cor E peso (e não-widget), senão novo.
            if let Some(last) = cur.last_mut() {
                if last.atomic.is_none()
                    && last.color == run.color
                    && last.bold == run.bold
                    && last.deco == run.deco
                    && last.owners == run.owners
                {
                    last.text.push_str(&piece);
                    last.text_width += piece_w;
                } else {
                    cur.push(Segment {
                        text: piece,
                        text_width: piece_w,
                        color: run.color,
                        bold: run.bold,
                        deco: run.deco,
                        owners: run.owners.clone(),
                        atomic: None,
                        ww: 0.0,
                        wh: 0.0,
                    });
                }
            } else {
                cur.push(Segment {
                    text: piece,
                    text_width: piece_w,
                    color: run.color,
                    bold: run.bold,
                    deco: run.deco,
                    owners: run.owners.clone(),
                    atomic: None,
                    ww: 0.0,
                    wh: 0.0,
                });
            }
            cur_w += piece_w;
            at_line_start = false;
            pending_space = false;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(vec![Segment {
            text: String::new(),
            text_width: 0.0,
            color: 0,
            bold: false,
            deco: 0,
            owners: Vec::new(),
            atomic: None,
            ww: 0.0,
            wh: 0.0,
        }]);
    }
    lines
}

/// Quebra `text` em LINHAS que cabem em `max_w` (word-wrap do CSS `white-space:
/// normal`): acumula palavras separadas por espaço; quando a próxima não cabe,
/// fecha a linha e começa outra. Uma palavra maior que `max_w` fica sozinha na
/// linha (não quebra no meio da palavra — `overflow-wrap:normal`).
fn wrap_text(text: &str, max_w: f32, font_size: f32, mono: bool, m: &dyn TextMeasurer) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0.0f32;
    let space_w = m.text_width(" ", font_size, mono, false);
    for word in text.split_whitespace() {
        let word_w = m.text_width(word, font_size, mono, false);
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else if current_w + space_w + word_w <= max_w {
            current.push(' ');
            current.push_str(word);
            current_w += space_w + word_w;
        } else {
            // não cabe: fecha a linha atual e começa nova com a palavra.
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_w = word_w;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Concatena o texto de todos os descendentes de `id` (ordem de documento).
fn collect_text(dom: &Dom, id: NodeIdx) -> String {
    let _phase = crate::metrics::phases::scope("collect-text");
    let mut out = String::new();
    collect_into(dom, id, &mut out);
    return out;

    fn collect_into(dom: &Dom, id: NodeIdx, out: &mut String) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => out.push_str(t),
            // `<script>`/`<style>` não são conteúdo renderável — o texto cru
            // deles não entra no texto pintado (mesmo skip do collect_runs).
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            _ => {
                for &c in &dom.node(id).children {
                    collect_into(dom, c, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_html_to_dom;

    /// Tolerância da comparação reuso × cálculo.
    ///
    /// Reusar um fragmento numa posição nova é somar um deslocamento às
    /// coordenadas, e somar não dá bit a bit o mesmo que calcular a posição do
    /// zero: `67.4` calculado vira `67.399994` deslocado. É a mesma aritmética
    /// que qualquer motor de layout com reuso tem, a diferença é da ordem de
    /// 1e-5 pontos — invisível em tela e irrelevante para hit-test. O que o
    /// teste NÃO tolera é diferença de conteúdo, de contagem ou de ordem.
    const TOL: f32 = 0.01;

    fn rects_equivalentes(a: &Rect, b: &Rect) -> bool {
        (a.x - b.x).abs() < TOL
            && (a.y - b.y).abs() < TOL
            && (a.w - b.w).abs() < TOL
            && (a.h - b.h).abs() < TOL
    }

    /// Dois itens de pintura iguais a menos da tolerância acima. Texto, cor e
    /// tipo têm de bater EXATAMENTE: só a geometria admite o erro do
    /// deslocamento.
    fn itens_equivalentes(a: &DisplayItem, b: &DisplayItem) -> bool {
        use DisplayItem as D;
        match (a, b) {
            (D::SolidRect { rect: ra, color: ca, radius: da }, D::SolidRect { rect: rb, color: cb, radius: db }) => {
                rects_equivalentes(ra, rb) && ca == cb && (da - db).abs() < TOL
            }
            (D::Border { rect: ra, width: wa, color: ca, radius: da }, D::Border { rect: rb, width: wb, color: cb, radius: db }) => {
                rects_equivalentes(ra, rb) && (wa - wb).abs() < TOL && ca == cb && (da - db).abs() < TOL
            }
            (
                D::Text { x: xa, y: ya, text: ta, color: ca, size: sa, mono: ma, bold: ba, letter_spacing: la, decoration: dea },
                D::Text { x: xb, y: yb, text: tb, color: cb, size: sb, mono: mb, bold: bb, letter_spacing: lb, decoration: deb },
            ) => {
                (xa - xb).abs() < TOL
                    && (ya - yb).abs() < TOL
                    && ta == tb
                    && ca == cb
                    && (sa - sb).abs() < TOL
                    && ma == mb
                    && ba == bb
                    && (la - lb).abs() < TOL
                    && dea == deb
            }
            (D::EndClip { .. }, D::EndClip { .. }) => true,
            // As demais variantes não aparecem neste corpus; comparar por
            // igualdade estrita aqui é o certo — se um dia aparecerem com
            // deslocamento, o teste falha e o braço é escrito.
            _ => a == b,
        }
    }

    /// Dentro de um container que ROLA, `width:%` de um filho é a porcentagem da
    /// CAIXA — não do conteúdo transbordado.
    ///
    /// O layout de um scroll container usa a largura NATURAL do conteúdo para
    /// dispor os filhos (senão o flex os comprimiria e nada transbordaria), e
    /// essa largura estava servindo também de base para as porcentagens. Numa
    /// página que aninha vários `overflow:auto` — o WhatsApp Web é uma —, cada
    /// nível multiplicava o seguinte, e o conteúdo terminava desenhado fora da
    /// janela: a tela abria vazia com tudo pintado à direita dela.
    #[test]
    fn porcentagem_dentro_de_scroll_e_da_caixa() {
        let dom = parse_html_to_dom(
            "<div style='overflow-y:auto; padding-left:40px; padding-right:40px'>               <div id='meio' style='width:100%'>                 <div style='width:3000px'>conteudo bem mais largo que a caixa</div>               </div>             </div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let lista = layout_document(&dom, &ctx);
        let meio = dom.resolve(dom.query("#meio").unwrap()).unwrap();
        let rect = lista.geometry().rects[&meio];
        assert!(
            (rect.w - 920.0).abs() < 1.0,
            "100% da caixa (1000 - 80 de padding) e não do conteúdo: {rect:?}"
        );
        // E o conteúdo largo continua transbordando — o container rola, não corta.
        let geo = lista.geometry();
        let largo = dom
            .node(meio)
            .children
            .iter()
            .copied()
            .find(|c| matches!(dom.node(*c).kind, NodeKind::Element { .. }))
            .expect("o filho largo");
        assert!(geo.rects[&largo].w > 2900.0, "o filho largo mantém a largura dele");
    }

    /// SEQUÊNCIA LONGA de mutações sorteadas: o reuso tem de bater com o cálculo
    /// do zero em todas elas.
    ///
    /// O teste dirigido acima cobre os caminhos que eu sabia listar; este cobre
    /// as COMBINAÇÕES, que é onde um cache erra — invalidar A e depois B, mexer
    /// num nó recém-inserido, remover o que acabou de mudar. O gerador é um LCG
    /// com semente fixa: a sequência é sempre a mesma, então uma falha é
    /// reproduzível e o teste nunca fica intermitente.
    #[test]
    fn sequencia_longa_de_mutacoes_mantem_a_equivalencia() {
        let mut dom = parse_html_to_dom(
            "<style>.a{padding:4px}.b{margin:2px}.t{font-size:14px}</style>             <main id='root'>               <div class='a'><p class='t'>um</p><p>dois</p></div>               <div class='b'><span>tres</span> quatro <b>cinco</b></div>               <ul id='l'><li>x</li><li>y</li></ul>             </main>",
        );
        let ctx = LayoutCtx { viewport_w: 500.0, viewport_h: 400.0, measurer: &ApproxMeasurer };
        let raiz = dom.query("#root").unwrap();
        let lista = dom.query("#l").unwrap();
        let mut semente = 0x2545_F491_4F6C_DD1Du64;
        let mut sorteia = move |n: u64| {
            semente = semente.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (semente >> 33) % n
        };
        let mut vivos: Vec<crate::dom::NodeId> = dom.query_all("p, li, span, b");

        for passo in 0..120 {
            match sorteia(5) {
                0 => {
                    if !vivos.is_empty() {
                        let alvo = vivos[sorteia(vivos.len() as u64) as usize];
                        dom.set_text(alvo, &format!("t{passo}"));
                    }
                }
                1 => {
                    if !vivos.is_empty() {
                        let alvo = vivos[sorteia(vivos.len() as u64) as usize];
                        let classe = ["a", "b", "t", "sem-regra"][sorteia(4) as usize];
                        dom.set_attr(alvo, "class", classe);
                    }
                }
                2 => {
                    let novo = dom.create_element(if passo % 2 == 0 { "li" } else { "p" });
                    let txt = dom.create_text_node(&format!("novo {passo}"));
                    dom.append_child(novo, txt);
                    let pai = if passo % 3 == 0 { lista } else { raiz };
                    dom.append_child(pai, novo);
                    vivos.push(novo);
                }
                3 => {
                    if vivos.len() > 3 {
                        let i = sorteia(vivos.len() as u64) as usize;
                        let alvo = vivos.remove(i);
                        dom.remove_node(alvo);
                    }
                }
                _ => {
                    if vivos.len() > 2 {
                        // move um nó para o fim da lista (reordena irmãos)
                        let alvo = vivos[sorteia(vivos.len() as u64) as usize];
                        dom.append_child(lista, alvo);
                    }
                }
            }

            let reusado = layout_cached(&dom, &ctx);
            dom.clear_fragment_cache();
            let zero = layout_document(&dom, &ctx);
            assert_eq!(
                reusado.materialized().len(),
                zero.materialized().len(),
                "passo {passo}: nº de itens diverge"
            );
            for (i, (a, b)) in reusado.materialized().iter().zip(&zero.materialized()).enumerate() {
                assert!(
                    itens_equivalentes(a, b),
                    "passo {passo}, item {i}:
  reuso: {a:?}
  zero:  {b:?}"
                );
            }
        }
    }

    /// O caminho CACHEADO e o cálculo do zero produzem a mesma coisa, depois de
    /// cada mutação de uma sequência que passa por texto, atributo, classe,
    /// inserção e remoção.
    ///
    /// É o guarda de qualquer reuso de geometria: um cache que devolve a lista
    /// errada tem exatamente a mesma cara de um cache rápido, e o único jeito de
    /// distinguir os dois é recalcular e comparar. Vale hoje (o reuso é
    /// tudo-ou-nada) e continua valendo quando o reuso for por subárvore.
    #[test]
    fn o_layout_reusado_e_igual_ao_recalculado() {
        let mut dom = parse_html_to_dom(
            "<style>.card{padding:8px;margin:4px}.t{font-size:18px}.hi{color:#ff0000}</style>             <main id='root'>               <div class='card'><h3 class='t'>Um titulo</h3><p>texto de exemplo aqui</p></div>               <div class='card'><h3 class='t'>Outro</h3><p>mais texto <b>em negrito</b> junto</p></div>               <ul id='lista'><li>a</li><li>b</li><li>c</li></ul>             </main>",
        );
        let ctx = LayoutCtx { viewport_w: 640.0, viewport_h: 480.0, measurer: &ApproxMeasurer };
        let alvo = dom.query("p").unwrap();
        let card = dom.query(".card").unwrap();
        let lista = dom.query("#lista").unwrap();

        let mut passo = 0;
        let conferir = |dom: &Dom, passo: &mut i32| {
            let cacheado = layout_cached(dom, &ctx);
            // Do ZERO: sem os fragmentos, nada é reusado e o resultado é o que o
            // layout produz quando calcula tudo. Comparar o reuso com ele mesmo
            // seria o mesmo que não comparar.
            dom.clear_fragment_cache();
            let recalculado = layout_document(dom, &ctx);
            assert_eq!(
                cacheado.materialized().len(),
                recalculado.materialized().len(),
                "quantidade de itens diverge no passo {passo}"
            );
            for (i, (a, b)) in cacheado.materialized().iter().zip(&recalculado.materialized()).enumerate() {
                assert!(
                    itens_equivalentes(a, b),
                    "item {i} diverge no passo {passo}:
  reuso: {a:?}
  cálculo: {b:?}"
                );
            }
            assert!(
                (cacheado.content_height - recalculado.content_height).abs() < TOL,
                "altura divergente no passo {passo}"
            );
            let mut a: Vec<_> = cacheado.geometry().rects.iter().map(|(i, r)| (*i, *r)).collect();
            let mut b: Vec<_> = recalculado.geometry().rects.iter().map(|(i, r)| (*i, *r)).collect();
            a.sort_by_key(|(idx, _)| *idx);
            b.sort_by_key(|(idx, _)| *idx);
            assert_eq!(a.len(), b.len(), "nº de retângulos diverge no passo {passo}");
            for ((ia, ra), (ib, rb)) in a.iter().zip(&b) {
                assert_eq!(ia, ib, "nós diferentes no passo {passo}");
                assert!(rects_equivalentes(ra, rb), "rect de {ia} diverge no passo {passo}");
            }
            *passo += 1;
        };

        conferir(&dom, &mut passo);
        dom.set_text(alvo, "outro texto bem mais longo do que o anterior era");
        conferir(&dom, &mut passo);
        dom.set_attr(alvo, "class", "hi");
        conferir(&dom, &mut passo);
        dom.set_attr(alvo, "class", "classe-que-ninguem-cita");
        conferir(&dom, &mut passo);
        let novo = dom.create_element("li");
        let txt = dom.create_text_node("d");
        dom.append_child(novo, txt);
        dom.append_child(lista, novo);
        conferir(&dom, &mut passo);
        dom.remove_node(card);
        conferir(&dom, &mut passo);
        dom.set_inner_html(lista, "<li>x</li><li>y</li>");
        conferir(&dom, &mut passo);
        assert_eq!(passo, 7, "todos os passos foram conferidos");
    }

    /// O cache de layout devolve a MESMA lista enquanto nada muda, e uma NOVA
    /// depois de qualquer mutação. Sem a segunda metade, "o frame parado custa
    /// zero" seria só outra forma de dizer que a página parou de atualizar.
    #[test]
    fn cache_de_layout_reusa_e_invalida() {
        let mut dom = parse_html_to_dom("<div id='a'><p>um</p></div>");
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let first = layout_cached(&dom, &ctx);
        let again = layout_cached(&dom, &ctx);
        assert!(std::rc::Rc::ptr_eq(&first, &again), "nada mudou: a lista é a mesma");

        let alvo = dom.query("#a").unwrap();
        dom.set_text(alvo, "outro");
        let after = layout_cached(&dom, &ctx);
        assert!(!std::rc::Rc::ptr_eq(&first, &after), "o texto mudou: lista nova");

        // Viewport diferente também é outro layout.
        let narrow = LayoutCtx { viewport_w: 300.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let small = layout_cached(&dom, &narrow);
        assert!(!std::rc::Rc::ptr_eq(&after, &small));
    }

    /// Registra `<div>` como bloco vertical (os testes precisam que a tag tenha
    /// layout de bloco para entrar no caminho `layout_block` dos filhos).
    fn def_div() {
        crate::block::define(
            "div",
            crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
        );
    }

    /// Layout determinístico com medidor aproximado e viewport fixo.
    fn layout(html: &str, vw: f32) -> DisplayList {
        def_div();
        let dom = parse_html_to_dom(html);
        let ctx = LayoutCtx { viewport_w: vw, viewport_h: 600.0, measurer: &ApproxMeasurer };
        layout_document(&dom, &ctx)
    }

    #[test]
    fn cache_de_medidas_invalida_largura_mutada() {
        def_div();
        let mut dom = parse_html_to_dom(
            "<div id='host' style='display:flex'><div id='card' style='width:100px;height:10px'></div></div>",
        );
        let card = dom.query("#card").unwrap();
        let card_idx = dom.resolve(card).unwrap();
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };

        let first = layout_document(&dom, &ctx);
        let _warm = layout_document(&dom, &ctx);
        let before = first.geometry().rects[&card_idx].w;
        dom.set_style_property(card, "width", "200px");
        let after = layout_document(&dom, &ctx).geometry().rects[&card_idx].w;

        assert!((before - 100.0).abs() < 0.1);
        assert!((after - 200.0).abs() < 0.1);
    }

    #[test]
    fn cache_intrinseca_invalida_texto_mutado() {
        def_div();
        let mut dom = parse_html_to_dom(
            "<div id='host' style='display:flex'><span id='text' style='display:inline-block'>a</span></div>",
        );
        let text = dom.query("#text").unwrap();
        let text_idx = dom.resolve(text).unwrap();
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };

        let before = layout_document(&dom, &ctx).geometry().rects[&text_idx].w;
        let _warm = layout_document(&dom, &ctx);
        dom.set_text(text, "uma linha de texto bem mais comprida");
        let after = layout_document(&dom, &ctx).geometry().rects[&text_idx].w;

        assert!(after > before, "a largura intrínseca deve acompanhar o novo texto");
    }

    #[test]
    fn hit_test_escolhe_o_no_mais_profundo() {
        // Filho dentro do pai: clicar dentro do filho devolve o FILHO (menor
        // área = mais profundo); clicar no pai fora do filho devolve o pai;
        // fora de tudo devolve None.
        def_div();
        let dom = parse_html_to_dom(
            "<div id=pai style='padding:50px'><div id=filho style='height:20px'>x</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let pai = dom.query("#pai").unwrap();
        let filho = dom.query("#filho").unwrap();
        let pai_idx = dom.resolve(pai).unwrap();
        let filho_idx = dom.resolve(filho).unwrap();
        let fr = list.geometry().rects[&filho_idx];
        let hit = list.hit_test(fr.x + fr.w / 2.0, fr.y + fr.h / 2.0);
        assert_eq!(hit, Some(filho_idx));
        // canto do pai (dentro do padding, fora do filho).
        let pr = list.geometry().rects[&pai_idx];
        let hit2 = list.hit_test(pr.x + 5.0, pr.y + 5.0);
        assert_eq!(hit2, Some(pai_idx));
        // fora de tudo.
        assert_eq!(list.hit_test(-10.0, -10.0), None);
    }

    #[test]
    fn elemento_inline_recebe_bounding_rect() {
        let dom = parse_html_to_dom("<div><span id='s'>texto</span></div>");
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let span = dom.query("#s").unwrap();
        let span_idx = dom.resolve(span).unwrap();
        let geo = list.geometry();
        let rect = geo.rects.get(&span_idx).expect("inline deveria ter rect");
        assert!(rect.w > 0.0);
        assert!(rect.h > 0.0);
    }

    /// Um `<a>` dentro de um parágrafo ocupa a sua fatia da linha: começa DEPOIS
    /// do texto que o antecede e é mais estreito do que o parágrafo inteiro.
    /// Sem isto respondia `0,0,0,0` — inexistente para hit-test e para medição.
    #[test]
    fn link_no_meio_do_paragrafo_tem_caixa_na_sua_fatia_da_linha() {
        let dom = parse_html_to_dom("<p>antes <a id='l'>link</a> depois</p>");
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#l").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("o <a> devia ter caixa");
        assert!(r.x > 0.0, "começa depois do texto que o antecede: x={}", r.x);
        assert!(r.w > 0.0 && r.w < 800.0, "largura da sua fatia, não a do <p>: w={}", r.w);
        assert!(r.h > 0.0);
    }

    /// Um `<a>` que quebra em duas linhas tem UMA caixa que contém as duas — é a
    /// definição da spec (bounding box dos fragmentos), e é o que
    /// `getBoundingClientRect` devolve no browser.
    #[test]
    fn link_partido_em_duas_linhas_tem_caixa_que_contem_as_duas() {
        let texto = "palavra ".repeat(40);
        let dom = parse_html_to_dom(&format!("<p><a id='l'>{texto}</a></p>"));
        let ctx = LayoutCtx { viewport_w: 200.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#l").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("o <a> devia ter caixa");
        // várias linhas: a altura é a soma delas, muito acima de uma linha só.
        let uma_linha = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        assert!(r.h > uma_linha * 2.0, "devia abranger várias linhas: h={}", r.h);
        // e a união é larga como a coluna, não como o último fragmento.
        assert!(r.w > 100.0, "a união abrange a largura da coluna: w={}", r.w);
    }

    /// Um `<img>` inline é um REPLACED element: tem caixa própria com a largura e
    /// a altura dos atributos, mesmo sem pixels descodificados (é o caso de uma
    /// página real medida sem rede). Antes não gerava run nenhum e ficava a zero.
    #[test]
    fn img_inline_tem_caixa_propria_dos_atributos() {
        let dom = parse_html_to_dom("<p>antes <img id='i' width='40' height='30'> depois</p>");
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#i").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("o <img> devia ter caixa");
        assert_eq!((r.w, r.h), (40.0, 30.0));
        assert!(r.x > 0.0, "está depois do texto que o antecede: x={}", r.x);
    }

    /// Um inline VAZIO (`<source>` dentro de um `<picture>`) tem no browser
    /// largura zero mas posição e altura reais — não é `0,0,0,0`.
    #[test]
    fn inline_vazio_tem_posicao_e_altura_sem_largura() {
        let dom = parse_html_to_dom(
            "<p>antes <picture><source id='s'><img width='40' height='30'></picture></p>",
        );
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#s").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("o <source> devia ter caixa");
        assert_eq!(r.w, 0.0, "um inline vazio não tem largura");
        assert!(r.x > 0.0 && r.h > 0.0, "mas tem posição e altura de linha: {r:?}");
    }

    /// Um inline vazio SOZINHO num bloco não inventa uma linha: o bloco continua
    /// com a mesma altura. É o corte que separa "acrescentar geometria" de
    /// "mudar o layout".
    #[test]
    fn inline_vazio_sozinho_nao_cria_linha() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let com = layout_document(&parse_html_to_dom("<div><span></span></div>"), &ctx);
        let sem = layout_document(&parse_html_to_dom("<div></div>"), &ctx);
        assert_eq!(com.content_height, sem.content_height);
    }



    /// Um inline com CAIXA (fundo/padding) no meio de um parágrafo continua na
    /// linha: o parágrafo tem a altura de uma linha, não de três. Era o que
    /// multiplicava a altura de uma página real — cada `<span>` com fundo
    /// fechava o fluxo inline e abria linha nova.
    #[test]
    fn inline_com_caixa_nao_parte_a_linha() {
        let ctx = LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let linhas = |html: &str| {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let mut ys: Vec<f32> = Vec::new();
            list.walk(|it, _dx, dy| {
                if let DisplayItem::Text { y, .. } = it {
                    ys.push(y + dy);
                }
            });
            ys.sort_by(f32::total_cmp);
            ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
            ys.len()
        };
        assert_eq!(linhas("<p>antes <span>simples</span> depois</p>"), 1);
        assert_eq!(linhas("<p>antes <span style='background:#eee'>com caixa</span> depois</p>"), 1);
    }


    /// O `line-height` declarado vale em QUALQUER contexto, não só no fluxo
    /// inline: o mesmo parágrafo dentro de um `display:flex` respondia a altura
    /// do medidor (16×1.125) e ignorava a folha, porque o caminho de flex media
    /// o texto solto perguntando direto ao medidor. Uma linha de 16px com
    /// `line-height:2` tem 32px de altura, esteja onde estiver.
    #[test]
    fn line_height_declarado_vale_tambem_dentro_de_flex() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let altura = |html: &str| {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let idx = dom.resolve(dom.query("#alvo").unwrap()).unwrap();
            list.geometry().rects.get(&idx).expect("o alvo devia ter caixa").h
        };
        let sem = altura("<div id='alvo' style='display:flex'>uma linha</div>");
        let com = altura("<div id='alvo' style='display:flex; line-height:2'>uma linha</div>");
        assert_eq!(sem, ApproxMeasurer.line_height(DEFAULT_FONT_SIZE));
        assert_eq!(com, 32.0, "16 × 2 = 32, e não a altura do medidor");
    }

    /// Um `<span>` filho de um `display:flex` é um item de flex, e um item de
    /// flex é BLOCKIFICADO pela spec — tem caixa própria, com a sua posição e o
    /// seu tamanho. O Chrome reporta `display:block` nele.
    ///
    /// Antes era achatado para uma string e pintado com o estilo do CONTAINER:
    /// não registava caixa (eram 345 dos 351 elementos `display:block` sem caixa
    /// da Wikipédia) e perdia a sua própria cor pelo caminho.
    #[test]
    fn span_filho_de_flex_tem_caixa_propria() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let dom = parse_html_to_dom(
            "<div style='display:flex'><span id='a'>um</span><span id='b'>dois</span></div>",
        );
        let list = layout_document(&dom, &ctx);
        let geo = list.geometry();
        let caixa = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            *geo.rects.get(&idx).unwrap_or_else(|| panic!("{sel} devia ter caixa"))
        };
        let (a, b) = (caixa("#a"), caixa("#b"));
        assert!(a.w > 0.0 && a.h > 0.0, "o primeiro tem tamanho: {a:?}");
        assert!(b.x >= a.x + a.w, "o segundo à direita do primeiro: {a:?} {b:?}");
    }

    /// A cor de um `<span>` filho de flex é a DELE, não a do container — o mesmo
    /// achatamento que lhe tirava a caixa pintava o texto com o estilo do pai.
    #[test]
    fn span_filho_de_flex_pinta_com_a_sua_propria_cor() {
        let list = layout(
            "<div style='display:flex; color:#0000ff'><span style='color:#ff0000'>vermelho</span></div>",
            600.0,
        );
        let texts = all_texts(&list);
        assert_eq!(texts.len(), 1, "{texts:?}");
        assert_eq!(texts[0].3, 0xFF0000FF, "a cor do span, não a do container");
    }


    /// `<br>` fecha a linha: o texto depois dele começa uma linha nova, e o
    /// próprio `<br>` tem posição e altura de linha com largura zero — é o que o
    /// browser reporta. Antes não quebrava nada: as duas linhas saíam como uma.
    #[test]
    fn br_quebra_a_linha_e_tem_a_sua_caixa() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let dom = parse_html_to_dom("<div>antes<br id='b'>depois</div>");
        let list = layout_document(&dom, &ctx);
        let mut ys: Vec<f32> = Vec::new();
        list.walk(|it, _dx, dy| {
            if let DisplayItem::Text { y, .. } = it {
                ys.push(y + dy);
            }
        });
        ys.sort_by(f32::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        assert_eq!(ys.len(), 2, "duas linhas, não uma: {ys:?}");
        let idx = dom.resolve(dom.query("#b").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("o <br> devia ter caixa");
        assert_eq!(r.w, 0.0);
        assert!(r.h > 0.0, "altura de linha: {r:?}");
    }

    /// A caixa de um elemento inline é a CONTENT AREA DA FONTE, não a caixa de
    /// linha: com `line-height: 3`, o `<a>` continua a ter a altura da fonte e
    /// fica CENTRADO na linha pela meia-entrelinha. O `line-height` decide o
    /// espaçamento (onde a linha seguinte começa), não o tamanho do inline.
    ///
    /// Dar-lhe a altura da linha somava ~8px por elemento numa página com
    /// `line-height: 26px` — 3 032 `<a>` na Wikipédia, ~24 500px de excesso.
    #[test]
    fn caixa_do_inline_e_a_altura_da_fonte_nao_a_da_linha() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let fonte = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        let dom = parse_html_to_dom(
            "<div style='line-height:3'>antes <a id='l'>link</a> depois</div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#l").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("o <a> devia ter caixa");
        assert_eq!(r.h, fonte, "a altura da FONTE, não os 48 da linha");
        // meia-entrelinha: (48 − 18) / 2 = 15 acima.
        assert_eq!(r.y, (3.0 * DEFAULT_FONT_SIZE - fonte) / 2.0, "centrado na linha: {r:?}");
    }


    #[test]
    fn tmp_a() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        for html in [
            "<div style='line-height:1.625'>t <a id='x'>link</a> f</div>",
            "<div style='line-height:1.625'><a id='x'>link</a></div>",
            "<div style='line-height:1.625'><a id='x' style='color:#06c'>link</a> f</div>",
            "<li style='line-height:1.625'><a id='x'>link</a></li>",
            "<div style='line-height:1.625'><a id='x' style='padding:2px'>link</a></div>",
            "<div style='line-height:1.625'><a id='x'><img width='20' height='15'></a></div>",
        ] {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
            eprintln!("DIAG {:<66} -> {:?}", html, list.geometry().rects.get(&idx));
        }
    }

    /// `border-radius` sozinho NÃO tira um elemento do fluxo inline: um raio sem
    /// fundo nem borda não pinta nada e não cria caixa. O `has_box` conta-o
    /// porque responde a outra pergunta ("há algo a pintar por este caminho").
    ///
    /// Não é um caso de laboratório: 5 262 dos 5 263 `<a>` da Wikipédia eram
    /// blockificados só por isto, e por isso nenhuma correção do fluxo inline
    /// lhes tocava — eles nunca lá chegavam.
    #[test]
    fn radius_sozinho_nao_tira_o_elemento_do_fluxo_inline() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let fonte = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        let dom = parse_html_to_dom(
            "<div style='line-height:2'>t <a id='x' style='border-radius:2px'>link</a> f</div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("o <a> devia ter caixa");
        // Se tivesse sido blockificado, a altura seria a da LINHA (32) e o x seria
        // zero (caixa própria a começar na margem do bloco).
        assert_eq!(r.h, fonte, "altura da fonte, logo continua inline: {r:?}");
        assert!(r.x > 0.0, "flui depois do texto que o antecede: {r:?}");
    }

    /// Um `<a>` à volta de uma imagem grande NÃO fica do tamanho dela: no browser
    /// a caixa de um inline tem a LARGURA do que ele contém e a ALTURA DA FONTE.
    ///
    /// Medido na Wikipédia antes de o corpus mudar: um `<a>` com uma imagem de
    /// 600x528 responde `600x17` no Chrome, com o topo a 254px do topo da
    /// imagem — que é a meia-entrelinha da linha que a imagem tornou alta.
    /// Nós dávamos-lhe os 528, e era o maior erro de altura da página inteira.
    #[test]
    fn inline_a_volta_de_uma_imagem_mantem_a_altura_da_fonte() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let fonte = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        let dom = parse_html_to_dom(
            "<div><span id='s'><a id='a'><img id='i' width='300' height='200'></a></span></div>",
        );
        let list = layout_document(&dom, &ctx);
        let geo = list.geometry();
        let caixa = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            *geo.rects.get(&idx).unwrap_or_else(|| panic!("{sel} sem caixa"))
        };
        let (img, a, span) = (caixa("#i"), caixa("#a"), caixa("#s"));
        assert_eq!((img.w, img.h), (300.0, 200.0), "a imagem tem a sua caixa");
        assert_eq!(a.h, fonte, "o <a> mede a fonte, não a imagem: {a:?}");
        assert_eq!(a.w, 300.0, "mas ocupa a largura dela na linha: {a:?}");
        assert_eq!((span.w, span.h), (a.w, a.h), "e o <span> à volta, o mesmo");
        // e fica centrado na linha que a imagem tornou alta.
        assert_eq!(a.y, (200.0 - fonte) / 2.0, "meia-entrelinha: {a:?}");
    }


    /// Um `checkbox`/`radio` é um quadradinho de 13x13, não um campo de texto de
    /// 190x26 — e a medida que o fluxo RESERVA na linha é a mesma que a emissão
    /// pinta, porque agora as duas perguntam à mesma função.
    #[test]
    fn checkbox_e_um_quadrado_e_nao_um_campo_de_texto() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let caixa = |html: &str| {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
            *list.geometry().rects.get(&idx).expect("o input devia ter caixa")
        };
        let c = caixa("<div>a <input id='x' type='checkbox'> b</div>");
        assert_eq!((c.w, c.h), (13.0, 13.0), "quadrado intrínseco: {c:?}");
        let r = caixa("<div>a <input id='x' type='radio'> b</div>");
        assert_eq!((r.w, r.h), (13.0, 13.0), "o radio idem: {r:?}");
        // o campo de texto continua a ser um campo de texto.
        let t = caixa("<div>a <input id='x' type='text'> b</div>");
        assert!(t.w > 100.0, "campo de texto mantém a largura de campo: {t:?}");
    }

    /// `height: %` num `<input>` mede-se contra a ALTURA do containing block, não
    /// contra a largura. A Wikipédia usa o "checkbox hack" — oito
    /// `<input type=checkbox>` com `height:100%` — e cada um vinha com a largura
    /// da viewport de altura: o pior rácio de erro da página inteira.
    #[test]
    fn altura_percentual_de_input_mede_se_no_eixo_vertical() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let dom = parse_html_to_dom(
            "<div style='height:400px'><input id='x' type='checkbox' style='height:100%'></div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("sem caixa");
        assert_eq!(r.h, 400.0, "100% da ALTURA do pai, não da largura: {r:?}");
    }


    /// `display:none` num ANCESTRAL remove a subárvore inteira do layout — e um
    /// `position:absolute` lá dentro não é exceção.
    ///
    /// Era o pior número da página: um `<input type=checkbox; height:100%>` de um
    /// menu escondido da Wikipédia continuava a ser medido, e como o pai
    /// escondido não tem caixa, a procura do containing block saltava-o e
    /// ancorava-o num contentor com a altura do DOCUMENTO — 96 665px de altura
    /// para um controlo invisível.
    #[test]
    fn absolute_dentro_de_display_none_nao_tem_caixa() {
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let dom = parse_html_to_dom(
            "<div style='position:relative;height:400px'>               <div style='display:none;position:relative'>                 <i id='x' style='position:absolute;height:100%'>a</i>               </div>             </div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
        assert!(
            list.geometry().rects.get(&idx).is_none(),
            "um absoluto num ramo escondido não gera caixa: {:?}",
            list.geometry().rects.get(&idx)
        );
        // e o que NÃO está escondido continua a ser posicionado.
        let dom = parse_html_to_dom(
            "<div style='position:relative;height:400px'>               <i id='y' style='position:absolute;height:100%'>a</i>             </div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#y").unwrap()).unwrap();
        let r = *list.geometry().rects.get(&idx).expect("este devia ter caixa");
        assert_eq!(r.h, 400.0, "100% da altura do containing block: {r:?}");
    }

    #[test]
    fn hit_test_respeita_z_index_em_elementos_sobrepostos() {
        let dom = parse_html_to_dom(
            "<style>#back { position:absolute; left:0; top:0; width:200px; height:200px; z-index:10 } #front { position:absolute; left:0; top:0; width:100px; height:100px; z-index:0 }</style><div id='back'></div><div id='front'></div>",
        );
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let back = dom.resolve(dom.query("#back").unwrap()).unwrap();
        assert_eq!(list.hit_test(50.0, 50.0), Some(back));
    }

    /// Primeiro `SolidRect` da lista (o fundo da 1ª caixa) — atalho de assert.
    ///
    /// PLANA (`materialized`), como o `all_rects`: desde que a saída passou a ser
    /// uma árvore de fragmentos, o fundo de um filho de bloco vive no fragmento
    /// dele e não no buffer próprio da lista. Ler `list.items` direto respondia
    /// "não há SolidRect nenhum" numa página que pinta — o erro não estava no
    /// motor, estava na navegação.
    fn first_rect(list: &DisplayList) -> Rect {
        list.materialized()
            .iter()
            .find_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("esperava ao menos um SolidRect")
    }

    /// Todos os itens de TEXTO, na ordem de pintura: `(texto, x, y, cor)`.
    /// Mesma razão do `first_rect` para não ler `list.items`.
    fn all_texts(list: &DisplayList) -> Vec<(String, f32, f32, u32)> {
        list.materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { text, x, y, color, .. } => {
                    Some((text.to_string(), *x, *y, *color))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn block_ocupa_largura_do_container() {
        // <div> sem width: bloco ocupa a largura do viewport menos o frame.
        // Aqui só padding=10 (margin/border=0): content = 600 - 20 = 580; a CAIXA
        // (content+padding) = 600 (largura cheia).
        let list = layout("<div style='background:#112233; padding:10'>x</div>", 600.0);
        let r = first_rect(&list);
        assert_eq!(r.x, 0.0);
        assert_eq!(r.y, 0.0);
        assert_eq!(r.w, 600.0); // content(580) + padding(2×10) = 600
    }

    #[test]
    fn width_percent_resolve_contra_container() {
        // width:50% de um viewport 800 → content=400; sem padding/border a caixa=400.
        let list = layout("<div style='background:#111111; width:50%'>x</div>", 800.0);
        let r = first_rect(&list);
        assert_eq!(r.w, 400.0); // 50% de 800
        assert_eq!(r.x, 0.0);
    }

    #[test]
    fn blocos_empilham_vertical() {
        // dois <div> com altura de 1 linha empilham: o 2º começa abaixo do 1º.
        // Sem box (sem bg) — só checa o Y das linhas de texto.
        //
        // Lido pela ÁRVORE (`walk`) e não por `list.items`: um filho de bloco é
        // emitido como FRAGMENTO, e os itens dele vivem no fragmento. Ler
        // `items` direto respondia zero textos e o teste falhava a dizer que os
        // blocos não pintavam, quando o que não pintava era a leitura.
        let list = layout("<div>um</div><div>dois</div>", 600.0);
        let mut texts: Vec<f32> = Vec::new();
        list.walk(|it, _dx, dy| {
            if let DisplayItem::Text { y, .. } = it {
                texts.push(y + dy);
            }
        });
        texts.sort_by(f32::total_cmp);
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0], 0.0); // primeiro no topo
        // uma linha do medidor aproximado: 16 × 1.125.
        let uma_linha = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        assert!(texts[1] >= uma_linha, "segundo bloco abaixo do primeiro (y={})", texts[1]);
    }

    #[test]
    fn fundo_vem_antes_do_texto_filho_no_zorder() {
        // O SolidRect (fundo) deve estar ANTES do Text na lista (pinta atrás).
        let list = layout("<div style='background:#222222; padding:8'>oi</div>", 600.0);
        let i_rect = list.materialized().iter().position(|it| matches!(it, DisplayItem::SolidRect { .. }));
        let i_text = list.materialized().iter().position(|it| matches!(it, DisplayItem::Text { .. }));
        assert!(i_rect < i_text, "fundo (idx {i_rect:?}) deve vir antes do texto (idx {i_text:?})");
    }

    #[test]
    fn box_model_content_box_offset_do_texto() {
        // content-box: o texto começa deslocado por margin+border+padding.
        // padding=14, border=2, margin=6 → offset = 22. (MDN: outer = m+b+p+content)
        //
        // O `border-style: solid` é indispensável e não é decoração do teste:
        // `border-width` SOZINHO não cria borda nenhuma, porque o estilo por
        // omissão é `none` e uma borda que não pinta também não ocupa espaço.
        // Sem ele o offset certo passa a ser 20 (só margem e padding) — foi o
        // que este teste passou a acusar quando as bordas por lado começaram a
        // respeitar o estilo, e a premissa errada era a do teste.
        let list = layout(
            "<div style='background:#111111; padding:14; border-width:2; border-style:solid; margin:6'>z</div>",
            600.0,
        );
        let txt = all_texts(&list).first().map(|(_, x, y, _)| (*x, *y)).expect("texto");
        assert_eq!(txt.0, 22.0); // x = margin(6)+border(2)+padding(14)
        assert_eq!(txt.1, 22.0); // y idem
        // a caixa (fundo) NÃO inclui a margin: começa em (6,6).
        let r = first_rect(&list);
        assert_eq!(r.x, 6.0);
        assert_eq!(r.y, 6.0);
    }

    #[test]
    fn tres_cards_empilham_no_vertical() {
        // <div> vertical (default): 3 cards empilham — mesmo x, Y crescente, cada
        // um com sua caixa de 30% de 900 = 270.
        let list = layout(
            "<div style='background:#111;width:30%'>a</div>\
             <div style='background:#222;width:30%'>b</div>\
             <div style='background:#333;width:30%'>c</div>",
            900.0,
        );
        let rects: Vec<Rect> = all_rects(&list);
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.x == 0.0)); // mesmo x (vertical)
        assert!(rects.iter().all(|r| (r.w - 270.0).abs() < 0.01)); // 30% de 900
        assert!(rects[0].y < rects[1].y && rects[1].y < rects[2].y); // Y crescente
    }

    #[test]
    fn cards_lado_a_lado_no_horizontal() {
        // <row display:horizontal> com 3 <div> cada 30% → ficam LADO A LADO: X
        // crescente, MESMO y (topo), cada caixa 270 de largura. (O caso do
        // stat-card: era isto que o egui colapsava; agora o layout do DOM resolve.)
        crate::block::define(
            "row",
            crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
        );
        let dom = parse_html_to_dom(
            "<row>\
               <div style='background:#111;width:30%'>a</div>\
               <div style='background:#222;width:30%'>b</div>\
               <div style='background:#333;width:30%'>c</div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 900.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = all_rects(&list);
        assert_eq!(rects.len(), 3);
        // mesmo Y (lado a lado, não empilhado).
        assert!(rects.iter().all(|r| r.y == rects[0].y), "todos no mesmo topo: {rects:?}");
        // X crescente: card 2 à direita do 1, card 3 à direita do 2.
        assert!(rects[0].x < rects[1].x && rects[1].x < rects[2].x, "X crescente: {rects:?}");
        // cada caixa 30% de 900 = 270 (a % resolve contra o content do <row>).
        assert!(rects.iter().all(|r| (r.w - 270.0).abs() < 1.0), "largura ~270: {rects:?}");
        // o 2º começa onde o 1º termina (sem sobrepor): x[1] ≈ x[0] + w[0].
        assert!((rects[1].x - (rects[0].x + rects[0].w)).abs() < 1.0, "encostados: {rects:?}");
    }

    #[test]
    fn cards_com_filhos_nao_esticam_o_ultimo() {
        // REGRESSÃO (bug visto na tela): 3 cards width:32% COM filhos (<p>) num <row>
        // largo — o ÚLTIMO não pode esticar até a borda. Cada um = 32% da largura,
        // o resto fica vazio à direita (como no navegador). p=wrap pra bater o real.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("p", crate::block::BlockDef { display: 1, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<row>\
               <div style='background:#111;width:32%'><p>256</p><p>testes</p></div>\
               <div style='background:#222;width:32%'><p>31%</p><p>paridade</p></div>\
               <div style='background:#333;width:32%'><p>5</p><p>fases</p></div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        assert_eq!(rects.len(), 3);
        // TODOS com a MESMA largura = 32% de 1000 = 320 (o 3º NÃO estica).
        for (i, r) in rects.iter().enumerate() {
            assert!((r.w - 320.0).abs() < 1.0, "card[{i}] devia ter 320 (32%), tem {}: {rects:?}", r.w);
        }
        // o último termina BEM antes da borda (3×320=960 < 1000), sobra vazio.
        let last = rects[2];
        assert!(last.x + last.w <= 1000.0, "último não passa da borda: {last:?}");
    }

    #[test]
    fn border_box_faz_3_cards_caberem() {
        // box-sizing:border-box: width:32% INCLUI padding+border → a CAIXA é 32%,
        // 3 cards = 96% (cabem, sobra ~4%). Sem border-box (content-box) cada caixa
        // seria 32%+frame e estouraria. Prova a propriedade real do CSS.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>.card{box-sizing:border-box;width:32%;padding:14;border-width:2;background:#1a2030}</style>\
             <row>\
               <div class='card'>a</div><div class='card'>b</div><div class='card'>c</div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        assert_eq!(rects.len(), 3);
        // cada CAIXA = 32% de 1000 = 320 (border-box: o width É a caixa inteira).
        for (i, r) in rects.iter().enumerate() {
            assert!((r.w - 320.0).abs() < 1.0, "card[{i}] caixa=320 (border-box): {rects:?}");
        }
        // 3×320=960 < 1000: cabem com folga (sobra ~40 = 4%).
        let last = rects[2];
        assert!(last.x + last.w <= 1000.0, "cabem todos: {rects:?}");
        assert!(1000.0 - (last.x + last.w) >= 30.0, "sobra espaço à direita: {rects:?}");
    }

    #[test]
    fn min_max_width_clamp() {
        // VALIDADO no Chrome: used_width = clamp(min, width, max) (#1751).
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let cases = [
            ("width:500px;max-width:300px", 300.0),  // max limita
            ("width:50px;min-width:200px", 200.0),   // min eleva
            ("width:1000px;max-width:400px;min-width:100px", 400.0), // clamp
            ("width:600px;max-width:50%", 400.0),    // % de 800
        ];
        for (style, expected) in cases {
            let dom = parse_html_to_dom(&format!("<div id=\"t\" style=\"{style}\">x</div>"));
            let t = dom.query("#t").unwrap();
            let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
            let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();
            assert!((rect.w - expected).abs() < 1.0, "{style}: w={} esperado {expected}", rect.w);
        }
    }

    #[test]
    fn min_max_height_clamp() {
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        // height:500 max-height:200 → caixa de 200.
        let dom = parse_html_to_dom("<div id=\"t\" style=\"height:500px;max-height:200px;width:100px\">x</div>");
        let t = dom.query("#t").unwrap();
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();
        assert!((rect.h - 200.0).abs() < 1.0, "max-height: h={}", rect.h);
        // min-height:300 num conteúdo pequeno → caixa de 300.
        let dom2 = parse_html_to_dom("<div id=\"t\" style=\"min-height:300px;width:100px\">x</div>");
        let t2 = dom2.query("#t").unwrap();
        let rect2 = bounding_rect(&dom2, dom2.resolve(t2).unwrap(), &ctx).unwrap();
        assert!(rect2.h >= 300.0, "min-height: h={}", rect2.h);
    }

    #[test]
    fn text_align_desloca_o_texto() {
        // text-align center/right desloca o texto pelo espaço livre (#1749).
        let dom = parse_html_to_dom("<style>#c{text-align:center;width:400px}#r{text-align:right;width:400px}</style><div id=\"c\">x</div><div id=\"r\">y</div>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(String, f32)> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::Text { text, x, .. } => Some((text.to_string(), *x)),
            _ => None,
        }).collect();
        // "x" (1 char, ~8px = 16×0.5) centrado em 400 → x ≈ (400-8)/2 = 196.
        let cx = texts.iter().find(|(t, _)| t == "x").unwrap().1;
        assert!((cx - 196.0).abs() < 2.0, "center: {cx}");
        // "y" à direita → x ≈ 400-8 = 392.
        let rx = texts.iter().find(|(t, _)| t == "y").unwrap().1;
        assert!((rx - 392.0).abs() < 2.0, "right: {rx}");
    }

    #[test]
    fn svg_reserva_a_caixa() {
        // um `<svg>` reserva a caixa (width/height do atributo, ou razão do
        // viewBox) mesmo sem desenhar o vetor — o logo/ícones ocupam o espaço.
        let dom = parse_html_to_dom(
            "<div><svg id=logo width=272 height=92 viewBox='0 0 272 92'></svg></div>\
             <svg id=ico width=24 height=24></svg>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let r = |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let logo = r("#logo");
        assert!((logo.w - 272.0).abs() < 1.0 && (logo.h - 92.0).abs() < 1.0, "logo: {}x{}", logo.w, logo.h);
        let ico = r("#ico");
        assert!((ico.w - 24.0).abs() < 1.0 && (ico.h - 24.0).abs() < 1.0, "ico: {}x{}", ico.w, ico.h);
    }

    #[test]
    fn grid_fr_track_sizing() {
        // grid-template-columns: 200px 1fr 2fr num container 620 → 200/140/280.
        let dom = parse_html_to_dom(
            "<div style='display:grid;grid-template-columns:200px 1fr 2fr;width:620px'>\
             <div id=a>A</div><div id=b>B</div><div id=c>C</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rect = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            list.geometry().rects[&idx]
        };
        let a = rect("#a"); let b = rect("#b"); let c = rect("#c");
        assert!((a.w - 200.0).abs() < 1.0, "col px: {}", a.w);
        assert!((b.w - 140.0).abs() < 1.0, "col 1fr: {}", b.w); // (620-200)/3
        assert!((c.w - 280.0).abs() < 1.0, "col 2fr: {}", c.w); // 2×140
        assert!((b.x - 200.0).abs() < 1.0 && (c.x - 340.0).abs() < 1.0, "posições");
    }

    #[test]
    fn area_nomeada_poe_sidebar_e_conteudo_lado_a_lado() {
        // Sem áreas nomeadas os dois filhos caem na colocação automática de um grid
        // de 1 coluna e EMPILHAM — que é o que punha o artigo da Wikipédia fora da
        // viewport. Com a matriz, o `lado` fica na coluna 0 e o `conteudo` na 1, na
        // MESMA linha.
        let dom = parse_html_to_dom(
            "<div style=\"display:grid;width:600px;grid-template-columns:200px 1fr;\
             grid-template-areas:'lado conteudo'\">\
             <div id=b style='grid-area:conteudo'>conteudo</div>\
             <div id=a style='grid-area:lado'>lado</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rect = |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let a = rect("#a");
        let b = rect("#b");
        assert!((a.y - b.y).abs() < 1.0, "mesma linha: a.y={} b.y={}", a.y, b.y);
        assert!(b.x > a.x, "conteudo à direita do lado: a.x={} b.x={}", a.x, b.x);
        // a ordem do DOM tem o conteúdo PRIMEIRO — a matriz é que manda, não ela.
        assert!((a.x - 0.0).abs() < 1.0 && (a.w - 200.0).abs() < 1.0, "lado: x={} w={}", a.x, a.w);
        assert!((b.x - 200.0).abs() < 1.0 && (b.w - 400.0).abs() < 1.0, "conteudo: x={} w={}", b.x, b.w);
    }

    #[test]
    fn area_que_atravessa_colunas_cobre_o_gap() {
        // 'topo topo' / 'lado conteudo': o topo ocupa as DUAS colunas, e o span
        // inclui o gap do meio (senão o cabeçalho ficaria 24px mais estreito que a
        // linha que ele encima).
        let dom = parse_html_to_dom(
            "<div style=\"display:grid;width:624px;column-gap:24px;\
             grid-template-columns:200px 400px;\
             grid-template-areas:'topo topo' 'lado conteudo'\">\
             <div id=t style='grid-area:topo'>t</div>\
             <div id=l style='grid-area:lado'>l</div>\
             <div id=c style='grid-area:conteudo'>c</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rect = |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let t = rect("#t");
        let l = rect("#l");
        let c = rect("#c");
        assert!((t.w - 624.0).abs() < 1.0, "topo cobre as 2 colunas + gap: {}", t.w);
        assert!(l.y > t.y, "lado abaixo do topo: t.y={} l.y={}", t.y, l.y);
        assert!((l.y - c.y).abs() < 1.0, "lado e conteudo na mesma linha");
        assert!((c.x - 224.0).abs() < 1.0, "conteudo após 200px + 24 de gap: {}", c.x);
    }

    #[test]
    fn filho_sem_grid_area_continua_na_colocacao_automatica() {
        // Um item nomeado NÃO desliga o auto-placement dos outros: o sem nome cai na
        // primeira célula livre, que é a linha implícita abaixo da área ocupada.
        let dom = parse_html_to_dom(
            "<div style=\"display:grid;width:400px;grid-template-columns:1fr 1fr;\
             grid-template-areas:'x x'\">\
             <div id=n style='grid-area:x'>n</div><div id=s>s</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rect = |sel: &str| list.geometry().rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let n = rect("#n");
        let s = rect("#s");
        assert!((n.w - 400.0).abs() < 1.0, "nomeado ocupa as 2 colunas: {}", n.w);
        assert!(s.y > n.y, "sem nome vai para a linha seguinte: n.y={} s.y={}", n.y, s.y);
        assert!((s.x - 0.0).abs() < 1.0, "e para a 1ª coluna livre: {}", s.x);
    }

    #[test]
    fn grid_align_items_center_centraliza_na_celula() {
        // single-column grid de altura fixa + align-items:center → o item de
        // altura menor centraliza verticalmente na track (o logo do google).
        let dom = parse_html_to_dom(
            "<div style='display:grid;align-items:center;height:240px'>\
             <div id=logo style='height:92px'>x</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#logo").unwrap()).unwrap();
        let r = list.geometry().rects[&idx];
        assert!((r.y - 74.0).abs() < 2.0, "y centralizado: {} (esperado 74=(240-92)/2)", r.y);
        assert!((r.h - 92.0).abs() < 2.0, "altura preservada: {}", r.h);
    }

    #[test]
    fn calc_de_altura_resolve_contra_a_altura() {
        // `calc(100% - 560px)` num `height` resolve o `%` contra a ALTURA do
        // containing block (800), não a largura (1000): 800-560=240, não 440.
        let dom = parse_html_to_dom(
            "<div style='height:800px'>\
             <div id=c style='height:calc(100% - 560px);background:#eee'>x</div>\
             </div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let c = dom.query("#c").unwrap();
        let idx = dom.resolve(c).unwrap();
        assert!((list.geometry().rects[&idx].h - 240.0).abs() < 2.0,
            "calc height: {} (esperado 240 = 800-560)", list.geometry().rects[&idx].h);
    }

    #[test]
    fn flex_grow_vertical_da_altura_a_filho_100pct() {
        // flex column 800px: navbar 60 + item flex-grow:1 (cresce p/ 740) e o
        // filho height:100% do item resolve contra os 740 (não a altura própria).
        let dom = parse_html_to_dom(
            "<div style='display:flex;flex-direction:column;height:800px'>\
             <div style='height:60px'>nav</div>\
             <div style='flex-grow:1'><div id=alvo style='height:100%;background:#00f'>x</div></div>\
             </div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let alvo = dom.query("#alvo").unwrap();
        let idx = dom.resolve(alvo).unwrap();
        let r = list.geometry().rects[&idx];
        assert!((r.h - 740.0).abs() < 2.0, "altura do filho 100%: {} (esperado 740)", r.h);
        assert!((r.y - 60.0).abs() < 2.0, "y do filho: {}", r.y);
    }

    #[test]
    fn absolute_ancora_no_containing_block() {
        // `position:absolute` com right:0/top:0 ancora no canto do ANCESTRAL
        // positioned (relative), não do viewport (o padrão do google: ícone no
        // canto da caixa de busca).
        let dom = parse_html_to_dom(
            "<div style='position:relative;width:400px;height:50px;margin-left:100px'>\
             <span style='position:absolute;top:0px;right:0px;width:30px;height:30px;background:#00f'>i</span>\
             </div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        // o span azul: canto direito da caixa (x=100, w=400 → 500) menos a largura.
        let sp = list.materialized().iter().find_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == 0x0000FFFF => Some(*rect),
            _ => None,
        }).expect("span absolute");
        assert!((sp.x - 470.0).abs() < 2.0, "x do abs: {} (esperado ~470 = 100+400-30)", sp.x);
        assert!(sp.y.abs() < 2.0 || (sp.y - 0.0).abs() < 2.0, "y do abs: {}", sp.y);
    }

    #[test]
    fn link_ua_azul_sublinhado_por_run() {
        // `<a>` sem CSS de autor: cor azul default + underline (deco=1) SÓ no seu
        // texto — o texto adjacente do parágrafo fica preto e sem decoração.
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom("<p>antes <a href=x>link</a> depois</p>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let segs: Vec<(String, u32, u8)> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::Text { text, color, decoration, .. } => {
                Some((text.to_string(), *color, *decoration))
            }
            _ => None,
        }).collect();
        let link = segs.iter().find(|(t, ..)| t.contains("link")).expect("run do link");
        assert_eq!(link.1, 0x0000EEFF, "link azul");
        assert_eq!(link.2, 1, "link sublinhado");
        // o texto ao redor (preto, sem deco) é um segmento SEPARADO.
        assert!(segs.iter().any(|(t, c, d)| t.contains("antes") && *c == 0x000000FF && *d == 0));
    }

    #[test]
    fn line_height_e_text_transform() {
        // line-height do CSS respeitado + text-transform aplicado (#1749). Usa <div>
        // (sem margin default da UA, ao contrário de <p>) p/ isolar o line-height.
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom("<style>div{line-height:3;text-transform:uppercase}</style><div>oi</div><div>tchau</div>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(String, f32)> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::Text { text, y, .. } => Some((text.to_string(), *y)),
            _ => None,
        }).collect();
        // uppercase aplicado.
        assert!(texts.iter().any(|(t, _)| t == "OI"));
        assert!(texts.iter().any(|(t, _)| t == "TCHAU"));
        // line-height:3 = 3×16 = 48px entre as linhas (div sem margin).
        let y_oi = texts.iter().find(|(t, _)| t == "OI").unwrap().1;
        let y_tchau = texts.iter().find(|(t, _)| t == "TCHAU").unwrap().1;
        assert!((y_tchau - y_oi - 48.0).abs() < 5.0, "line-height: {y_oi} → {y_tchau}");
    }

    #[test]
    fn display_vem_do_css_nao_do_defineblock() {
        // O `display:flex` no <style> faz <row> dispor os filhos LADO A LADO, sem
        // precisar de defineBlock. `display:none` some. É o motor lendo o display DO
        // CSS. (`<div>` é block via a UA-stylesheet `ua.ts` em produção; nos testes
        // unitários — sem o prelude TS — registramos o default à mão.)
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>row{display:flex} hide{display:none} \
                    .c{width:30%;background:#111}</style>\
             <row>\
               <div class='c'>a</div><div class='c'>b</div><div class='c'>c</div>\
             </row>\
             <hide>invisível</hide>",
        );
        let ctx = LayoutCtx { viewport_w: 900.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        assert_eq!(rects.len(), 3, "3 cards (o <hide> display:none não pinta)");
        // display:flex do CSS → lado a lado (X crescente, mesmo Y).
        assert!(rects[0].x < rects[1].x && rects[1].x < rects[2].x, "lado a lado: {rects:?}");
        assert!(rects.iter().all(|r| r.y == rects[0].y), "mesma linha: {rects:?}");
        // display:none → o texto "invisível" NÃO está na lista.
        let has_invisivel = list.materialized().iter().any(|it| matches!(it, DisplayItem::Text { text, .. } if text.contains("invisível")));
        assert!(!has_invisivel, "display:none não renderiza o conteúdo");
    }

    #[test]
    fn margin_vertical_empilha_sem_deslocar_horizontal() {
        // margin_v (UA-stylesheet) separa blocos no VERTICAL mas NÃO empurra no
        // eixo horizontal (como `margin: Npx 0` do navegador para h1/p). Dois
        // parágrafos com margin_v: o 2º começa mais abaixo, mas ambos em x=0.
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        crate::style::define_style("p", crate::style::SLOT_MARGIN_V, 16);
        let dom = parse_html_to_dom("<p>um</p><p>dois</p>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(f32, f32)> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::Text { x, y, .. } => Some((*x, *y)),
            _ => None,
        }).collect();
        assert_eq!(texts.len(), 2);
        // X: ambos em 0 (margin VERTICAL não desloca horizontal).
        assert_eq!(texts[0].0, 0.0, "1º texto em x=0: {texts:?}");
        assert_eq!(texts[1].0, 0.0, "2º texto em x=0 (margin não empurrou): {texts:?}");
        // Y: o 2º bem abaixo (margin colapsado entre eles + altura da linha).
        assert!(texts[1].1 > texts[0].1 + 20.0, "2º empilhado abaixo: {texts:?}");
    }

    #[test]
    fn bounding_rect_dos_cards() {
        // getBoundingClientRect: o border-box de cada nó-bloco. Os 3 cards (flex,
        // 32% border-box) têm os MESMOS rects que o dump mostra (x=20/322/624, w=302).
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>.card{box-sizing:border-box;width:32%;padding:14;border-width:2;background:#1a2030}</style>\
             <row>\
               <div class='card' id='a'>1</div><div class='card' id='b'>2</div><div class='card' id='c'>3</div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        // resolve os NodeIdx dos 3 cards e mede cada um.
        let a = dom.resolve(dom.query("#a").unwrap()).unwrap();
        let b = dom.resolve(dom.query("#b").unwrap()).unwrap();
        let c = dom.resolve(dom.query("#c").unwrap()).unwrap();
        let ra = bounding_rect(&dom, a, &ctx).expect("card a tem rect");
        let rb = bounding_rect(&dom, b, &ctx).expect("card b tem rect");
        let rc = bounding_rect(&dom, c, &ctx).expect("card c tem rect");
        // border-box = 32% de 1000 = 320 cada; lado a lado.
        assert!((ra.w - 320.0).abs() < 1.0, "largura ~320: {ra:?}");
        assert!((rb.w - 320.0).abs() < 1.0);
        assert!((rc.w - 320.0).abs() < 1.0);
        assert_eq!(ra.x, 0.0); // (sem padding no body de teste, x começa em 0)
        assert!(rb.x > ra.x && rc.x > rb.x, "X crescente: {ra:?} {rb:?} {rc:?}");
        assert_eq!(ra.y, rb.y); // mesma linha (flex)
    }

    #[test]
    fn bounding_rect_none_para_texto() {
        // texto/inline não tem rect próprio (a API só dá rect de elemento-bloco).
        let dom = parse_html_to_dom("<p>oi</p>");
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let p = dom.resolve(dom.query("p").unwrap()).unwrap();
        // o <p> (bloco) TEM rect.
        assert!(bounding_rect(&dom, p, &ctx).is_some());
        // o nó de texto filho NÃO tem (não é bloco).
        let txt = dom.node(p).children[0];
        assert!(bounding_rect(&dom, txt, &ctx).is_none());
    }

    /// Helper: layout de um HTML num row flex e os rects (x ordenado) dos N cards.
    fn flex_card_rects(style: &str, n_cards: usize, vw: f32) -> Vec<Rect> {
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        // flex-shrink:0 nos cards: estes testes validam JUSTIFY (inclusive em
        // overflow real, como o Chrome foi medido); sem o 0, o shrink default=1
        // (agora implementado!) encolheria os itens e não haveria overflow.
        let mut html = format!("<style>row{{display:flex;{style}}} .c{{width:100px;flex-shrink:0;background:#111}}</style><row>");
        for i in 0..n_cards {
            html.push_str(&format!("<div class='c' id='c{i}'>x</div>"));
        }
        html.push_str("</row>");
        let dom = parse_html_to_dom(&html);
        let ctx = LayoutCtx { viewport_w: vw, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let mut rects: Vec<Rect> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        rects.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        rects
    }

    #[test]
    fn flex_gap_separa_itens() {
        // gap:20px entre 3 cards de 100px: x = 0, 120, 240.
        let r = flex_card_rects("gap:20px", 3, 600.0);
        assert_eq!(r.len(), 3);
        assert!((r[0].x - 0.0).abs() < 0.5, "{r:?}");
        assert!((r[1].x - 120.0).abs() < 0.5, "card2 em 100+20: {r:?}");
        assert!((r[2].x - 240.0).abs() < 0.5, "card3 em 220+20: {r:?}");
    }

    #[test]
    fn flex_justify_content() {
        // 3 cards de 100 num container de 600 → free = 600-300 = 300.
        // space-between: x = 0, 100+150=250, 200+300=500.
        let r = flex_card_rects("justify-content:space-between", 3, 600.0);
        assert!((r[0].x - 0.0).abs() < 0.5, "{r:?}");
        assert!((r[1].x - 250.0).abs() < 0.5, "between=150: {r:?}");
        assert!((r[2].x - 500.0).abs() < 0.5, "flush no fim: {r:?}");
        // center: leading = 150 → x = 150, 250, 350.
        let r = flex_card_rects("justify-content:center", 3, 600.0);
        assert!((r[0].x - 150.0).abs() < 0.5, "center leading=150: {r:?}");
        assert!((r[2].x - 350.0).abs() < 0.5, "{r:?}");
        // flex-end: leading = 300 → x = 300, 400, 500.
        let r = flex_card_rects("justify-content:flex-end", 3, 600.0);
        assert!((r[0].x - 300.0).abs() < 0.5, "flex-end leading=300: {r:?}");
        // space-evenly: leading = between = 300/4 = 75 → x = 75, 250, 425.
        let r = flex_card_rects("justify-content:space-evenly", 3, 600.0);
        assert!((r[0].x - 75.0).abs() < 0.5, "evenly leading=75: {r:?}");
        assert!((r[1].x - 250.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_grow_distribui_o_espaco() {
        // O GRID do Bootstrap: `.col { flex: 1 0 0% }` — 3 colunas dividem o
        // container igualmente (base 0, grow distribui TUDO).
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<style>row{display:flex} .col{flex:1 0 0%;background:#111}</style>\
             <row><div class='col'>a</div><div class='col'>b</div><div class='col'>c</div></row>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3);
        assert!((r[0].w - 200.0).abs() < 0.5, "3 colunas iguais: {r:?}");
        assert!((r[1].x - 200.0).abs() < 0.5, "{r:?}");
        assert!((r[2].x - 400.0).abs() < 0.5, "{r:?}");
        // grow 1:2 → 200 e 400.
        let l2 = layout(
            "<style>row{display:flex} .a{flex:1 0 0%;background:#111} .b{flex:2 0 0%;background:#222}</style>\
             <row><div class='a'>a</div><div class='b'>b</div></row>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!((r2[0].w - 200.0).abs() < 0.5, "grow 1: {r2:?}");
        assert!((r2[1].w - 400.0).abs() < 0.5, "grow 2: {r2:?}");
    }

    #[test]
    fn flex_shrink_encolhe_em_overflow() {
        // shrink DEFAULT = 1 (fiel ao CSS): 2 itens de 400 em 600 encolhem para
        // 300 cada; com shrink:0 no primeiro, ele mantém 400 e o outro cede.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<style>row{display:flex} .c{width:400px;background:#111}</style>\
             <row><div class='c'>a</div><div class='c'>b</div></row>",
            600.0,
        );
        let r = all_rects(&list);
        assert!((r[0].w - 300.0).abs() < 0.5, "shrink default encolhe: {r:?}");
        assert!((r[1].w - 300.0).abs() < 0.5, "{r:?}");
        let l2 = layout(
            "<style>row{display:flex} .fix{width:400px;flex-shrink:0;background:#111} .c{width:400px;background:#222}</style>\
             <row><div class='fix'>a</div><div class='c'>b</div></row>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!((r2[0].w - 400.0).abs() < 0.5, "shrink:0 nao cede: {r2:?}");
        assert!((r2[1].w - 200.0).abs() < 0.5, "o flexivel cede tudo: {r2:?}");
    }

    #[test]
    fn flex_order_e_align_self() {
        // `order` reordena visualmente (menor primeiro); `align-self` vence o
        // align-items do container; STRETCH real estica o item sem height.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<style>row{display:flex;height:100px;align-items:flex-start}\
              .a{order:2;width:100px;height:20px;background:#111}\
              .b{order:1;width:100px;height:20px;align-self:center;background:#222}\
              .s{align-self:stretch;width:100px;background:#333}</style>\
             <row><div class='a'>a</div><div class='b'>b</div><div class='s'>s</div></row>",
            600.0,
        );
        // os rects saem na ordem VISUAL (pós-order): .s (0) → .b (1) → .a (2).
        let r = all_rects(&list);
        assert!((r[0].x - 0.0).abs() < 0.5, ".s primeiro (order 0): {r:?}");
        assert!((r[1].x - 100.0).abs() < 0.5, ".b no meio (order 1): {r:?}");
        assert!((r[2].x - 200.0).abs() < 0.5, ".a por ultimo (order 2): {r:?}");
        // align-self:stretch do .s (sem height): estica até a linha (100) — o
        // align-items:flex-start do container é VENCIDO pelo align-self.
        assert!((r[0].h - 100.0).abs() < 0.5, "stretch estica: {r:?}");
        // align-self:center do .b: y = (100-20)/2 = 40.
        assert!((r[1].y - 40.0).abs() < 0.5, "align-self center: {r:?}");
        // .a fica no topo (align-items:flex-start do container).
        assert!((r[2].y - 0.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_justify_overflow() {
        // 3 cards de 100 em 200 (overflow real = -100). VALIDADO contra Chrome
        // (flex-shrink:0): os space-* caem para flex-start → x = 0, 100, 200.
        for jc in ["space-between", "space-around", "space-evenly", "flex-start"] {
            let r = flex_card_rects(&format!("justify-content:{jc}"), 3, 200.0);
            assert!((r[0].x - 0.0).abs() < 0.5, "{jc} overflow→start: {r:?}");
            assert!((r[1].x - 100.0).abs() < 0.5, "{jc}: {r:?}");
            assert!((r[2].x - 200.0).abs() < 0.5, "{jc}: {r:?}");
        }
        // center em overflow: leading = free/2 = -50 → x = -50, 50, 150 (Chrome).
        let r = flex_card_rects("justify-content:center", 3, 200.0);
        assert!((r[0].x + 50.0).abs() < 0.5, "center overflow leading=-50: {r:?}");
        assert!((r[2].x - 150.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_align_center_usa_altura_do_container() {
        // VALIDADO no Chrome: bar height:80, cards height:40, align-items:center
        // → cards em y=20 (centrados na altura DO CONTAINER, não na linha de 40).
        crate::block::define("bar", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>bar{display:flex;align-items:center;height:80px} .c{width:100px;height:40px;background:#ff0000}</style>\
             <bar><div class='c'>a</div><div class='c'>b</div></bar>",
        );
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let ys: Vec<f32> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(rect.y),
            _ => None,
        }).collect();
        assert!(ys.iter().all(|&y| (y - 20.0).abs() < 0.5), "cards centrados em y=20: {ys:?}");
    }

    #[test]
    fn flex_align_items_center() {
        // 1 card baixo + 1 alto: com align-items:center o baixo desce metade da folga.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>row{display:flex;align-items:center} .a{height:20px;width:50px;background:#111111} .b{height:60px;width:50px;background:#222222}</style>\
             <row><div class='a' id='a'>x</div><div class='b' id='b'>y</div></row>",
        );
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<(f32, f32)> = list.materialized().iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some((rect.x, rect.y)),
            _ => None,
        }).collect();
        // ordena por x: o card 'a' (baixo, x menor) deve ter y MAIOR que o 'b' (alto).
        let mut s = rects.clone(); s.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
        assert!(s[0].1 > s[1].1, "card baixo centralizado desce: {s:?}");
    }

    #[test]
    fn badges_fluem_e_quebram_linha_no_wrap() {
        // <tags display:wrap> com badges: fluem lado a lado e QUEBRAM para a próxima
        // linha quando não cabem (inline-block flow). Cada badge dimensiona pelo
        // conteúdo (shrink-to-fit), não estica para a largura toda.
        crate::block::define(
            "tags",
            crate::block::BlockDef { display: 1, indent: 0.0, prefix: 0, flags: 0 },
        );
        crate::block::define(
            "badge",
            crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
        );
        // 4 badges; numa largura estreita (200) eles não cabem todos numa linha.
        let dom = parse_html_to_dom(
            "<tags>\
               <badge style='background:#111;padding:6'>rust</badge>\
               <badge style='background:#222;padding:6'>cranelift</badge>\
               <badge style='background:#333;padding:6'>typescript</badge>\
               <badge style='background:#444;padding:6'>egui</badge>\
             </tags>",
        );
        let ctx = LayoutCtx { viewport_w: 200.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = all_rects(&list);
        assert_eq!(rects.len(), 4);
        // shrink-to-fit: nenhum badge ocupa a largura toda (200) — cada um é estreito.
        assert!(rects.iter().all(|r| r.w < 150.0), "badges estreitos (conteúdo): {rects:?}");
        // QUEBROU linha: há pelo menos 2 valores distintos de Y (não todos na mesma linha).
        let ys: std::collections::BTreeSet<i32> = rects.iter().map(|r| r.y as i32).collect();
        assert!(ys.len() >= 2, "deve haver quebra de linha (Ys distintos): {rects:?}");
        // o primeiro badge começa no canto (x=0).
        assert_eq!(rects[0].x, 0.0);
    }

    /// Coleta todos os SolidRect da lista, em ordem (container primeiro, filhos
    /// depois — o fundo do container é inserido ATRÁS dos filhos).
    fn all_rects(list: &DisplayList) -> Vec<Rect> {
        // PLANA: os itens de uma subárvore reusada não estão no buffer próprio,
        // e um teste que lesse só ele veria a página pela metade.
        list.materialized()
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn flex_column_empilha_com_gap() {
        // `flex-direction:column`: itens empilham na VERTICAL (main = Y) com o gap
        // entre eles; align default (stretch) → cada item ocupa a largura.
        let list = layout(
            "<div style='display:flex; flex-direction:column; gap:10; background:#111'>\
               <div style='background:#222; height:30'>a</div>\
               <div style='background:#333; height:40'>b</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3, "container + 2 filhos: {r:?}");
        // filhos: o 1º em y=0; o 2º abaixo (30 + gap 10 = 40) — NÃO lado a lado.
        assert_eq!(r[1].y, 0.0);
        assert_eq!((r[1].h, r[2].h), (30.0, 40.0));
        assert_eq!(r[2].y, 40.0);
        assert_eq!(r[1].x, r[2].x, "mesmo X (coluna, não row)");
        // stretch (default): os itens ocupam a largura do container.
        assert_eq!(r[1].w, 600.0);
    }

    #[test]
    fn flex_column_margin_auto_empurra() {
        // O padrão do Bootstrap cover: header + main(mt-auto/mb-auto) + footer numa
        // coluna com altura — os margins auto ABSORVEM o espaço livre e centralizam
        // o main (spec flexbox §8.1; mb-auto/mt-auto).
        let list = layout(
            "<div style='display:flex; flex-direction:column; height:300; background:#111'>\
               <div style='background:#222; height:20'>h</div>\
               <div style='background:#333; height:60; margin-top:auto; margin-bottom:auto'>m</div>\
               <div style='background:#444; height:20'>f</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 4);
        // free = 300 - (20+60+20) = 200; 2 autos → 100 cada.
        assert_eq!(r[1].y, 0.0); // header no topo
        assert_eq!(r[2].y, 120.0); // main: 20 + 100 (mt-auto)
        assert_eq!(r[3].y, 280.0); // footer: 120 + 60 + 100 (mb-auto)
    }

    #[test]
    fn flex_column_justify_center() {
        // justify-content atua no eixo PRINCIPAL (Y em column) quando o container
        // tem altura: um item de 50 num container de 300 centra em y=125.
        let list = layout(
            "<div style='display:flex; flex-direction:column; height:300; justify-content:center'>\
               <div style='background:#222; height:50'>x</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].y, 125.0, "{r:?}");
    }

    #[test]
    fn height_percent_resolve_contra_altura_do_pai() {
        // `height:%` resolve contra a ALTURA do containing block (antes resolvia
        // errado contra a LARGURA). Pai height:200 → filho 50% = 100.
        let list = layout(
            "<div style='height:200; background:#111'>\
               <div style='height:50%; background:#222'>x</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].h, 200.0);
        assert_eq!(r[1].h, 100.0, "50% de 200: {r:?}");
        // pai SEM height (auto): height:% do filho vira auto (altura do conteúdo,
        // 1 linha ≈ 26) — fiel ao browser, não 50% da largura (que daria 300).
        let l2 = layout(
            "<div style='background:#111'><div style='height:50%; background:#222'>x</div></div>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!(r2[1].h < 40.0, "height %% com pai auto = altura natural: {r2:?}");
    }

    #[test]
    fn unidades_relativas_em_padding_e_margem_negativa() {
        // padding: 1rem = 16px (root 16, o default de browser) — o `p-3` do
        // Bootstrap; margem NEGATIVA puxa (os gutters `.row` usam margin -12px).
        let list = layout("<div style='background:#111; padding:1rem'>x</div>", 600.0);
        let tx = all_texts(&list).first().map(|(_, x, y, _)| (*x, *y)).unwrap();
        assert_eq!(tx, (16.0, 16.0), "texto após o padding de 1rem = 16px");
        // margem negativa: o segundo bloco com margin-top:-10 SOBE sobre o primeiro.
        let l2 = layout(
            "<div style='background:#111; height:30'>a</div>             <div style='background:#222; height:30; margin-top:-10px'>b</div>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert_eq!(r2[1].y, 20.0, "30 - 10 (negativa nao clampa): {r2:?}");
    }

    #[test]
    fn max_width_em_bate_com_o_chrome() {
        // `max-width: 42em` com root 16 = 672px — o `.cover-container` do Bootstrap
        // cover, VALIDADO numero-a-numero no Chrome (viewport 1000: rect 164,0,672).
        let list = layout(
            "<div style='background:#111; max-width:42em; margin:0 auto; height:50'>x</div>",
            1000.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].w, 672.0, "42em x 16: {r:?}");
        assert_eq!(r[0].x, 164.0, "(1000-672)/2, centrado como no Chrome");
    }

    #[test]
    fn inline_flow_links_fluem_no_paragrafo() {
        // P4 (o coracao): <p>texto <a>link</a>, fim</p> flui numa UNICA linha —
        // antes cada filho virava linha propria (o footer do cover saia em 5
        // linhas). A pontuacao NAO ganha espaco (fiel ao fonte: "Bootstrap, by").
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<p style='color:#ffffff'>Cover template for <a style='color:#ff0000'>Bootstrap</a>, by <a style='color:#00ff00'>mdo</a>.</p>",
            600.0,
        );
        let texts: Vec<(String, f32, f32, u32)> = all_texts(&list);
        // TODOS os segmentos na MESMA linha (y igual).
        let y0 = texts[0].2;
        assert!(texts.iter().all(|(_, _, y, _)| *y == y0), "uma linha so: {texts:?}");
        // os segmentos avancam em x (fluem lado a lado) e preservam a cor do span.
        assert!(texts.len() >= 4, "segmentos por cor: {texts:?}");
        assert!(texts.windows(2).all(|w| w[1].1 > w[0].1), "x crescente: {texts:?}");
        assert_eq!(texts[1].3, 0xFF0000FF, "cor do link preservada");
        // a virgula gruda no link (segmento seguinte comeca com ',' sem espaco).
        assert!(texts[2].0.starts_with(','), "pontuacao sem espaco: {:?}", texts[2].0);
    }

    #[test]
    fn float_left_right_dividem_a_linha() {
        // O header clássico (brand+nav do Bootstrap cover): float:left e
        // float:right consecutivos dividem a MESMA linha; o irmão não-float
        // começa abaixo do float mais alto; o pai contém os floats (BFC v1).
        let list = layout(
            "<div style='background:#111'>               <div style='float:left; background:#222; width:100; height:30'>brand</div>               <div style='float:right; background:#333; width:150; height:40'>nav</div>               <div style='background:#444; height:20'>abaixo</div>             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 4);
        assert_eq!((r[1].x, r[1].y), (0.0, 0.0), "left encosta na esquerda: {r:?}");
        assert_eq!((r[2].x, r[2].y), (450.0, 0.0), "right encosta na direita (600-150)");
        assert_eq!(r[3].y, 40.0, "nao-float comeca abaixo do float mais alto");
        assert_eq!(r[0].h, 60.0, "pai contem os floats: 40 + 20");
    }

    #[test]
    fn position_fixed_sai_do_fluxo_e_posiciona_no_viewport() {
        // O caso do dropdown do Bootstrap cover: um `position:fixed` DENTRO de um
        // flex row NÃO pode empurrar os irmãos (sai do fluxo) e pinta contra o
        // viewport pelos offsets (bottom/right).
        let list = layout(
            "<div style='display:flex; background:#111'>\
               <div style='position:fixed; bottom:10; right:10; width:50; height:20; background:#900'>t</div>\
               <div style='background:#222; height:30'>conteudo</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3);
        // o item NO FLUXO começa em x=0 (o fixed não o empurrou).
        assert_eq!(r[1].x, 0.0, "{r:?}");
        assert_eq!(r[1].h, 30.0);
        // o fixed: x = 600-50-10 = 540; y = viewport_h(600)-20-10 = 570. Pintado por
        // ÚLTIMO (por cima do fluxo).
        assert_eq!((r[2].x, r[2].y), (540.0, 570.0), "{r:?}");
        assert_eq!((r[2].w, r[2].h), (50.0, 20.0));
    }

    #[test]
    fn viewport_e_o_containing_block_da_raiz() {
        // `height:100%` no elemento RAIZ resolve contra a viewport_h (600 no
        // helper) — o `h-100` do html/body de páginas reais.
        //
        // Declarado em `html, body` e não num `<div>` de topo: o parser cria
        // `<html>`/`<body>` implícitos, como qualquer browser, e um `<div>` já
        // não é filho direto do documento — a percentagem dele resolve contra o
        // pai, de altura automática, que é o que o Chrome também faz. É por isto
        // mesmo que as páginas reais escrevem `html, body { height: 100% }` (a
        // Wikipédia escreve-o): a corrente de percentagens tem de partir da
        // raiz. O que o teste pina — a viewport é o containing block da raiz —
        // continua provado, e agora pelo caminho real.
        let list = layout(
            "<style>html,body{height:100%}</style><div style='height:100%;background:#111'>x</div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].h, 600.0, "{r:?}");
    }
    /// `visibility:hidden` esconde SEM tirar do fluxo — é o que o distingue de
    /// `display:none`, e a distinção decide layouts reais: o MediaWiki esconde
    /// os menus que abrem ao clique assim, e sem ela o menu aparecia por cima
    /// do artigo.
    #[test]
    fn visibility_hidden_ocupa_espaco_e_nao_pinta() {
        let dom = parse_html_to_dom(
            "<style>.oculto{visibility:hidden}</style>             <div class='oculto' style='height:50px;background:#ff0000'>invisível</div>             <div style='height:20px'>depois</div>",
        );
        let m = ApproxMeasurer;
        let ctx = LayoutCtx { viewport_w: 400.0, viewport_h: 300.0, measurer: &m };
        let lista = layout_document(&dom, &ctx);
        let itens = lista.materialized();
        // o segundo bloco começa DEPOIS dos 50px do primeiro: o espaço ficou.
        let segundo = dom.query_all("div")[1];
        let y = lista.rect_of(dom.resolve(segundo).unwrap()).map(|r| r.y).unwrap_or(0.0);
        assert!(y >= 50.0, "o elemento oculto tem de ocupar o espaço dele (y={y})");
        // e nada do que ele pinta tem alpha.
        for item in &itens {
            match item {
                DisplayItem::SolidRect { color, .. } if (color >> 8) == 0xFF0000 => {
                    assert_eq!(color & 0xFF, 0, "o fundo de um elemento oculto não é pintado");
                }
                DisplayItem::Text { text, color, .. } if text.contains("invisível") => {
                    assert_eq!(color & 0xFF, 0, "o texto de um elemento oculto não é pintado");
                }
                _ => {}
            }
        }
    }


    /// Um `<input>` com `opacity: 0` não pinta fundo nem borda.
    ///
    /// Vale uma página inteira: a Wikipédia usa o "checkbox hack" — um
    /// `<input type=checkbox>` invisível, dimensionado à altura do documento,
    /// para abrir menus sem JavaScript. Com o fundo branco da UA pintado opaco
    /// por cima de tudo, o que se via era uma janela EM BRANCO, com o layout e a
    /// lista de pintura inteiramente corretos.
    #[test]
    fn input_com_opacidade_zero_nao_pinta_fundo_nem_borda() {
        let dom = parse_html_to_dom("<input id='oculto' style='opacity:0;width:200px;height:100px'>");
        let m = ApproxMeasurer;
        let ctx = LayoutCtx { viewport_w: 400.0, viewport_h: 300.0, measurer: &m };
        let lista = layout_document(&dom, &ctx);
        for item in lista.materialized() {
            match item {
                DisplayItem::SolidRect { color, .. } | DisplayItem::Border { color, .. } => {
                    assert_eq!(color & 0xFF, 0, "um input invisível não pinta (cor #{color:08X})");
                }
                _ => {}
            }
        }
    }

    /// Um elemento com `background-color` E `mask-image` não emite fundo.
    ///
    /// É o ícone monocromático do MediaWiki (`.cdx-button__icon`: cor de fundo
    /// mais uma máscara que lhe dá a forma). Sem carregar a máscara, pintar o
    /// fundo dá o retângulo inteiro — os blocos cinzentos que apareciam no lugar
    /// do ☰ e da lupa na Wikipédia. O `-webkit-mask-image` conta igual: a folha
    /// real declara os dois lado a lado.
    #[test]
    fn elemento_com_mask_image_nao_pinta_fundo() {
        for prop in ["mask-image", "-webkit-mask-image"] {
            let html = format!(
                "<div style='background-color:#404244;{prop}:url(icone.svg);width:20px;height:20px'></div>"
            );
            let dom = parse_html_to_dom(&html);
            let m = ApproxMeasurer;
            let ctx = LayoutCtx { viewport_w: 400.0, viewport_h: 300.0, measurer: &m };
            let lista = layout_document(&dom, &ctx);
            for item in lista.materialized() {
                if let DisplayItem::SolidRect { color, .. } = item {
                    assert_ne!(
                        color, 0x404244FF,
                        "com `{prop}` o fundo não é pintado — sem a máscara seria um bloco"
                    );
                }
            }
        }
    }

    /// O mesmo fundo, SEM máscara declarada, continua a pintar — a supressão é
    /// da máscara, não uma exceção nova para a cor.
    #[test]
    fn elemento_sem_mask_image_pinta_o_fundo() {
        let dom = parse_html_to_dom(
            "<div style='background-color:#404244;width:20px;height:20px'></div>",
        );
        let m = ApproxMeasurer;
        let ctx = LayoutCtx { viewport_w: 400.0, viewport_h: 300.0, measurer: &m };
        let lista = layout_document(&dom, &ctx);
        let pintou = lista
            .materialized()
            .iter()
            .any(|i| matches!(i, DisplayItem::SolidRect { color, .. } if *color == 0x404244FF));
        assert!(pintou, "sem máscara, o fundo declarado é pintado");
    }
}

