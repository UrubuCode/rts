//! `BlockFormattingContext` — a entidade que faltava para floats, `clear` e o
//! crescimento do pai (CSS 2.1 §9.4.1, §9.5, §10.6.7).
//!
//! A auditoria estrutural de 2026-09-04
//! (`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/04-layout.md`,
//! finding 1 e 2) nomeia exatamente a falta: **"sem contexto de BFC como valor
//! propagado não há como saber, no ponto em que um float é fechado, se o
//! container CORRENTE é o BFC responsável"**. Antes deste módulo,
//! `layout_block`/`layout_children_vertical` recebiam um `&[Exclusao]` — as
//! exclusões HERDADAS de cima, uma cópia local a cada nível — e por isso dois
//! defeitos coexistiam: (a) um float dentro de um `<div>` sem BFC nunca
//! alcançava os IRMÃOS do `<div>` (só os do próprio `<div>`, porque a lista
//! morria no fim da chamada), e (b) o pai crescia para conter os SEUS floats
//! mesmo sem ser o BFC responsável — não havia como perguntar "sou eu quem
//! decide isto?" a um `&[Exclusao]`.
//!
//! **Por que uma entidade e não um `Vec` devolvido.** Mudar o tipo de retorno
//! de `layout_block`/`layout_children_vertical` para carregar as exclusões
//! novas percorreria os 13 sítios de chamada por um valor que só interessa a
//! floats, e cada dispatcher intermédio (flex, grid, coluna, tabela,
//! out-of-flow) teria de aprender a concatenar duas listas em vez de ignorar o
//! parâmetro que já ignora hoje. Em vez disso, as exclusões vivem num
//! `RefCell` dentro do `BlockFormattingContext`: um container que NÃO
//! estabelece o seu próprio BFC recebe a MESMA referência do antepassado que o
//! estabeleceu (em vez de criar uma nova), e escrever nela — três níveis de
//! recursão abaixo — fica visível a quem a possui sem nenhum valor a subir
//! pela pilha de chamadas. Um container que estabelece BFC (CSS 2.1 §9.4.1:
//! raiz, float, `position:absolute/fixed`, `overflow`≠visible, `flow-root`,
//! flex/grid/tabela/inline-block) cria uma instância NOVA e vazia — os floats
//! de fora não a atravessam, e só ele cresce para conter os que ela acumula.
//!
//! **O limite que isto ainda não fecha**: a cache de fragmentos
//! (`layout/fragmento.rs`) memoiza uma subárvore por CONSTRAINTS; um float que
//! escapa por ela é detectado (o comprimento do `BlockFormattingContext`
//! muda durante a construção) e o fragmento correspondente deixa de ser
//! GRAVADO — nunca fica em cache, então a próxima passada refaz a subárvore
//! inteira e os floats voltam a ser empurrados corretamente. O que isto NÃO
//! cobre é a COSTURA (`costurar`): um filho que se torna sujo e ganha um float
//! novo que precisa de escapar é reconstruído com um contexto ISOLADO (o
//! comentário em `layout/fragmento.rs` explica porquê) — um caso raro
//! (alternar `float` num elemento já cacheado, dentro de um pai sem BFC, numa
//! passada incremental) que fica sem cobertura nesta entrega, documentado em
//! vez de escondido.

use super::float::Exclusao;
use crate::style::FloatSide;
use std::cell::RefCell;

/// As exclusões de float ABERTAS de um bloco de formatação, partilhadas por
/// referência com todo o descendente que não estabelece o seu próprio BFC.
///
/// Guarda os dois lados NUMA lista (o campo `side` de [`Exclusao`] já os
/// distingue) em vez de dois `Vec`: um `clear:both` e o crescimento do pai
/// pedem os dois lados juntos tantas vezes quanto um só, e duas listas
/// obrigariam a mesclar/ordenar de volta sempre que fosse preciso o conjunto
/// inteiro (a busca de banda livre de um novo float, por exemplo).
#[derive(Default)]
pub(crate) struct BlockFormattingContext {
    floats: RefCell<Vec<Exclusao>>,
}

