//! ORDEM DE PINTURA do out-of-flow: um `z-index` NEGATIVO pinta-se ANTES do
//! fluxo normal — atrás dele —, nunca depois (CSS 2.1 Apêndice E, passo 3: só
//! o que tem z-index ≥ 0/`auto` fica por cima). `layout_document` sempre
//! pintou a passada out-of-flow inteira DEPOIS do fluxo inteiro, e o
//! comentário que ali estava dizia isso mesmo ("sem z-index real") — o
//! `sort_by_key` já ordenava os posicionados ENTRE SI, nunca contra o fluxo.
//! Medido (`claude-z-index-negativo-atras-do-fluxo`): um `#atras` vermelho
//! `z-index:-1` e um `#fundo` verde em fluxo, mesmo retângulo 100×100 — a
//! ordem saía [verde, vermelho] (vermelho visível, por cima) e devia ser o
//! oposto.
//!
//! Layout e pintura andam juntos nesta engine — uma só passada, e o veredito
//! da auditoria estrutural lista isso como "o que NÃO é problema" enquanto
//! houver um backend imediato só. Por isso "pintar antes" não é reordenar uma
//! lista: é montar os itens negativos numa lista À PARTE e PREPENDER essa
//! lista à frente da que já existe, corrigindo os índices que dependem de
//! POSIÇÃO — `at`/`hit_at` dos filhos já emitidos, e as contagens
//! `filhos_antes`/`filhos_dentro` dos marcadores de clip já na lista. É a
//! mesma disciplina que [`super::fragmento::insert_item`] já aplica a UM
//! item, generalizada a uma subárvore inteira.
//!
//! Um contexto que `opacity<1`, `transform` ou `z-index` explícito abre não
//! deixa o `z-index` de um filho escapar para o contexto raiz. A passada de
//! fora do fluxo ainda monta fragmentos separadamente, por isso ela usa a
//! chave léxica de TODOS os contextos ancestrais: `[0, 100]` (filho 100 dentro
//! do contexto 0) pinta antes de `[1]` (irmão no contexto raiz). Vários
//! negativos continuam na ordem ascendente dentro do mesmo contexto.
//!
//! **A camada 8 do Apêndice E junta `z-index:0` e `z-index:auto`** — dentro
//! de UM contexto, os dois pintam na ordem do documento, sem se separar por
//! terem ou não um `z-index` explícito (só a camada 3, negativos, e a camada
//! 9, positivos, se separam por número). `4c1d08132` só empurrava um
//! componente para a chave quando o PRÓPRIO nó abria contexto — um
//! `position:absolute` com `z-index:0` (abre contexto, chave `[0]`) ficava
//! mais LONGO que um irmão `z-index:auto` (não abre nada, chave `[]`), e
//! `Vec<i32>::cmp` ordena um prefixo antes do vector que o estende — `[] <
//! [0]` sempre, TREE ORDER à parte. Medido: `#atras{z-index:0}` antes de
//! `#fundo{z-index:auto}` no documento pintava `#fundo` por cima (a chave
//! vazia "vencia"), o oposto do Chrome e do binário pré-codex. A chave agora
//! sempre fecha com o `z-index` do PRÓPRIO nó (`auto` lido como `0`, a mesma
//! leitura de [`z_index_of`]) — os ancestrais que abrem contexto continuam a
//! prefixar, então `[0, 100]` ainda fica preso atrás de `[1]`, mas dois
//! irmãos de camada 8 (`0`/`auto`) empatam e o sort ESTÁVEL decide pela
//! árvore, como a camada pede.

use super::*;

/// O `z-index` computado de um out-of-flow, `0` para `auto`/sem estilo — a
/// mesma leitura que `layout_document` já fazia inline no `sort_by_key`.
///
/// **I6** (`docs/ui/html-engine/box-tree.md` §7): esta função lê o estilo do
/// NÓ, e uma caixa anónima não tem nó nem estilo próprio. Hoje isso não é
/// alcançável — este lote (BT-1) não produz caixas anónimas —, mas o dia em
/// que produzir, esta função responde `0` (a mesma coisa que "auto") em vez
/// de ter um ramo para "esta caixa não tem nó". Deixado como achado para o
/// lote das caixas anónimas, e não corrigido aqui.
pub(in crate::layout) fn z_index_of(dom: &Dom, id: NodeIdx) -> i32 {
    dom.computed_style_idx(id)
        .and_then(|c| c.z_index)
        .unwrap_or(0)
}

/// Chave de pintura de `id`: um componente por contexto ANCESTRAL que o
/// isola (o z-index desse contexto), seguido SEMPRE de um último componente
/// — o `z-index` do próprio `id` (`auto` como `0`) — que é o que mantém
/// `z-index:0` e `z-index:auto` na mesma camada 8 do Apêndice E: dois nós do
/// mesmo contexto, um com `0` explícito e outro `auto`, chegam à mesma chave
/// e o sort ESTÁVEL de `layout_document` desempata pela árvore. Sem esse
/// último componente, o nó que NÃO abre contexto próprio (`auto`) ficava com
/// uma chave mais curta que o irmão que abre (`0`), e um vector mais curto
/// ordena antes do que o estende — sempre, tree order à parte.
///
/// A ordenação léxica mantém uma subárvore inteira contida no lugar do seu
/// ancestral: um filho `z-index:100` de um pai `z-index:0` (`[0, 100]`) não
/// ultrapassa o irmão raiz `z-index:1` (`[1]`), porque o PRIMEIRO componente
/// já decide.
pub(in crate::layout) fn stacking_key(dom: &Dom, id: NodeIdx) -> Vec<i32> {
    let mut ancestors = Vec::new();
    let mut current = dom.node(id).parent;
    while let Some(node) = current {
        ancestors.push(node);
        current = dom.node(node).parent;
    }
    ancestors.reverse();

    let mut key = Vec::new();
    for node in ancestors {
        if creates_context(dom, node) {
            key.push(z_index_of(dom, node));
        }
    }
    key.push(z_index_of(dom, id));
    key
}

