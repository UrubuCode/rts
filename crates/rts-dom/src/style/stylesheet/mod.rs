//! O STYLESHEET de autor (`<style>`/`addStylesheet`) e a cascade: regras
//! ordenadas por (especificidade, ordem do fonte), camadas normal/`!important`
//! ([`DeclBlock`]) e a extração de `@keyframes`.
//!
//! ## Fidelidade à cascade CSS da MDN
//!
//! O modelo segue os estágios da cascade
//! (<https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Cascade>):
//! 1. **Origem/importância:** normais UA(`defineStyle`) < `<style>` autor <
//!    `style=""` inline < override-por-nó; depois os `!important` por cima (autor <
//!    inline) — `!important` inverte a precedência de origem. Em `Dom::computed_style`.
//! 2. **Especificidade:** id(100) > classe(10) > tag(1) > universal(0) — em
//!    [`Selector::specificity`]; a regra mais específica sobrepõe.
//! 3. **Ordem do fonte:** empate de especificidade → a regra DECLARADA DEPOIS
//!    vence (campo `order`, desempate em [`computed_for`](Stylesheet::computed_for)).
//! 4. **Herança:** color/font-size descem do pai no render (`inherit_from` gerado
//!    pela tabela de propriedades); propriedade não-tocada fica `None`.
//!
//! **Seletores (#1752 — implementado):** compostos (`.a.b`, `p.card#x`),
//! combinadores (`div p`, `>`, `+`, `~`), atributo (`[a]`/`[a=v]`/`^=`/`$=`/`*=`/
//! `~=`/`|=`), pseudo estruturais (`:first-child`/`:last-child`/`:only-child`/
//! `:empty`/`:root`/`:nth-child`) e de estado-via-atributo (`:checked`/`:disabled`/
//! `:enabled`/`:required`), e lista por vírgula em querySelector/matches/closest.
//! **Cortes (não bugs):** pseudo de estado VIVO (`:hover`/`:focus`); pseudo-elementos
//! (`::before`); flag de case `[a=v i]`. `@layer`, `inherit`, `initial`, `unset`,
//! `revert`, `revert-layer` (lote J — `stylesheet::revert`) e `!important` já
//! atravessam o pipeline actual.

pub(in crate::style::stylesheet) use super::parse::parse_inline_block;
pub(in crate::style::stylesheet) use super::props::ComputedStyle;
pub(in crate::style::stylesheet) use super::selector::{ComplexSelector, PseudoClass, Selector, compound_matches};

// `MediaQuery`/`MediaContext` moveram-se para `stylesheet::media` (lote P,
// §5.P) — a gramática completa (`not`/`only`/listas, orientation,
// aspect-ratio, resolution, hover/pointer, prefers-*) não cabia mais como um
// par de campos opcionais aqui. Reexportados por `pub use media::*` no fim
// deste ficheiro.

