//! A LISTA DE DESENHO: `Rect`, `Corners`, `DisplayItem`, `ScrollRegion`,
//! `DisplayList`, `Geometry` — o que o layout produz e o backend pinta.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
use crate::boxes::{BoxId, BoxTree};
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

    /// The bounding rectangle of `self` and `other`.
    ///
    /// The ONE union arithmetic: a box's fragments (`box_fragments.rs`), a
    /// node's boxes and a reused subtree's rects all fold through it. It has
    /// no sentinel to skip: a reserved placeholder is FLAGGED in its entry and
    /// replaced by the first real write, so "empty box at the origin" and "no
    /// box yet" cannot be confused — by construction, not by a special case.
    pub fn union(self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.w).max(other.x + other.w);
        let bottom = (self.y + self.h).max(other.y + other.h);
        Rect::new(x, y, right - x, bottom - y)
    }
}

/// Os quatro raios de canto de um retângulo pintado, em pontos.
///
/// Vive aqui e não em `style::radius` porque é o que a DISPLAY LIST carrega: um
/// número por canto, já resolvido, sem `Option` e sem cascata. O `ComputedStyle`
/// tem a pergunta ("foi declarado?"), este tem a resposta ("pinta assim").
///
/// Existe porque um raio só não representava o que 334 declarações do corpus
/// dizem: um canto declarado sozinho (`border-top-left-radius`) nunca tocava o
/// campo único — deliberadamente, porque escrevê-lo ali arredondaria os outros
/// três — e saía pintado QUADRADO. E `border-radius: 2px 2px 0 0`, a forma dos
/// cartões do Bootstrap, arredondava os quatro.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Corners {
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
}

impl Corners {
    pub const ZERO: Corners = Corners {
        tl: 0.0,
        tr: 0.0,
        br: 0.0,
        bl: 0.0,
    };

    /// Os quatro iguais — o que um `radius: f32` queria dizer.
    pub fn same(r: f32) -> Corners {
        Corners {
            tl: r,
            tr: r,
            br: r,
            bl: r,
        }
    }

    /// Algum canto arredonda?
    ///
    /// É uma pergunta sobre os QUATRO, e é a que o backend faz para decidir se
    /// pode recortar o retângulo ao visível. Respondê-la por um canto só faria um
    /// `<div>` de dezenas de milhares de pontos voltar inteiro ao tesselador —
    /// uma regressão de desempenho que nenhum teste de layout apanha, e por isso
    /// a pergunta é um método aqui em vez de uma comparação no consumidor.
    pub fn any(&self) -> bool {
        self.tl > 0.0 || self.tr > 0.0 || self.br > 0.0 || self.bl > 0.0
    }

    /// Os cantos de um estilo, com `default` para o que ninguém declarou.
    ///
    /// A ordem é canto → campo único → `default`. O campo único entra como
    /// fallback e não como override: é o que mantém a condição do lote anterior
    /// — `border-radius: 6px` escreve os dois, portanto os quatro cantos já
    /// respondem 6 e o fallback nem chega a ser consultado; quem só declarou um
    /// canto continua a não ver os outros três mexer.
    pub fn from_style(css: &ComputedStyle, default: f32) -> Corners {
        let um = |c: Option<f32>| c.or(css.corner_radius).unwrap_or(default);
        Corners {
            tl: um(css.corner_tl),
            tr: um(css.corner_tr),
            br: um(css.corner_br),
            bl: um(css.corner_bl),
        }
    }
}