/// `id` isola um `z-index` filho num contexto próprio: `opacity<1`,
/// `transform`, um `position` não-`static` com `z-index` explícito (CSS 2.1
/// Apêndice E), ou — Flexbox §4.3 — um ITEM de contentor flex/grid com
/// `z-index` não-`auto`, mesmo `position:static` (um filho direto de
/// `display:flex`/`grid` participa do empilhamento do contentor sem precisar
/// de `position`). `isolation`/`filter`/`will-change` ficam de fora: nenhum
/// dos três tem campo no `ComputedStyle` — `style/inert.rs` os declara
/// INERTES de propósito — e inventar o parse deles não é o corte deste lote.
fn creates_context(dom: &Dom, node: NodeIdx) -> bool {
    let Some(css) = dom.computed_style_idx(node) else {
        return false;
    };
    if css.opacity.is_some_and(|opacity| opacity < 1.0) || css.transform.is_some() {
        return true;
    }
    if css.z_index.is_none() {
        return false;
    }
    let positioned = css
        .position
        .is_some_and(|position| position != crate::style::Position::Static);
    if positioned {
        return true;
    }
    dom.node(node)
        .parent
        .and_then(|parent| dom.computed_style_idx(parent))
        .and_then(|parent_css| parent_css.effective_display())
        .is_some_and(|display| {
            matches!(
                display,
                crate::style::DisplayKind::Flex
                    | crate::style::DisplayKind::FlexWrap
                    | crate::style::DisplayKind::Grid
            )
        })
}

/// Prepende `antes` a `alvo`: os itens e subárvores de `antes` passam a
/// pintar-se PRIMEIRO — mais atrás — que tudo o que `alvo` já tinha, e os
/// índices de `alvo` que apontam por POSIÇÃO (o `at`/`hit_at` de cada
/// subárvore reusada, e as contagens `filhos_antes`/`filhos_dentro` de cada
/// marcador de clip já emitido) deslocam-se pela mesma quantidade — sem essa
/// correção um clip existente passaria a "conter" as subárvores negativas
/// que nunca esteve dentro dele.
pub(in crate::layout) fn merge_before(alvo: &mut DisplayList, mut antes: DisplayList) {
    if antes.items.is_empty()
        && antes.children.is_empty()
        && antes.box_rects.is_empty()
        && antes.hit_order.is_empty()
    {
        return;
    }
    let n_items = antes.items.len();
    let n_children = antes.children.len();
    let n_hits = antes.hit_order.len();
    for item in alvo.items.iter_mut() {
        match item {
            DisplayItem::BeginClip { filhos_antes, .. } => *filhos_antes += n_children,
            DisplayItem::EndClip { filhos_dentro } => *filhos_dentro += n_children,
            _ => {}
        }
    }
    for child in alvo.children.iter_mut() {
        child.at += n_items;
        child.hit_at += n_hits;
    }
    antes.items.append(&mut alvo.items);
    alvo.items = antes.items;
    antes.children.append(&mut alvo.children);
    alvo.children = antes.children;
    antes.hit_order.append(&mut alvo.hit_order);
    alvo.hit_order = antes.hit_order;
    // `box_rects` é a geometria por CAIXA (era `node_rects`, por nó); a
    // fusão continua sendo uma simples união de mapas — as chaves de `antes`
    // e `alvo` não colidem, porque vêm de subárvores disjuntas.
    alvo.box_rects.extend(antes.box_rects);
    alvo.grid_column_tracks.extend(antes.grid_column_tracks);
    alvo.scroll_regions.splice(0..0, antes.scroll_regions);
}

/// Appends a positioned fragment after the current list, translating the
/// fragment-local item and hit indices on the way.
pub(in crate::layout) fn merge_after(alvo: &mut DisplayList, mut depois: DisplayList) {
    if depois.items.is_empty()
        && depois.children.is_empty()
        && depois.box_rects.is_empty()
        && depois.hit_order.is_empty()
    {
        return;
    }
    let n_items = alvo.items.len();
    let n_hits = alvo.hit_order.len();
    for child in depois.children.iter_mut() {
        child.at += n_items;
        child.hit_at += n_hits;
    }
    alvo.items.append(&mut depois.items);
    alvo.children.append(&mut depois.children);
    alvo.hit_order.append(&mut depois.hit_order);
    alvo.box_rects.extend(depois.box_rects);
    alvo.grid_column_tracks.extend(depois.grid_column_tracks);
    alvo.scroll_regions.append(&mut depois.scroll_regions);
}
