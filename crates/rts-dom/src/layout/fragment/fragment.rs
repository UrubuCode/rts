//! FRAGMENTOS: o desenho de uma subárvore guardado em coordenadas relativas, a
//! chave que o valida, e a costura que o reinsere numa lista sem o recalcular.
//!
//! É o que torna o layout incremental: mudar uma folha invalida o epoch dela e
//! dos ancestrais, e todo irmão intacto reusa o fragmento em vez de refazer
//! cascade, medição de texto e box model.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

use super::fragment_key::fragment_key;
pub(in crate::layout) use super::fragment_key::KeyBase;

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
    let (antiga, anterior) = dom.last_fragment_of(key.target)?;
    // Só o epoch do nó pode diferir: viewport, constraints, estilo global e
    // animação mudam o desenho inteiro, não uma parte dele.
    if (
        antiga.tree,
        antiga.avail_w,
        antiga.avail_h,
        antiga.viewport_w,
        antiga.viewport_h,
    ) != (
        key.tree,
        key.avail_w,
        key.avail_h,
        key.viewport_w,
        key.viewport_h,
    ) || (antiga.style_epoch, antiga.anim_epoch, antiga.measurer)
        != (key.style_epoch, key.anim_epoch, key.measurer)
    {
        return None;
    }
    if crate::paint::pieces::children(&anterior.pieces).next().is_none() {
        return None;
    }
    // Um container cujos filhos carregam tamanho IMPOSTO (flex, grid,
    // out-of-flow) nunca é costurado: a imposição vem de um algoritmo de
    // DISTRIBUIÇÃO entre irmãos (grow/shrink, tracks, stretch), e a costura só
    // sabe verificar se a ALTURA do filho reposto bateu com a antiga — não se a
    // distribuição inteira precisava mudar por causa dele (um `flex-grow` cujo
    // conteúdo cresceu pode precisar de MENOS espaço para os outros itens, sem
    // que a altura do próprio item mude). Recusar aqui força o container a
    // refazer `layout_block` — que reexecuta a distribuição do zero — e deixa
    // cada ITEM, individualmente, bater no cache por `FragmentKey` exata
    // (ver `layout_block_reusing`). Bloco normal nunca tem `forced_outer_*`
    // definido, então este guard não custa nada ao caminho comum.
    if crate::paint::pieces::children(&anterior.pieces)
        .any(|c| c.forced_outer_w.is_some() || c.forced_outer_h.is_some())
    {
        return None;
    }
    let sujos = dom.dirty_children_of(id)?;
    let tree = dom.box_tree();
    // A árvore que emitiu o desenho antigo, guardada ANTES de reidratar: é
    // contra a caixa dela que a sequência de filhos se compara.
    let (arvore_antiga, caixa_antiga) = (std::rc::Rc::clone(&anterior.tree), anterior.caixa);
    // O fragmento pode ter sido produzido pela árvore anterior. Reidratar na
    // entrada é a única fronteira permitida para os `BoxId`s que ele guarda.
    let anterior = anterior.remapped_to(&tree)?;
    // Inserção, remoção, reordenação ou uma caixa anónima nova mudam quem
    // desenha o quê, e trocar uma referência não daria conta disso.
    if !super::stitching::mesma_sequencia_de_filhos(
        &arvore_antiga,
        caixa_antiga,
        &tree,
        anterior.caixa,
    ) || !super::stitching::sujeira_coberta(&tree, &sujos, &anterior.pieces)
    {
        return None;
    }
    let _phase = crate::metrics::phases::scope("fragment-patch");

    // The dirty child is replaced WHERE IT STANDS, found by its box: its
    // `Piece::Child` keeps its place in the sequence, so nothing painted around
    // it moves and there is no index to correct.
    let mut pieces = (*anterior.pieces).clone();
    let mut grid_column_tracks = (*anterior.grid_column_tracks).clone();
    let mut trocou = false;
    for piece in &mut pieces {
        let super::Piece::Child(child) = piece else { continue };
        let Some(child_node) = tree.node_of(child.caixa) else {
            return None;
        };
        if !sujos.contains(&child_node) {
            continue;
        }
        let previous_grid_nodes: Vec<NodeIdx> = child
            .fragment
            .grid_column_tracks
            .iter()
            .map(|(node, _)| *node)
            .collect();
        let mut own = DisplayList::for_dom(dom);
        // Onde o filho FOI POSTO: a origem em que o fragmento dele foi calculado
        // mais o deslocamento com que entrou aqui. Somar à origem do PAI daria
        // uma posição sem sentido — foi o que o teste de equivalência mostrou,
        // com o texto reaparecendo em (0,16) em vez de (12, 67.4).
        let origem = (
            child.fragment.origin.0 + child.dx,
            child.fragment.origin.1 + child.dy,
        );
        let margem = (child.margin_top, child.margin_bottom);
        let ((_, altura), nova_margem) = layout_block_reusing(
            dom,
            child_node,
            child.caixa,
            origem.0,
            origem.1,
            child.avail_w,
            child.avail_h,
            || margem,
            // O guard acima já recusa costura quando algum filho tem tamanho
            // imposto — estes dois são sempre `None`/`false` aqui, mas lidos
            // do `ChildRef` (em vez de escritos `None`/`false` à mão) para que
            // um guard futuro mais permissivo não precise tocar nesta linha.
            child.forced_outer_w,
            child.forced_outer_h,
            // O guard no topo desta função já recusa a costura quando algum
            // filho tem `forced_outer_h` — que é sempre o caso de um MAIN
            // SIZE de coluna (é `Some` incondicionalmente ali). Este ramo
            // nunca corre com `hard=true` na prática; `false` é a leitura
            // honesta do que `ChildRef` sabe (não guarda a flag — ver o
            // comentário em `layout_block_reusing`).
            false,
            child.shrink_to_fit,
            // A costura só alcança o que virou fragmento, e um bloco estorvado
            // por float — ou que ele próprio deixa floats escaparem para o
            // BFC ambiente — nunca vira (ver os dois guards em
            // `layout_block_reusing`). Um BFC NOVO e isolado é por isso seguro
            // aqui: SE este filho sujo passar a ter um float que precisaria
            // de escapar, ele deixa de bater no cache na PRÓXIMA passada (o
            // guard reage ao comprimento do BFC), mas NESTA passada de
            // costura o float fica preso a este contexto descartável — um
            // limite conhecido, não escondido: ver o cabeçalho de
            // `block/bfc.rs`.
            &BlockFormattingContext::new(),
            ctx,
            &mut own,
        );
        if (altura - child.height).abs() > 0.001
            || (nova_margem.0 - child.margin_top).abs() > 0.001
            || (nova_margem.1 - child.margin_bottom).abs() > 0.001
        {
            return None;
        }
        // O `layout_block_reusing` emitiu numa lista própria; o que interessa é a
        // referência que ele acabou de registrar para este nó — com o
        // deslocamento DELA. O antigo levava o fragmento velho até `origem`; o
        // novo pode ter sido calculado já em `origem` (deslocamento zero) ou ser
        // uma costura que herdou a origem do velho. Manter o `dx`/`dy` antigo
        // aplicava o deslocamento duas vezes a um filho que tinha subido.
        let novo = crate::paint::pieces::children(&own.pieces).next()?;
        grid_column_tracks.retain(|(node, _)| !previous_grid_nodes.contains(node));
        grid_column_tracks.extend(novo.fragment.grid_column_tracks.iter().cloned());
        (child.dx, child.dy) = (novo.dx, novo.dy);
        child.fragment = novo.fragment.clone();
        trocou = true;
    }
    if !trocou {
        return None;
    }
    let ultima_linha = crate::layout::inline::line_baseline::total_da_costura(dom, id, &anterior, &pieces, &tree);
    let fragment = std::rc::Rc::new(Fragment {
        caixa: tree.boxes_of(key.target.node).get(key.target.ordinal as usize).copied()?,
        tree: std::rc::Rc::clone(&tree),
        // Shares what did NOT change; the sequence is new because a subtree in
        // it is.
        pieces: std::rc::Rc::new(pieces),
        rects: std::rc::Rc::clone(&anterior.rects),
        grid_column_tracks: std::rc::Rc::new(grid_column_tracks),
        scroll_regions: anterior.scroll_regions.clone(),
        linha_directa: anterior.linha_directa,
        ultima_linha,
        ancoras_estaticas: std::rc::Rc::clone(&anterior.ancoras_estaticas),
        origin: anterior.origin,
        size: anterior.size,
        margin_top: anterior.margin_top,
        margin_bottom: anterior.margin_bottom,
    });
    dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    Some(fragment)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn emit_fragment(
    fragment: &std::rc::Rc<Fragment>,
    list: &mut DisplayList,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    shrink_to_fit: bool,
) {
    let _phase = crate::metrics::phases::scope("fragment-emit");
    fragment.emit_at(
        list,
        x,
        y,
        avail_w,
        avail_h,
        forced_outer_w,
        forced_outer_h,
        shrink_to_fit,
    );
}