/// UM item da display list — uma instrução de pintura ATÔMICA e já posicionada. O
/// backend percorre a lista em ordem (a ordem É o z-order: o que vem depois pinta
/// por cima) e desenha cada item, sem nenhuma decisão de layout. Egui-free: cor é
/// `u32` RGBA, posição é `f32` — nenhum tipo de backend.
#[derive(Clone, PartialEq, Debug)]
pub enum DisplayItem {
    /// Retângulo preenchido (fundo de uma caixa). `radius` arredonda os cantos —
    /// um valor POR CANTO, porque o CSS tem quatro e um cartão com
    /// `border-radius: 2px 2px 0 0` não é o mesmo desenho que um com `2px`.
    ///
    /// Os outros três itens com `radius: f32` (`Shadow`, `GradientRect`,
    /// `Border`) continuam com um valor só: mudá-los é a fatia seguinte, e
    /// enquanto não for feita respondem exatamente o que respondiam.
    SolidRect {
        rect: Rect,
        color: u32,
        radius: Corners,
    },
    /// SOMBRA de caixa (`box-shadow`): pintada ATRÁS da caixa. `dx`/`dy` deslocam,
    /// `blur` amacia a borda, `spread` cresce/encolhe o rect, `color` é a cor (com
    /// alpha). O backend usa o blur real do egui (`epaint::Shadow`).
    Shadow {
        rect: Rect,
        dx: f32,
        dy: f32,
        blur: f32,
        spread: f32,
        color: u32,
        radius: f32,
    },
    /// Retângulo com GRADIENTE LINEAR (`background: linear-gradient(...)`). Interpola
    /// `c0`→`c1` ao longo do ângulo `angle_deg` (0=para cima, 90=para a direita, como
    /// o CSS). O backend pinta como mesh de 4 vértices coloridos. `radius` arredonda
    /// (aproximado — o mesh não recorta os cantos; suficiente p/ heros/botões).
    GradientRect {
        rect: Rect,
        c0: u32,
        c1: u32,
        angle_deg: f32,
        radius: f32,
    },
    /// Borda (contorno) de uma caixa, espessura `width`, na cor dada.
    Border {
        rect: Rect,
        width: f32,
        color: u32,
        radius: f32,
    },
    /// Um QUADRILÁTERO convexo cheio, em coordenadas de conteúdo — a forma de
    /// UM lado de borda quando os lados adjacentes têm cores diferentes: o
    /// Blink junta-os numa diagonal do canto exterior ao interior (é o que
    /// desenha o triângulo de CSS), e uma barra rectangular por lado pinta o
    /// vizinho por cima. Só `border_items` o emite, e só nesse caso; com cores
    /// iguais a sobreposição é invisível e as barras continuam.
    Quad {
        pts: [(f32, f32); 4],
        color: u32,
    },
    /// IMAGEM (`<img>` / background-image) — um bitmap RGBA8 já decodificado. O
    /// `pixels_handle` é um Buffer no HandleTable com `img_w*img_h*4` bytes RGBA
    /// (a partir do offset `pixels_off`); o backend sobe como textura e pinta no
    /// `rect` (escalando). Decodificação/download acontecem ANTES (no browser .ts,
    /// via fetchBytes+imgdec); o rts-dom só carrega o handle+dims — segue wasm-safe.
    Image {
        rect: Rect,
        pixels_handle: u64,
        pixels_off: u32,
        img_w: u32,
        img_h: u32,
    },
    /// PIXELS que o próprio documento carrega (um `<canvas>` que o programa
    /// pintou), RGBA8, `w*h*4` bytes.
    ///
    /// Variante separada da `Image` porque a fonte é outra: aquela aponta para
    /// um `Buffer` de fora por handle — o `<img>` que o mini-browser baixou e
    /// decodificou — e esta CARREGA os bytes, porque quem pintou foi o programa
    /// e o desenho não tem outro dono. Um `Rc` para que passar a lista adiante
    /// não copie a imagem.
    Pixels {
        rect: Rect,
        data: std::rc::Rc<Vec<u8>>,
        w: u32,
        h: u32,
    },
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
        /// `true` quando a familia computada resolve na fonte de teste Ahem.
        ///
        /// Um BIT e nao a lista de familias: quem o le e o rasterizador, e a
        /// unica pergunta que ele faz e se pode desenhar o glifo como o
        /// retangulo solido que a Ahem define. Carregar a lista inteira por
        /// item de texto pagaria uma alocacao por fragmento reusado para
        /// responder a um booleano.
        ///
        /// Fora da Ahem o rasterizador continua a mascarar o texto, e isso e
        /// deliberado: desenhar uma fonte real precisa de um motor de fontes
        /// que este crate nao tem, e inventar retangulos para ela faria falhar
        /// reftests que hoje passam por outra razao.
        is_ahem: bool,
        bold: bool,
        /// `font-style: italic`/`oblique`. Um bit à parte do `bold` e não um
        /// "peso" — no browser são dois eixos independentes (`<em><strong>` é
        /// bold-italic), e colapsá-los num só perderia essa combinação.
        italic: bool,
        letter_spacing: f32,
        decoration: u8,
    },
    /// Começa a RECORTAR a um retângulo (scroll container interno): os itens
    /// seguintes, até o `EndClip`, só pintam DENTRO deste rect E são transladados por
    /// `(offset_x, offset_y)` (o quanto a região rolou). O backend aplica o clip
    /// (egui: `painter.with_clip_rect`) e soma o offset. `node` liga ao `ScrollRegion`
    /// (o backend injeta o offset aqui antes de pintar). Empilha — pode aninhar.
    ///
    /// What it clips is what lies between it and its `EndClip` in the piece
    /// sequence (`pecas.rs`), reused subtrees included. It used to carry how
    /// many subtrees existed when it opened, because a subtree entered the list
    /// by an index that inserting this marker shifted; with one sequence there
    /// is no index to shift.
    BeginClip {
        rect: Rect,
        node: NodeIdx,
        offset_x: f32,
        offset_y: f32,
    },
    /// Abre uma matriz `transform` EXATA para os itens seguintes, até o
    /// `PopTransform` correspondente — rotação/skew/matrix pintados como o
    /// quadrilátero real, não a bounding box axis-aligned que
    /// `itens::apply_transform_to_item` calcula para `node_rects`.
    ///
    /// Duas verdades convivem de propósito: `node_rects` (o que
    /// `getBoundingClientRect` devolve) SEMPRE guarda a bbox — é o contrato
    /// que `layout/tests/transform_corpus.rs` mede e este lote não mexe — e
    /// os ITENS de pintura, a partir daqui, guardam a matriz em vez de
    /// mutarem `rect`/`size` pela aproximação de norma-de-coluna. Quem pinta
    /// (`claude-raster.rs`, `rts-egui`) aplica a matriz aos 4 cantos do
    /// `rect` original e preenche o quadrilátero — exato para
    /// translate/scale/rotate/skew/matrix, os cinco casos do CSS.
    PushTransform { mat: super::transformacao::Mat2d },
    /// Fecha a matriz aberta pelo `PushTransform` correspondente.
    PopTransform,
    /// Fecha o clip aberto pelo `BeginClip` correspondente.
    ///
    /// No count of subtrees any more. When a subtree entered by an item index,
    /// several could share the index of this marker — those already there were
    /// inside the clip, those a later sibling added were not — and the count was
    /// how a walk told them apart; without it a whole Wikipedia page went blank,
    /// 30 325 of 30 528 items clipped to a MediaWiki `width:1px;height:1px;
    /// overflow:hidden` rule. A later sibling's subtree is now simply AFTER this
    /// marker in the sequence.
    EndClip,
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
#[derive(Clone, Default, Debug)]
pub struct DisplayList {
    /// The box tree this list was laid out against, memoised on the `Dom`
    /// (`Dom::box_tree`).
    ///
    /// This is what the flow sequence, the fragment cache's address and the
    /// public geometry read the boxes from — the fix for invariant I1 in
    /// `docs/ui/html-engine/box-tree.md` §7. `Rc` and not a borrow:
    /// this list can outlive the layout pass that built it (it is cached),
    /// and cloning the `Rc` guarantees it is always read against the SAME
    /// tree it was built against, never half of one tree and half of
    /// another because the memo on the `Dom` moved on in between.
    ///
    /// `BoxTree` derives `Default`, so `Rc<BoxTree>` does too (an empty tree):
    /// the five `DisplayList::default()` call sites compile unchanged.
    pub tree: std::rc::Rc<BoxTree>,
    /// The output in paint order: own items, subtrees reused by REFERENCE, and
    /// the geometry marks the hit order is read from — one sequence
    /// (`pecas.rs` says what it replaced and why).
    ///
    /// The subtrees are what make the output a TREE: a frame that touches one
    /// leaf does not rebuild the page's 30 000 items, it points at the fragments
    /// that already existed. Whoever paints walks it ([`Self::walk`]); whoever
    /// must mutate or compare flattens it ([`Self::materialize`]).
    pub pieces: Vec<Piece>,
    /// A cor do CANVAS — o fundo do `<body>`/`<html>` propagado, e BRANCO
    /// quando nenhum dos dois define um. Vive aqui e não como item da lista
    /// porque é a cor de LIMPEZA do backend; sem ela o que aparecia por trás de
    /// uma página era a cor padrão dele (quase preta), e uma página real cujo
    /// estilo mora num `<link>` externo saía texto preto sobre preto.
    pub canvas_background: u32,
    /// Altura total ocupada pelo conteúdo (para o backend dimensionar o scroll).
    pub content_height: f32,
    /// Geometria por CAIXA (border-box, em coordenadas de conteúdo) — a base
    /// de `element.getBoundingClientRect()`/`offsetWidth`/etc. Preenchido
    /// durante o layout: cada bloco regista o retângulo da sua caixa (margin
    /// EXCLUÍDA — border-box, como o `getBoundingClientRect` do browser);
    /// nós de texto não entram.
    ///
    /// Chaveado por `BoxId` e não por `NodeIdx` — a mudança central do lote
    /// BT-1. Enquanto a árvore for o espelho (uma caixa por elemento) isto é
    /// indistinguível da forma antiga: é o que faz este lote não mudar
    /// resposta nenhuma. `rect_of_node` e `geometry_now` são as vistas
    /// agregadas por nó, para quando um nó vier a ter mais do que uma caixa.
    pub box_rects: super::BoxRects,
    pub ancoras_estaticas: Vec<(BoxId, f32, f32)>, // static positions, `ancora_estatica.rs`
    /// Tracks de coluna de grids explícitos, já resolvidas em px pelo layout. O
    /// `computedProperty` usa esta fonte de used values sem duplicar `resolve_tracks`.
    pub grid_column_tracks: crate::fasthash::FastMap<NodeIdx, Vec<f32>>,
    /// Containers roláveis internos (divs com `overflow`) — o backend gerencia o
    /// offset de cada região e recorta. Vazio quando a página não tem scroll interno.
    pub scroll_regions: Vec<ScrollRegion>,
    /// A geometria completa, montada sob demanda a partir da árvore. Não entra
    /// no `PartialEq` nem no `Clone` lógico: é derivada.
    geometry_cache: std::cell::RefCell<Option<std::rc::Rc<Geometry>>>,
}

