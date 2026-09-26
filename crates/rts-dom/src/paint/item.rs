//! A LISTA DE DESENHO, the items: `Corners` and `DisplayItem` — one atomic,
//! already positioned paint instruction each.
//!
//! Moved from `layout/display.rs` on 2026-09-25 (PQ-A2); nothing in it changed.

use crate::dom::NodeIdx;
use crate::paint::list::Rect;
use crate::style::ComputedStyle;

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
    /// sequence (`pieces.rs`), reused subtrees included. It used to carry how
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
    PushTransform { mat: crate::paint::transform::Mat2d },
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

/// DESLOCA um item de pintura por `(dx, dy)`.
///
/// É a operação que torna um fragmento de layout REUSÁVEL: o desenho de uma
/// subárvore cujo conteúdo e constraints não mudaram é o mesmo desenho, na
/// posição nova. Tudo o que um item carrega é geometria absoluta em coordenadas
/// de conteúdo, então deslocar é somar — exceto o que é tamanho (`radius`,
/// `blur`, `size` do texto), que não se move.
///
/// Moved from `layout/fragment/items.rs` in PQ-C1 (F1): a pure item utility,
/// paint's own; nothing in it changed.
pub(crate) fn translate_item(it: &mut DisplayItem, dx: f32, dy: f32) {
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
        DisplayItem::Quad { pts, .. } => {
            for p in pts.iter_mut() {
                p.0 += dx;
                p.1 += dy;
            }
        }
        // A matriz descreve pontos em coordenadas de CONTEÚDO já absolutas —
        // deslocar a subárvore por (dx,dy) é compor uma translação PURA
        // depois dela: `nova(p) = mat(p) + (dx,dy)`, que em `e`/`f` é somar
        // direto (a parte linear a/b/c/d não muda por uma translação).
        DisplayItem::PushTransform { mat } => {
            mat.e += dx;
            mat.f += dy;
        }
        DisplayItem::EndClip | DisplayItem::PopTransform => {}
    }
}