impl BlockFormattingContext {
    // `pub(crate)`, não `pub(in crate::layout)`: `crate::table` (fora do
    // módulo `layout`) precisa de construir um BFC isolado para os itens que
    // não recebem exclusões (célula de tabela, `<caption>`) — o resto do
    // `impl` fica mais estreito porque só é lido de dentro de `layout`.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `true` quando NENHUM float está aberto neste BFC — a mesma pergunta
    /// que decidia se um bloco podia ser servido pela cache de fragmentos
    /// antes deste módulo existir (um bloco ao lado de um float não pode: foi
    /// medido com a banda livre em conta, e a banda não é parte da chave).
    pub(in crate::layout) fn is_empty(&self) -> bool {
        self.floats.borrow().is_empty()
    }

    /// Quantos floats estão abertos — usado só para detectar que uma
    /// construção ACRESCENTOU floats (antes/depois), não para os ler.
    pub(in crate::layout) fn len(&self) -> usize {
        self.floats.borrow().len()
    }

    /// Regista um float novo, colocado por quem quer que esteja a percorrer
    /// este BFC agora — o ponto em que um float dentro de um container SEM
    /// BFC próprio "escapa": a referência é a mesma do antepassado que criou
    /// este valor, então o `push` fica visível a ele sem retorno nenhum.
    pub(in crate::layout) fn push(&self, exclusao: Exclusao) {
        self.floats.borrow_mut().push(exclusao);
    }

    /// A banda livre entre `y` e `y + altura`, considerando TODOS os floats
    /// abertos — delega em [`super::float::banda_livre`], a mesma fórmula de
    /// sempre, só que lendo do `RefCell` em vez de um `Vec` local.
    pub(in crate::layout) fn banda_livre(
        &self,
        y: f32,
        altura: f32,
        content_x: f32,
        content_w: f32,
    ) -> (f32, f32) {
        super::float::banda_livre(&self.floats.borrow(), y, altura, content_x, content_w)
    }

    /// Os fundos dos floats abertos, um por float, sem ordenar — para o laço
    /// de colocação de um float NOVO, que desce até ao fundo de cada um que
    /// estorve a banda pedida (ver o uso em `vertical.rs`).
    pub(in crate::layout) fn fundos(&self) -> Vec<f32> {
        self.floats.borrow().iter().map(|e| e.bottom).collect()
    }

    /// Uma CÓPIA das exclusões abertas — para o único consumidor que precisa
    /// do `&[Exclusao]` bruto ([`super::linha::layout_inline_flow`], que só
    /// LÊ, nunca escreve, e não tem por que aprender o `RefCell`). Correr o
    /// clone uma vez por flush de linha é barato: o número de floats abertos
    /// é o de floats na página, não de linhas.
    pub(in crate::layout) fn snapshot(&self) -> Vec<Exclusao> {
        self.floats.borrow().clone()
    }

    /// O fundo do float mais baixo NOS LADOS pedidos — a resposta por lado que
    /// `clear:left`/`right`/`both` precisam (CSS 2.1 §9.5.2) e que antes deste
    /// módulo não existia: os três valores respondiam o mesmo fundo porque só
    /// havia UMA lista combinada (ver `style::text::Clear`, que documentava o
    /// corte). `(true, true)` é o `both` e também o que o crescimento do pai
    /// usa (10.6.7: um BFC contém floats dos dois lados).
    pub(in crate::layout) fn fundo_lado(&self, esquerda: bool, direita: bool) -> Option<f32> {
        let floats = self.floats.borrow();
        let filtrados: Vec<Exclusao> = floats
            .iter()
            .copied()
            .filter(|e| match e.side {
                FloatSide::Left => esquerda,
                FloatSide::Right => direita,
                FloatSide::None => false,
            })
            .collect();
        super::float::fundo_dos_floats(&filtrados)
    }
}