/// Equal when everything a repaint or a hit-test could observe is equal.
///
/// `tree` and `geometry_cache` are excluded on purpose — both, the comment on
/// `geometry_cache` already said before `tree` existed, are DERIVED from the
/// same document that produced `box_rects` and the pieces. Comparing `tree`
/// would add nothing and would force `BoxTree`/`LayoutBox` to carry `PartialEq`
/// for no other reason.
impl PartialEq for DisplayList {
    fn eq(&self, other: &Self) -> bool {
        self.pieces == other.pieces
            && self.canvas_background == other.canvas_background
            && self.content_height == other.content_height
            && self.box_rects == other.box_rects
            && self.grid_column_tracks == other.grid_column_tracks
            && self.scroll_regions == other.scroll_regions
    }
}

/// A geometria de uma passada de layout, já com as subárvores reusadas somadas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub rects: crate::fasthash::FastMap<NodeIdx, Rect>,
    pub hit_order: Vec<(NodeIdx, Rect)>, // each box's own rect: `pecas::collect`
    pub scroll_regions: Vec<ScrollRegion>,
}

impl DisplayList {
    /// An empty list that already carries the document box tree.
    ///
    /// **Use this and not `default()` for any list layout writes into.** The
    /// translation from node to box happens through `tree`, so a list built
    /// with `default()` has an EMPTY tree, `boxes_of` answers nothing, and
    /// every rectangle written into it is silently dropped. That is not a
    /// hypothetical: it is what 322 tests failed with before this existed.
    ///
    /// `default()` stays for the callers that never receive geometry — a probe,
    /// a test that only reads items.
    pub fn for_dom(dom: &crate::dom::Dom) -> Self {
        DisplayList {
            tree: dom.box_tree(),
            ..Default::default()
        }
    }

