//! A DIMENSÃO e a sua resolução tardia: `ResolveCtx`, `CalcLen`, `Dimension`
//!
//! Extraído de `values.rs` sem alterar uma linha.

/// O contexto de resolução de uma [`Dimension`] relativa, conhecido só no
/// render. Cada unidade resolve contra um eixo diferente (north-star risco 5: a
/// resolução de `%`/`em`/`vw`/… é TARDIA, no layout, não no parse). Egui-free.
#[derive(Clone, Copy, Debug)]
pub struct ResolveCtx {
    /// Largura do content-box do PAI (containing block) — base de `%` e `vw` (este
    /// usa a largura da viewport, passada aqui como `viewport_w`).
    pub parent_content_w: f32,
    /// `font-size` COMPUTADO deste nó — base de `em`.
    pub node_font_size: f32,
    /// `font-size` da RAIZ (`:root`/`html`) — base de `rem`.
    pub root_font_size: f32,
    /// Largura da viewport (janela) em pontos — base de `vw`.
    pub viewport_w: f32,
    /// Altura da viewport (janela) em pontos — base de `vh`.
    pub viewport_h: f32,
}

/// `true` quando o interruptor de medição repõe o comportamento antigo. Lido
/// uma vez: isto corre por caixa medida.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::style::values) enum ModoPct {
    /// A regra inteira: nem `width` nem padding/margem em percentagem contam.
    Completa,
    /// Só o `width`. O padding/margem em percentagem resolvem como antes.
    SoWidth,
    /// Nada: o comportamento anterior à mudança.
    Desligada,
}

pub(in crate::style::values) fn modo_pct() -> ModoPct {
    static MODO: std::sync::OnceLock<ModoPct> = std::sync::OnceLock::new();
    *MODO.get_or_init(|| match std::env::var("RTS_PCT_INTRINSECO").as_deref() {
        Ok("1") => ModoPct::Desligada,
        Ok("width") => ModoPct::SoWidth,
        _ => ModoPct::Completa,
    })
}

pub(in crate::style::values) fn pct_intrinseco_ligado() -> bool {
    modo_pct() == ModoPct::Desligada
}

/// Um comprimento que conta para uma medição INTRÍNSECA — ou seja, um que não
/// depende de uma largura que ainda está por decidir.
///
/// Uma percentagem (e um `calc()` com componente percentual) responde `None`:
/// é contra o containing block, e o tamanho intrínseco é justamente o que se
/// está a usar para o decidir. Devolver um número ali é circular, e o número que
/// sairia seria contra a VIEWPORT — 50% de 1280 dentro de uma caixa de 220.
///
/// Foi medido duas vezes com o mesmo desenlace: uma célula de tabela a exigir
/// 1280px de largura mínima, e um item flex a ocupar a linha inteira e a
/// empurrar o irmão para a linha seguinte.
pub fn dimensao_absoluta(d: Dimension, ctx: &ResolveCtx) -> Option<f32> {
    // INTERRUPTOR DE MEDIÇÃO: `RTS_PCT_INTRINSECO=1` repõe o comportamento
    // anterior (a percentagem resolvida contra a viewport) e `=width` mede a
    // variante que aplica a regra só ao `width`.
    //
    // Existe para esta regra poder ser medida ISOLADAMENTE sobre a página real,
    // ligada e desligada na MESMA árvore. Sem ele, atribuí-la exigiria comparar
    // duas medições separadas por outras mudanças — que é como um número deixa
    // de dizer o que se pensa que diz, e foi o que aconteceu quando ela entrou.
    //
    // Fica enquanto o eixo VERTICAL estiver em aberto: a medição mostrou que
    // esta regra melhora o horizontal e piora o vertical por acumulação, e quem
    // atacar a altura vai querer voltar a separar as duas coisas. Sai quando
    // isso estiver fechado. A leitura do ambiente é uma vez só.
    if pct_intrinseco_ligado() {
        return d.resolve(ctx);
    }
    match d {
        Dimension::Percent(_) => None,
        Dimension::Calc(c) if c.pct != 0.0 => None,
        outra => outra.resolve(ctx),
    }
}

