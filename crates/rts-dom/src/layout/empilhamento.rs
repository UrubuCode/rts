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
//! CORTE dito: não há stacking context aninhado nesta engine (`opacity<1`,
//! `transform` não isolam um `z-index` filho do contexto do documento), então
//! só existe UM nível — o que o Apêndice E chama de "root" — e é nele que
//! este módulo actua. Vários `z-index` negativos entre si já saem na ordem
//! certa (o `sort_by_key` de `layout_document` cobre isso; o filtro aqui só
//! separa o grupo, sem reordenar).

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