    /// Paints `item` over everything emitted so far — the one write most of
    /// layout does. An item that must go BEHIND what is already there is an
    /// insert into `pieces` at a position remembered before it (`pecas.rs`).
    pub fn push_item(&mut self, item: DisplayItem) {
        self.pieces.push(Piece::Item(item));
    }

    /// Todos os itens a pintar, em z-order, cada um com o deslocamento a somar.
    ///
    /// Anda a ÁRVORE de fragmentos: um item de uma subárvore reusada sai daqui
    /// sem nunca ter sido copiado. Quem pinta já somava uma origem, então somar
    /// mais um deslocamento é grátis — foi o que permitiu a saída deixar de ser
    /// uma lista plana refeita por frame.
    pub fn walk(&self, mut f: impl FnMut(&DisplayItem, f32, f32)) {
        super::pecas::walk(&self.pieces, 0.0, 0.0, &mut f);
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

    /// Achata esta lista em itens próprios, esquecendo a árvore — and the
    /// geometry of the subtrees it reused, as it always did.
    pub fn materialize(&mut self) {
        super::pecas::flatten_from(&mut self.pieces, 0);
    }

    /// Quantos itens esta lista pinta ao todo.
    pub fn total_items(&self) -> usize {
        super::pecas::count_items(&self.pieces)
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
        // Agrega `box_rects` (por CAIXA) em `rects` (por NÓ) — o limite de
        // agregação que o §6 do desenho da árvore de caixas pede. Enquanto a
        // árvore for o espelho, cada nó tem no máximo uma caixa e isto é uma
        // cópia; deixa de ser quando um nó vier a ter várias.
        let mut rects: crate::fasthash::FastMap<NodeIdx, Rect> = crate::fasthash::FastMap::default();
        for (box_id, rect) in self.box_rects.unions() {
            if let Some(node) = self.tree.node_of(box_id) {
                match rects.get_mut(&node) {
                    Some(existing) => *existing = existing.union(rect),
                    None => {
                        rects.insert(node, rect);
                    }
                }
            }
        }
        let mut g = Geometry {
            rects,
            hit_order: Vec::new(),
            scroll_regions: self.scroll_regions.clone(),
        };
        // The hit order is the geometry marks in paint order, a reused
        // subtree's entering where its `Child` stands — `pecas::collect`.
        super::pecas::collect(&self.tree, &self.pieces, 0.0, 0.0, &|b| self.box_rects.union(b), &mut g);
        g
    }

    /// `getBoundingClientRect` — NÃO é só `Geometry::rects`; ver `rect_cliente.rs`.
    pub fn rect_of(&self, node: NodeIdx) -> Option<Rect> {
        super::rect_cliente::rect_cliente(self, node)
    }

    /// Retângulo de uma caixa concreta da BoxTree, inclusive quando ela foi
    /// emitida dentro de um fragmento reutilizado. Esta é a consulta interna
    /// para código de layout; a API de DOM continua a usar [`Self::rect_of`],
    /// pois caixas anônimas não possuem `NodeIdx` para expor.
    pub(crate) fn rect_of_box(&self, box_id: BoxId) -> Option<Rect> {
        self.box_rects
            .union(box_id)
            .or_else(|| super::pecas::rect_in_children(&self.pieces, box_id, 0.0, 0.0))
    }

    /// O retângulo de um NÓ: a união dos retângulos das caixas que ele
    /// gerou. Par de `rect_of`, sem passar por `Geometry` nem pelo cache —
    /// para um chamador que já tem um `NodeIdx` isolado e não quer montar a
    /// geometria completa da lista.
    ///
    /// Enquanto a árvore for o espelho (BT-1 fase 1), `boxes_of` devolve no
    /// máximo uma caixa, então isto é exatamente o retângulo dela, ou `None`
    /// para um nó que não gerou caixa nenhuma (texto, `display:none`) ou
    /// ainda não foi layoutado.
    pub fn rect_of_node(&self, node: NodeIdx) -> Option<Rect> {
        let mut acc: Option<Rect> = None;
        for &box_id in self.tree.boxes_of(node) {
            if let Some(rect) = self.rect_of_box(box_id) {
                acc = Some(match acc {
                    Some(a) => a.union(rect),
                    None => rect,
                });
            }
        }
        acc
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
            let hit = |r: &Rect| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
            return g.hit_order.iter().rev().find(|(_, r)| hit(r)).map(|&(idx, _)| idx);
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