/// Uma regra do stylesheet: um seletor + as declarações já parseadas (separadas
/// nas camadas normal/important da cascade). A ordem de declaração no fonte
/// (`order`) desempata especificidades iguais.
#[derive(Clone, PartialEq, Debug)]
pub struct Rule {
    pub selector: Selector,
    /// Declarações CSS no estado especificado, preservadas pelo AST antes do
    /// lowering para `ComputedStyle`. A mesma instância é partilhada pelos
    /// selectors que vierem de uma regra com lista de selectors.
    pub specified: std::rc::Rc<crate::style::syntax::SpecifiedStyle>,
    /// Alias de compatibilidade para consumidores que ainda precisam da fatia
    /// directa de declarações especificadas.
    pub source_declarations: std::rc::Rc<[crate::style::syntax::DeclarationAst]>,
    /// Ordem da cascade layer. `None` significa regra autoral não agrupada;
    /// para declarações normais ela fica acima de todas as layers nomeadas.
    pub layer: Option<u32>,
    /// As declarações, COMPARTILHADAS: `Rc` e não valor.
    ///
    /// Um `DeclBlock` tem 2120 bytes (dois `ComputedStyle` inteiros, medido por
    /// `metrics::footprint::type_sizes`), e `a, b, .c { … }` vira uma regra POR
    /// SELETOR — que clonava os 2 KB a cada uma. Com o `Rc`, os seletores de uma
    /// mesma regra dividem um bloco, o `Vec<Rule>` realoca movendo 8 bytes em
    /// vez de 2120, e o retorno recursivo do parse de `@media` para de carregar
    /// megabytes. Medido: a emissão de regras era 29% do `parse-css`, que era
    /// 86% do custo de ABRIR uma página Bootstrap.
    pub decls: std::rc::Rc<RuleDecls>,
    /// Posição da regra no fonte (0-based) — desempate da cascade.
    pub order: u32,
    /// A condição `@media` que ENVOLVE a regra (None = sempre aplica). Avaliada
    /// contra o viewport na cascade ([`Stylesheet::computed_for_node`]).
    pub media: Option<MediaQuery>,
    /// O `content` declarado, quando esta regra tem pseudo-elemento.
    ///
    /// Fora do `decls` porque `content` não é uma propriedade do
    /// `ComputedStyle` — ver [`crate::pseudo::Content`]. `Rc` pela mesma razão
    /// que as declarações o são: `a::before, b::before { content:"x" }` é uma
    /// regra por seletor e as duas partilham o valor.
    pub content: Option<std::rc::Rc<crate::pseudo::Content>>,
    /// `counter-reset`/`counter-increment` declarados, quando os há.
    ///
    /// Fora do `decls` pela razão do `content` — não são propriedades do
    /// `ComputedStyle` — mas ao contrário dele são lidas de QUALQUER regra e não
    /// só das de pseudo-elemento: na folha da Wikipédia o `counter-increment`
    /// que numera os retrolinks está num `::before`, e o que numera as
    /// referências está num `<li>` comum.
    pub counters: Option<std::rc::Rc<crate::counters::Ops>>,
    /// `true` para uma regra da folha de UA (lote I, `style::ua`), `false` para
    /// uma regra de autor. É a chave MAIS FORTE da ordenação da cascade — mais
    /// forte que layer — porque a UA é a origem mais fraca em declarações
    /// normais e a mais forte em `!important` (CSS Cascade 5 §6.1): nenhuma
    /// combinação de layer/especificidade de autor deve poder vencer a UA em
    /// normal, nem perder para o autor em `!important`.
    pub is_ua: bool,
}

/// Um bloco de declarações separado nas DUAS camadas de importância da cascade
/// (MDN estágio 1): `normal` e `important`. Na cascade os `normal` de todas as
/// regras são aplicados primeiro (por origem<especificidade<ordem); depois os
/// `important`, na mesma ordem — então `!important` SEMPRE vence o normal, mas
/// entre dois `important` a especificidade/ordem ainda desempata. Egui-free.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct DeclBlock {
    /// Declarações normais (sem `!important`).
    pub normal: ComputedStyle,
    /// `all: initial` normal resetou as declarações anteriores deste bloco.
    pub all_initial_normal: bool,
    /// Declarações marcadas `!important` (vencem qualquer normal na cascade).
    pub important: ComputedStyle,
    /// `all: initial !important` resetou a camada importante deste bloco.
    pub all_initial_important: bool,
    /// Declarações normais de CUSTOM PROPERTIES normais (`--nome: valor`) do bloco, na
    /// ordem da fonte, com o valor cru. Participam da cascade por elemento.
    pub custom: Vec<(String, String)>,
    /// Custom properties marcadas `!important`, aplicadas depois das normais e
    /// antes de resolver os valores pendentes com `var()`.
    pub custom_important: Vec<(String, String)>,
    /// Declarações PENDENTES — o valor contém `var()` e só resolve POR ELEMENTO
    /// (contra as custom props computadas dele): `(prop, valor-cru, important)`.
    /// Resolvidas na posição da regra em [`Stylesheet::computed_for_node`].
    pub pending: Vec<(String, String, bool)>,
}

/// O que uma REGRA guarda: o mesmo conteúdo de um [`DeclBlock`], mas com as
/// duas camadas em LISTA ESPARSA.
///
/// Um `ComputedStyle` tem 1000 bytes e uma regra CSS declara 2,1 propriedades em
/// média — guardar duas structs inteiras por regra fazia o stylesheet do
/// Bootstrap ocupar 5,9 MiB, e o parse zerar 2 KB por regra. O `DeclBlock` segue
/// existindo para o `style=""` INLINE, onde a struct é o formato natural (é
/// lida uma vez, por elemento, e a cascade a consome direto).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct RuleDecls {
    pub normal: Box<[super::props::Decl]>,
    pub important: Box<[super::props::Decl]>,
    pub all_initial_normal: bool,
    pub all_initial_important: bool,
    pub custom: Vec<(String, String)>,
    pub custom_important: Vec<(String, String)>,
    pub pending: Vec<(String, String, bool)>,
}