/// Uma expressão `calc()` LINEAR já reduzida à combinação das 6 bases de
/// comprimento: `px + pct·CB + em·font + rem·root + vw·VW + vh·VH`. Qualquer
/// calc de soma/subtração/multiplicação-por-escalar reduz a esta forma no PARSE
/// (simbolicamente), e a resolução continua TARDIA como toda [`Dimension`] — é o
/// que faz `calc(1.375rem + 1.5vw)` (a tipografia fluida do Bootstrap) funcionar.
/// `Copy` de propósito (a `Dimension` viaja por valor pelo layout inteiro).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CalcLen {
    pub px: f32,
    /// coeficiente de `%` (resolve contra o containing block, como `Percent`).
    pub pct: f32,
    pub em: f32,
    pub rem: f32,
    pub vw: f32,
    pub vh: f32,
}

impl CalcLen {
    /// Soma termo a termo (o `+`/`-` do calc; para `-`, chame com `rhs.scale(-1.0)`).
    pub fn add(self, rhs: CalcLen) -> CalcLen {
        CalcLen {
            px: self.px + rhs.px,
            pct: self.pct + rhs.pct,
            em: self.em + rhs.em,
            rem: self.rem + rhs.rem,
            vw: self.vw + rhs.vw,
            vh: self.vh + rhs.vh,
        }
    }
    /// Multiplica por um escalar (o `*`/`/` do calc — a spec só permite escalar).
    pub fn scale(self, k: f32) -> CalcLen {
        CalcLen {
            px: self.px * k,
            pct: self.pct * k,
            em: self.em * k,
            rem: self.rem * k,
            vw: self.vw * k,
            vh: self.vh * k,
        }
    }
}

/// Uma dimensão de caixa que SOBREVIVE a unidade relativa até o layout (north-star
/// risco 5): só `Px`/`Auto` resolvem de imediato; `Percent`/`Em`/`Rem`/`Vw`/`Vh`
/// (e o [`Calc`](Dimension::Calc) que os combina) precisam de um eixo conhecido só
/// no render (pai/fonte/viewport), então o tipo PRESERVA a forma e
/// [`resolve`](Dimension::resolve) calcula tarde.
/// Egui-free (tipo próprio, não `Vec2`/`f32`), como o resto do `style`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dimension {
    /// `auto` — o layout decide (o egui usa a largura disponível).
    Auto,
    /// Valor absoluto em pontos/px (≥ 0).
    Px(f32),
    /// `%` do containing block (0..=100): `pai_content_w * p/100`.
    Percent(f32),
    /// `em` — múltiplo do `font-size` DESTE nó.
    Em(f32),
    /// `rem` — múltiplo do `font-size` da RAIZ.
    Rem(f32),
    /// `vw` — `%` da largura da viewport (0..=100): `viewport_w * v/100`.
    Vw(f32),
    /// `vh` — `%` da altura da viewport (0..=100): `viewport_h * v/100`.
    Vh(f32),
    /// `ex` — múltiplo da ALTURA DO X da fonte deste nó. Sem métrica de fonte
    /// aqui, é `font-size × X_HEIGHT_RATIO` (0,491, calibrado contra o Chrome
    /// em `text_metrics`): `10ex` a 16px dá 78,6 onde o Blink mede 78,44 com
    /// a Consolas — dentro da tolerância do corpus (`claude-font-unidades-ch-ex`).
    Ex(f32),
    /// `ch` — múltiplo do AVANÇO DO "0" da fonte deste nó. Sem métrica, é
    /// `font-size × MONO_ADVANCE` (0,5498): exacto para a monoespaçada
    /// calibrada (`10ch` = 87,97 = Blink), e uma aproximação dita para as
    /// proporcionais — o "0" da Arial avança 0,556em, da Segoe UI 0,539em, por
    /// isso o mesmo número serve às duas dentro de 1px em `10ch`.
    Ch(f32),
    /// `max-content` — a largura que o CONTEÚDO pede sem quebrar.
    ///
    /// Não é um comprimento: só se resolve com a árvore na mão, e por isso
    /// [`resolve`](Dimension::resolve) responde `None` (como o `auto`) e quem a
    /// calcula é o layout, que já tem `intrinsic_content_width` — a mesma função
    /// que serve o shrink-to-fit do inline-block e do item flex. A variante
    /// existe para o parse deixar de a DESCARTAR: `width:max-content` respondia
    /// `None`, indistinguível de "não declarado", e o elemento tomava a largura
    /// do pai. É o painel do menu da Wikipédia — 198,6px no Chrome, 56,2 aqui.
    ///
    /// `min-content` e `fit-content` NÃO entram, e não são aproximados a esta:
    /// `min-content` é a maior palavra indivisível e `fit-content` precisa das
    /// duas para o seu `min(max(min, disponível), max)`. A máquina que temos
    /// calcula só o máximo. Responder max-content a um `min-content` erraria em
    /// silêncio no sentido oposto ao que o nome promete, que é pior do que a
    /// ausência — continuam descartados no parse, como hoje.
    MaxContent,
    /// `calc(...)` linear reduzido no parse ([`CalcLen`]). Não cruza a ABI de
    /// faixas (`to_abi` → `-1`, corte documentado — o TS não empacota calc).
    Calc(CalcLen),
}

