//! A LISTA DE DESENHO: `Rect`, `Corners`, `DisplayItem`, `ScrollRegion`,
//! `DisplayList`, `Geometry` — o que o layout produz e o backend pinta.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
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
    /// Tracks de coluna de grids explícitos, já resolvidas em px pelo layout. O
    /// `computedProperty` usa esta fonte de used values sem duplicar `resolve_tracks`.
    pub grid_column_tracks: crate::fasthash::FastMap<NodeIdx, Vec<f32>>,
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
    out.hit_order
        .extend_from_slice(&fragment.hit_order[next.min(fragment.hit_order.len())..]);
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
            + self
                .children
                .iter()
                .map(|c| c.fragment.total_items())
                .sum::<usize>()
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
        g.hit_order
            .extend_from_slice(&self.hit_order[next.min(self.hit_order.len())..]);
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
                g.rects
                    .get(&idx)
                    .is_some_and(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
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