impl RuleDecls {
    /// Converte o bloco recém-parseado na forma esparsa. A conversão acontece
    /// UMA vez, no parse; a cascade só aplica.
    pub fn from_block(block: DeclBlock) -> RuleDecls {
        RuleDecls {
            normal: block.normal.to_decls().into_boxed_slice(),
            important: block.important.to_decls().into_boxed_slice(),
            all_initial_normal: block.all_initial_normal,
            all_initial_important: block.all_initial_important,
            custom: block.custom,
            custom_important: block.custom_important,
            pending: block.pending,
        }
    }

    /// Aplica as declarações normais sobre um estilo (mesma precedência do
    /// `merge_over`: quem aplica depois vence).
    pub fn apply_normal(&self, target: &mut ComputedStyle) {
        if self.all_initial_normal {
            *target = ComputedStyle::default();
        }
        for d in &self.normal {
            d.apply(target);
        }
    }

    pub fn apply_important(&self, target: &mut ComputedStyle) {
        if self.all_initial_important {
            *target = ComputedStyle::default();
        }
        for d in &self.important {
            d.apply(target);
        }
    }

    /// Bytes ESTIMADOS deste bloco (para `metrics::footprint`).
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + (self.normal.len() + self.important.len()) * std::mem::size_of::<super::props::Decl>()
            + self
                .custom
                .iter()
                .chain(self.custom_important.iter())
                .map(|(k, v)| k.capacity() + v.capacity())
                .sum::<usize>()
            + self
                .pending
                .iter()
                .map(|(k, v, _)| k.capacity() + v.capacity())
                .sum::<usize>()
    }
}

impl DeclBlock {
    /// `true` se nenhuma das camadas tem qualquer propriedade setada.
    pub fn is_empty(&self) -> bool {
        self.normal == ComputedStyle::default()
            && self.important == ComputedStyle::default()
            && !self.all_initial_normal
            && !self.all_initial_important
            && self.custom.is_empty()
            && self.custom_important.is_empty()
            && self.pending.is_empty()
    }
}

/// `size_of::<Rule>()` — exposto porque `Rule` é privado do módulo e o número
/// dele é o que explica a pegada de um stylesheet grande (cada regra carrega um
/// `DeclBlock`, que carrega dois `ComputedStyle` inteiros).
pub fn rule_size() -> usize {
    std::mem::size_of::<Rule>()
}

/// Um stylesheet de autor (o conteúdo de um `<style>`), já parseado em regras
/// ordenadas. Egui-free como o resto. É anexado ao `Dom` e consultado na cascade
/// de `computed_style`.
// `Default` NÃO é derivado de propósito: a derivação constrói `rules: vec![]`
// direto, sem passar por `Stylesheet::new()` — e `new()` é o único lugar que
// instala a folha de UA (lote I). Um `Default::default()` derivado seria um
// stylesheet sem UA, silenciosamente, na primeira vez que alguém o chamasse em
// vez de `new()`. `impl Default` abaixo delega em `new()` para fechar essa porta.
#[derive(Clone, Debug)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// AST sintáctico dos blocos CSS anexados, preservado para diagnósticos,
    /// tooling e lowering futuro. A cascade continua a consumir `rules`.
    pub syntax: Vec<crate::style::syntax::StylesheetAst>,
    /// Os `@keyframes nome {...}` da página (#1776), por nome. Consultados pelo
    /// `advance` quando um nó tem `animation: nome ...`.
    pub keyframes: std::collections::HashMap<String, crate::anim::Keyframes>,
    /// Índice de regras por chave-alvo (id/classe/tag) — reduz a cascade de "testar
    /// TODAS as regras por nó" para "testar só as candidatas". Construído lazy sob
    /// `&self` (por isso `RefCell`); reconstruído quando `rules` muda (o `covered`
    /// do índice detecta). Estado DERIVADO — fora do `PartialEq` (que compara só
    /// `rules`).
    index: std::cell::RefCell<super::ruleindex::RuleIndex>,
    /// Buffer derivado reutilizado por uma cascade por vez; evita criar um Vec de
    /// índices candidatos para cada elemento. O stylesheet já usa RefCell para o
    /// índice lazy e a cascade é chamada de forma síncrona pelo DOM.
    candidate_scratch: std::cell::RefCell<Vec<usize>>,
    /// Cache de [`hover_reach`](Stylesheet::hover_reach) — a resposta é uma
    /// varredura de todas as regras, e a pergunta é feita a cada movimento do
    /// mouse. Invalidado junto com o índice, em `append_css`.
    hover_reach: std::cell::RefCell<Option<HoverReach>>,
    /// Cache de [`position_sensitive`](Stylesheet::position_sensitive), pelo
    /// mesmo motivo do `hover_reach` — a pergunta é por mutação de árvore.
    position_sensitive: std::cell::RefCell<Option<bool>>,
    /// Cache de [`has_out_of_flow`](Stylesheet::has_out_of_flow).
    out_of_flow: std::cell::RefCell<Option<bool>>,
    /// Ordem global das cascade layers deste stylesheet. É persistida entre
    /// chamadas a `append_css`, porque folhas anexadas são uma única origem
    /// autoral para fins de cascade.
    pub(crate) layer_names: std::cell::RefCell<Vec<String>>,
    /// As `@property` registadas (lote P, §5.P item 4) — estrutural como
    /// `keyframes`, e pela mesma razão fica fora do `PartialEq`: dois
    /// stylesheets com as mesmas `rules` são o mesmo estilo mesmo que um tenha
    /// registado uma `@property` que a outra folha nunca leu.
    pub(crate) properties: property::CustomPropertyRegistry,
}