impl Dimension {
    /// Resolve para PONTOS absolutos, dado o contexto do render. `Auto` → `None`
    /// (o layout decide). É chamado TARDE (em `frame/render.rs`), nunca no parse.
    /// Clampa em ≥ 0 (largura/altura negativa não existe); para MARGENS/offsets
    /// (negativo é válido), use [`resolve_signed`](Dimension::resolve_signed).
    pub fn resolve(self, ctx: &ResolveCtx) -> Option<f32> {
        self.resolve_signed(ctx).map(|px| px.max(0.0))
    }

    /// Como [`resolve`](Dimension::resolve), mas SEM o clamp ≥ 0 — para margens
    /// negativas (`.row` gutters do Bootstrap) e offsets de posicionamento.
    pub fn resolve_signed(self, ctx: &ResolveCtx) -> Option<f32> {
        Some(match self {
            // Sem árvore não há conteúdo para medir: quem resolve `max-content` é
            // o layout. Responder `None` aqui é o mesmo que o `auto` faz, e é o
            // que faz um chamador sem contexto cair no seu caminho de fallback em
            // vez de inventar um número.
            Dimension::Auto | Dimension::MaxContent => return None,
            Dimension::Px(v) => v,
            Dimension::Percent(p) => ctx.parent_content_w * p / 100.0,
            Dimension::Em(e) => ctx.node_font_size * e,
            Dimension::Rem(r) => ctx.root_font_size * r,
            Dimension::Vw(v) => ctx.viewport_w * v / 100.0,
            Dimension::Vh(v) => ctx.viewport_h * v / 100.0,
            Dimension::Ex(x) => ctx.node_font_size * crate::style::X_HEIGHT_RATIO * x,
            Dimension::Ch(c) => ctx.node_font_size * crate::style::MONO_ADVANCE * c,
            // calc linear: cada base resolvida no seu eixo e somada.
            Dimension::Calc(c) => {
                c.px + ctx.parent_content_w * c.pct / 100.0
                    + ctx.node_font_size * c.em
                    + ctx.root_font_size * c.rem
                    + ctx.viewport_w * c.vw / 100.0
                    + ctx.viewport_h * c.vh / 100.0
            }
        })
    }