/// A mesma [`layout_block`], mas passando primeiro pelo cache de fragmentos.
///
/// `forced_outer_w`/`forced_outer_h`/`shrink_to_fit` são a imposição do
/// CHAMADOR — o fluxo de bloco normal manda `None`/`None`/`false`; um item de
/// flex, coluna ou grid manda o que a distribuição decidiu para ele. Entram na
/// chave (ver `FragmentKey`) porque são parte do que determina o desenho tanto
/// quanto `avail_w`/`avail_h`: dois itens do MESMO nó, um esticado e outro não,
/// não podem servir um ao outro.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_block_reusing(
    dom: &Dom,
    id: NodeIdx,
    caixa: crate::boxes::BoxId,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    margens: impl FnOnce() -> (f32, f32),
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    // Ver o comentário em `layout_block` (`block.rs`) — só
    // `layout_children_column` passa `true`. Fica de fora da `FragmentKey`
    // de propósito: o `style_epoch` global já invalida o cache inteiro
    // quando um ancestral muda de `display`/`flex-direction` (o único jeito
    // do MESMO nó passar a ser chamado com um valor diferente), então não
    // há colisão possível a proteger.
    forced_outer_h_hard: bool,
    shrink_to_fit: bool,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> ((f32, f32), (f32, f32)) {
    // Um bloco ESTORVADO por um float não entra no cache de fragmentos, nem sai
    // dele: a chave é feita das constraints (largura, altura, viewport) e a
    // banda livre não é nenhuma delas. Sem esta recusa, o parágrafo ao lado da
    // figura seria servido pela versão de largura cheia guardada antes — e o
    // contrário também, a versão estreita reusada longe do float. Acrescentar a
    // banda à chave era a outra saída; recusar custa só nos blocos que têm
    // float ao lado, que são poucos, e não põe um campo novo em todas as
    // chaves da página.
    if !bfc.is_empty() {
        let size = layout_block(
            dom,
            id,
            caixa,
            x,
            y,
            avail_w,
            avail_h,
            forced_outer_w,
            forced_outer_h,
            forced_outer_h_hard,
            shrink_to_fit,
            bfc,
            ctx,
            list,
        );
        return (size, margens());
    }
    let key = fragment_key(
        dom,
        id,
        caixa,
        avail_w,
        avail_h,
        forced_outer_w,
        forced_outer_h,
        shrink_to_fit,
        ctx,
    );
    let tree = dom.box_tree();
    if let Some(fragment) = dom
        .fragment_get(key)
        .and_then(|fragment| fragment.remapped_to(&tree))
    {
        crate::bump!(fragment_hits);
        emit_fragment(
            &fragment,
            list,
            x,
            y,
            avail_w,
            avail_h,
            forced_outer_w,
            forced_outer_h,
            shrink_to_fit,
        );
        return (fragment.size, (fragment.margin_top, fragment.margin_bottom));
    }
    // COSTURA: trocar no desenho anterior só a subárvore que ficou suja. Agora
    // que a saída é uma ÁRVORE, costurar é substituir uma REFERÊNCIA num vetor
    // de mil entradas de 48 bytes — a primeira versão disto (revertida) copiava
    // 3000 itens com String e por isso não ganhava nada.
    // The stitch re-lays dirty children into its own lists; their lines are already in `ultima_linha`.
    let (linhas_antes, costurado) = (crate::layout::inline::line_baseline::marca(), costurar(dom, id, key, ctx));
    crate::layout::inline::line_baseline::descarta(linhas_antes);
    if let Some(fragment) = costurado {
        crate::bump!(fragment_patches);
        emit_fragment(
            &fragment,
            list,
            x,
            y,
            avail_w,
            avail_h,
            forced_outer_w,
            forced_outer_h,
            shrink_to_fit,
        );
        return (fragment.size, (fragment.margin_top, fragment.margin_bottom));
    }
    // Resolvidas AQUI e não dentro do literal: o `FnOnce` só se consome uma vez,
    // e os dois campos precisam do mesmo par.
    let margens_resolvidas = margens();
    crate::bump!(fragment_misses);
    let _phase = crate::metrics::phases::scope("fragment-build");
    // Lista PRÓPRIA: o fragmento precisa saber exatamente quais itens são dele,
    // e a única forma de saber isso é não misturá-los com os dos irmãos.
    let mut own = DisplayList::for_dom(dom);
    // `bfc` — a referência AMBIENTE, não uma isolada — porque `id` pode não
    // estabelecer BFC próprio e conter um float que precisa de ESCAPAR para
    // este mesmo `bfc` (ver `block/bfc.rs`). O comprimento antes/depois é
    // como se sabe se isso aconteceu: `floats_escaparam` abaixo.
    let floats_antes = bfc.len();
    let linhas_antes = crate::layout::inline::line_baseline::marca();
    let size = layout_block(
        dom,
        id,
        caixa,
        x,
        y,
        avail_w,
        avail_h,
        forced_outer_w,
        forced_outer_h,
        forced_outer_h_hard,
        shrink_to_fit,
        bfc,
        ctx,
        &mut own,
    );
    // Se esta subárvore ACRESCENTOU floats ao BFC ambiente, o fragmento NÃO
    // é gravado: uma reutilização futura (cache-hit ou costura) só REPINTA o
    // desenho guardado, nunca volta a chamar `layout_block` — e sem chamá-lo
    // o `push` que regista o float no BFC da passada CORRENTE nunca
    // aconteceria. Recusar o cache aqui é o preço; a alternativa (guardar as
    // exclusões produzidas dentro do `Fragment` e reinjectá-las em cada
    // emissão, mesmo em cache-hit) fica para quando um caso real o pedir —
    // documentado, não escondido, no cabeçalho de `block/bfc.rs`.
    let floats_escaparam = bfc.len() != floats_antes;
    let (linha_directa, ultima_linha) = crate::layout::inline::line_baseline::colhe_do_bloco(dom, id, linhas_antes, y, size.1);
    // O desenho guardado conserva a identidade que o produziu: `BoxId`. A
    // chave também a carrega; a passagem seguinte é fazer cada chamador levar
    // a caixa EXACTA, em vez de este caminho ainda obter a primeira do nó.
    // Não traduzir estes vetores de volta para `NodeIdx` evita perder a segunda
    // metade de um inline partido no próprio limite do cache.
    let fragment = std::rc::Rc::new(Fragment {
        caixa,
        tree: std::rc::Rc::clone(&own.tree),
        rects: std::rc::Rc::new(std::mem::take(&mut own.box_rects).into_pairs()),
        grid_column_tracks: std::rc::Rc::new(
            std::mem::take(&mut own.grid_column_tracks)
                .into_iter()
                .collect(),
        ),
        scroll_regions: std::mem::take(&mut own.scroll_regions),
        pieces: std::rc::Rc::new(std::mem::take(&mut own.pieces)),
        linha_directa,
        ultima_linha,
        ancoras_estaticas: std::rc::Rc::new(std::mem::take(&mut own.ancoras_estaticas)),
        origin: (x, y),
        size,
        margin_top: margens_resolvidas.0,
        margin_bottom: margens_resolvidas.1,
    });
    if !floats_escaparam {
        dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    }
    fragment.emit_at(
        list,
        x,
        y,
        avail_w,
        avail_h,
        forced_outer_w,
        forced_outer_h,
        shrink_to_fit,
    );
    (fragment.size, (fragment.margin_top, fragment.margin_bottom))
}