/// As regras que casaram um nó, ordenadas pela cascade. Opaco de propósito: o
/// que quem chama precisa é passá-lo aos dois passes, não ler o conteúdo.
#[derive(Debug, Default)]
pub struct MatchedRules {
    /// `(origem, prioridade-layer, especificidade, ordem, índice)`, crescente.
    /// `origem` é 0=UA/1=autor em declarações normais (invertido para
    /// `!important` no ponto de ordenação, não aqui — o valor guardado é
    /// sempre 0/1 e quem ordena decide o sentido). `prioridade-layer` usa
    /// `u32::MAX` para regras sem layer.
    pub(crate) rules: Vec<(u32, u32, u32, u32, usize)>,
}

impl MatchedRules {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Até onde uma mudança de `:hover` pode mexer no estilo desta folha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HoverReach {
    /// Nenhuma regra usa `:hover` — mover o mouse não muda estilo nenhum.
    None,
    /// Só o elemento que casa (`.btn:hover`). A subárvore dele ainda entra, por
    /// causa das propriedades HERDADAS (`color` num `:hover` desce aos filhos).
    SelfOnly,
    /// `.card:hover .title` — desce pela subárvore do que casa.
    Subtree,
    /// `.a:hover + .b` — sai da subárvore. A invalidação por subárvore não
    /// cobre isto, então este caso cai no fallback global, declarado.
    Siblings,
}

// PartialEq manual (Keyframes tem f32, não derivamos Eq; o diff de árvore só compara
// nodes+root, não o Stylesheet, então isto é só p/ testes).
impl PartialEq for Stylesheet {
    fn eq(&self, other: &Self) -> bool {
        self.rules == other.rules
    }
}

impl Stylesheet {
    /// Blocos sintácticos CSS preservados na ordem em que foram anexados.
    pub fn syntax(&self) -> &[crate::style::syntax::StylesheetAst] {
        &self.syntax
    }

    /// Diagnósticos de parsing dos blocos CSS anexados, na ordem da fonte.
    pub fn diagnostics(&self) -> Vec<crate::style::syntax::Diagnostic> {
        self.syntax
            .iter()
            .flat_map(|ast| ast.diagnostics.iter().cloned())
            .collect()
    }
}

// Os corpos movidos dizem `super::props::…`, `super::Combinator` e mais oito, como
// o ficheiro único dizia — mas ali `super` era `style` e aqui é `stylesheet`. Os
// dez nomes foram MEDIDOS (as duas formas: `super::X` e `super::{A, B}`), e
// reimportá-los no PAI mantém-nos a resolver sem tocar no que se moveu.
use super::{Combinator, CompoundSelector, Position, PseudoElement, SimpleSelector};
use super::{props, ruleindex, selector, vars};

mod sheet;
mod rules;
mod revert;
mod supports;
mod media;
mod property;

pub use rules::*;
pub use media::{MediaContext, MediaQuery, PrefersColorScheme};
pub use property::{CustomPropertyRegistry, PropertySyntax, RegisteredProperty};