    /// Decodifica a forma ABI `i64` (o TS empacota a dimensão num único inteiro,
    /// slot opaco — invariante 4). Esquema de FAIXAS por unidade (cada unidade tem
    /// uma base; o valor é `× MILLI` para preservar 3 casas decimais sem float na
    /// ABI). `< 0` (inclui `-1`) → `Auto`. O TS aplica a base; o Rust só decodifica
    /// (nunca casa string CSS). Faixas em [`DIM_BASE_PX`] e irmãs.
    pub fn from_abi(v: i64) -> Option<Self> {
        if v < 0 {
            return Some(Dimension::Auto);
        }
        // `unit_of` separa a base (faixa) do valor-em-milésimos.
        let unit = v / DIM_RANGE;
        let milli = (v % DIM_RANGE) as f32 / 1000.0;
        Some(match unit {
            0 => Dimension::Px(milli),
            1 => Dimension::Percent(milli),
            2 => Dimension::Em(milli),
            3 => Dimension::Rem(milli),
            4 => Dimension::Vw(milli),
            5 => Dimension::Vh(milli),
            _ => return None, // unidade desconhecida (TS registrou faixa futura)
        })
    }

    /// Re-codifica para a forma ABI `i64` (inverso de [`from_abi`]), para o
    /// `slot_value`/`nodeStyleSlot` que o layout-TS lê.
    pub fn to_abi(self) -> i64 {
        let (unit, val) = match self {
            Dimension::Auto => return -1,
            Dimension::Px(v) => (0, v),
            Dimension::Percent(p) => (1, p),
            Dimension::Em(e) => (2, e),
            Dimension::Rem(r) => (3, r),
            Dimension::Vw(v) => (4, v),
            Dimension::Vh(v) => (5, v),
            // `ex`/`ch` dependem da fonte e a faixa codifica unidade + número
            // que o TS resolve sem fonte: o mesmo corte do `calc`, `-1`.
            Dimension::Ex(_) | Dimension::Ch(_) => return -1,
            // calc não cabe na codificação de faixas — o TS lê `-1` (corte
            // documentado; calc resolve no layout, não cruza slots).
            Dimension::Calc(_) => return -1,
            // `max-content` também não, e pela mesma razão de fundo: a faixa
            // codifica uma unidade e um número, e isto não é nem uma nem outro.
            // O TS lê `-1` — CORTE, e não o valor `auto`: quem o ler não sabe
            // distinguir os dois, e a alternativa (uma faixa nova) obrigaria o
            // lado TS a saber medir conteúdo, que é o que ele não pode fazer.
            Dimension::MaxContent => return -1,
        };
        unit * DIM_RANGE + (val * 1000.0) as i64
    }
}

/// Tamanho de cada FAIXA de unidade na codificação ABI da [`Dimension`]. O valor
/// dentro da faixa é `pontos × 1000` (3 casas decimais sem float na ABI), então a
/// faixa cobre até 1.000.000 pontos — folgado. `unit = v / DIM_RANGE`,
/// `valor = (v % DIM_RANGE) / 1000`. Bases: 0=px 1=% 2=em 3=rem 4=vw 5=vh.
pub const DIM_RANGE: i64 = 1_000_000_000;
/// Bases de unidade (o TS multiplica por [`DIM_RANGE`] e soma `valor×1000`).
pub const DIM_BASE_PX: i64 = 0;
pub const DIM_BASE_PERCENT: i64 = DIM_RANGE;
pub const DIM_BASE_EM: i64 = 2 * DIM_RANGE;
pub const DIM_BASE_REM: i64 = 3 * DIM_RANGE;
pub const DIM_BASE_VW: i64 = 4 * DIM_RANGE;
pub const DIM_BASE_VH: i64 = 5 * DIM_RANGE;

/// Aplica o clamp de min/max a um valor base resolvido: `clamp(min, base, max)` =
/// `max(min, min(base, max))`. `min`/`max` resolvidos a px (None = sem limite).
pub fn clamp_size(base: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut v = base;
    // max primeiro, min depois (min vence se min > max — regra do CSS).
    if let Some(mx) = max {
        v = v.min(mx);
    }
    if let Some(mn) = min {
        v = v.max(mn);
    }
    v
}
