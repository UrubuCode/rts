//! FLUXO VERTICAL: empilhar filhos de bloco uns sobre os outros, colapsar as
//! margens entre eles, e a linha de `inline-block`.
//!
//! **Acima do teto, e a fronteira não se mexeu para o evitar.** O colapso de
//! margens fica aqui e não num módulo de perguntas sobre caixas, porque é aqui
//! que ele é usado — e porque é a próxima frente de correção conhecida (conta
//! a dobro e acumula por nível de aninhamento). Mover uma fronteira para
//! servir um número é escolher o número em vez do desenho.
//!
//! **A ORDEM deste laço passou a vir da ÁRVORE DE CAIXAS.** Era
//! `dom.node(id).children` que decidia por que filhos se desce e por que ordem,
//! e era essa linha que impedia uma caixa que o DOM não tem de chegar aqui, por
//! melhor construída que estivesse. Agora a sequência é
//! [`super::sequence::flow_sequence`] sobre `tree.children(box_id)`, e é lá
//! que está escrito o que ainda vem do DOM (um comentário, que não gera caixa).
//!
//! **Uma caixa ANÓNIMA é um passo deste laço, e é disposta como o bloco que é**
//! — por `super::block_box`, o caminho que aceita uma caixa sem nó. Um
//! `<span>` com um `<div>` dentro já não é um filho deste contentor: a partição
//! do CSS 2.1 §9.2.1.1 pôs aqui, no lugar dele, uma anónima por corrida e o
//! `<div>` como irmão delas (ver `boxes/build.rs`).
//!
//! **O `id` deixa de ser o dono do fluxo quando a caixa é anónima.** Ele é então
//! o CONTENTOR de quem ela herda, que já corre o seu próprio laço mais acima —
//! por isso tudo o que é do ELEMENTO e não de cada corrida dele (o `::before`, o
//! `::after` de bloco, o clearfix) é recusado aqui, ou aparecia uma vez por
//! corrida.
//!
//! **O AGRUPAMENTO inline é o mesmo algoritmo, e continua a ser.** O que mudou
//! é DE ONDE vem a sequência, não quem agrupa: `is_box` continua a excluir
//! texto de propósito, porque responder `true` ali manda cada palavra pelo
//! caminho de bloco e parte-a numa linha própria. Quem dispõe o texto é
//! `collect_runs`, através do grupo inline.
//!
//! Uma das cinco cópias da pergunta "é de bloco?" vive DENTRO do laço de
//! `layout_children_vertical`, escrita à mão — ver o cabeçalho de `box_kind.rs`.
//! Não é movível sem extrair uma função, o que deixa de ser um `move`.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi alterada.
//!
//! **O crescimento para conter floats DEIXOU de viver aqui.** Vivia no fim
//! desta função, incondicional (a divergência que `float_left_right_dividem_a_linha`
//! pinava de propósito); agora é `block.rs` que decide, porque só ele sabe se
//! `id` é o BFC responsável — ver `block/bfc.rs`.

pub(in crate::layout) use super::margin_collapse::{
    Strut, collapsed_strut, collapses_through, join_strut,
};
use super::sequence::{FlowStep, flow_sequence};
use super::*;
use crate::boxes::BoxId;

/// Empilha os filhos VERTICAL (cada um abaixo do anterior), ocupando a largura do
/// content. Devolve a altura TOTAL do content (soma das alturas dos filhos).
/// `avail_h` = altura do content DESTE container quando explícita (containing
/// block dos filhos p/ `height:%`).
// a macro de estado (flush_inline!) escreve no cursor a cada fechamento — a
// ÚLTIMA atribuição (no flush final) é estruturalmente morta, o que dispara
// unused_assignments sem haver bug.

#[allow(unused_assignments)]
pub(in crate::layout) fn layout_children_vertical(
    dom: &Dom,
    id: NodeIdx,
    // A CAIXA de `id` — ao lado do `NodeIdx`, não em vez dele. É ela que dá a
    // SEQUÊNCIA dos filhos, por [`super::sequence::flow_sequence`]: quem
    // desce já não pergunta ao DOM por que filhos desce nem por que ordem.
    box_id: BoxId,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    definite_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    // O bloco de formatação AMBIENTE — de quem o estabeleceu, herdado por
    // referência quando `id` (o pai destes filhos) não estabelece o seu
    // próprio (ver `layout_block`). Um float colocado aqui é escrito NELE
    // (`bfc.push`), então um `<div>` sem BFC dentro de outro `<div>` sem BFC
    // não perde o float ao subir — a mesma referência chega ao dono, seja ele
    // quem for. Ver `block/bfc.rs`.
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut child_y = content_y;
    let cb_h = definite_h.or_else(|| crate::inline_box::replaced_clamp::definite_cb_height(css, avail_h));
    // A base da chave de fragmento é a mesma para todos os filhos deste
    // container — só o nó e o epoch dele mudam.
    let key_base = KeyBase::new(dom, content_w, avail_h, ctx);
    // MARGIN-COLLAPSE: as margens verticais de blocos ADJACENTES colapsam numa
    // só, não somam — e colapsam TODAS DE UMA VEZ, não duas a duas. São por
    // isso dois valores e não um:
    //
    // `border` é onde acabou a última caixa que ocupou espaço (a aresta de baixo
    // dela, sem a margem), e `strut` é o conjunto de margens adjacentes aberto
    // desde então. A aresta de topo do bloco seguinte é
    // `border + collapsed_strut(strut)` — uma SOMA, e não a subtração de um
    // excesso, que é o que permite ao conjunto ter mais de dois membros.
    //
    // A invariante que liga isto ao resto do laço: `child_y` — o cursor que o
    // fluxo inline e os floats usam — é sempre `border + collapsed_strut(strut)`.
    let mut border = content_y;
    let mut strut: Strut = (0.0, 0.0);
    // ── CONTEXTO INLINE (P4): irmãos inline CONSECUTIVOS (texto + <a>/<b>/<span>)
    // fluem JUNTOS numa sequência de linhas — acumulados aqui e descarregados por
    // `flush_inline!` quando um bloco/float/fim interrompe o fluxo.
    // Cada membro traz a SUA caixa, a que a sequência da árvore deu. É por ela
    // que `collect_runs` desce (o fragmento de um inline partido não vê o
    // `<div>` que o partiu) e é ela que chega ao ÁTOMO — um inline-flex a meio
    // de texto é disposto pela caixa exacta, e sem ela `layout_block` não a
    // tinha para dar ao contentor flex nem à ordem de hit-test.
    let mut inline_group: Vec<(NodeIdx, BoxId)> = Vec::new();
    // A box-less DOM child (`FlowStep::NoBox`: a comment, a whitespace
    // run the split declined to wrap) OPENS the inline group without joining
    // it — it has no box to be collected by. What it did before BT-2a, as a
    // member with no box, was exactly that: `collect_runs` produced nothing
    // for it, and the flush that closed the group reset `border` and `strut`,
    // breaking the margin collapse of the two blocks around it. Whether it
    // SHOULD is a lot of its own, measured on its own; this flag keeps the
    // answer where it was.
    let mut group_open = false;
    // Corrida de INLINE-BLOCKS consecutivos (botões/pills lado a lado). Pintada
    // por `flush_ib` — mede cada um (shrink), põe lado a lado quebrando linha ao
    // encher, e alinha a linha pelo text-align do pai (center do google).
    let mut ib_run: Vec<(NodeIdx, BoxId)> = Vec::new();
    macro_rules! flush_ib {
        ($y:expr) => {
            if !ib_run.is_empty() {
                $y = layout_inline_block_line(
                    dom, id, &ib_run, content_x, $y, content_w, cb_h, css, font_size, ctx, list,
                );
                ib_run.clear();
                border = $y;
                strut = (0.0, 0.0);
            }
        };
    }
    macro_rules! flush_inline {
        ($y:expr) => {
            if !ib_run.is_empty() {
                flush_ib!($y);
            }
            if !inline_group.is_empty() || group_open {
                // Does NOT move below the floats: the lines go AROUND them. The
                // reference and not a copy: a float that appears in the MIDDLE
                // of the group is placed in there (`in_line.rs`) and has
                // to reach the siblings that come after.
                $y = layout_inline_flow(
                    dom,
                    id,
                    &inline_group,
                    content_x,
                    $y,
                    content_w,
                    cb_h,
                    css,
                    font_size,
                    bfc,
                    ctx,
                    list,
                );
                inline_group.clear();
                group_open = false;
                // texto quebra a sequência de margin-collapse
                border = $y;
                strut = (0.0, 0.0);
            }
        };
    }
    // A ÁRVORE é quem dá a sequência e a ordem. `Rc` clonado e não um
    // empréstimo de `list.tree`: `list` é escrito ao longo do laço inteiro — a
    // mesma razão pela qual `layout_document` o clona antes de tocar em
    // `box_rects`.
    let from_tree = std::rc::Rc::clone(&list.tree);
    // **ESTA descida é a de uma caixa ANÓNIMA?** Se for, `id` não é o dono do
    // fluxo: é o contentor de quem a anónima herda, e ele já está a correr o SEU
    // próprio `layout_children_vertical` mais acima. Tudo o que pertence ao
    // ELEMENTO e não a cada corrida dele — as caixas geradas de bloco e o
    // clearfix — é emitido lá e tem de ser recusado aqui, ou aparece uma vez por
    // corrida. Ver `block_box.rs`.
    let is_anonymous = matches!(
        from_tree.kind(box_id),
        crate::boxes::BoxKind::Anonymous { .. }
    );
    // `::before` de BLOCO com conteúdo — o primeiro do fluxo, antes de
    // qualquer filho real. Ver `pseudo_block.rs`.
    if !is_anonymous {
        super::pseudo_block::apply(
            dom,
            box_id,
            id,
            crate::style::PseudoElement::Before,
            content_x,
            content_w,
            font_size,
            &mut border,
            &mut strut,
            &mut child_y,
            ctx,
            list,
        );
    }
    let steps = flow_sequence(dom, &from_tree, id, box_id);
    for item in &steps {
        // **A CAIXA ANÓNIMA É UM PASSO, e é disposta como o BLOCO que é.** Ela
        // não tem margem, borda nem padding (CSS 2.1 §9.2.1.1: nenhuma
        // declaração é sua), por isso não há um `m` a colapsar — o conjunto
        // aberto fecha-se contra a aresta e ela começa exactamente aí.
        //
        // Uma anónima de altura zero ATRAVESSA-SE: a corrida que ela envolve não
        // pintou nada, e fechar o colapso ali separaria dois blocos que o
        // browser junta.
        let anonymous = match *item {
            FlowStep::AnonymousBox(b) => Some(b),
            FlowStep::Node { .. } | FlowStep::NoBox(_) => None,
        };
        if let Some(anon) = anonymous {
            flush_inline!(child_y);
            let edge = border + collapsed_strut(strut);
            child_y = edge;
            let h = super::block_box::layout_anonymous(
                dom, &from_tree, anon, content_x, child_y, content_w, avail_h, definite_h, css, font_size, bfc,
                ctx, list,
            );
            if !collapses_through(h, 0.0, 0.0) {
                border = edge + h;
                strut = (0.0, 0.0);
            }
            child_y = border + collapsed_strut(strut);
            continue;
        }
        // A box-less child: the same three answers it got as a `(child, None)`
        // member before BT-2a, and no other. Non-rendered metadata is skipped;
        // whitespace between blocks is skipped; anything else — a comment, a
        // separator whitespace — closes the inline-block run and opens the
        // inline group (see `group_open`).
        if let FlowStep::NoBox(dom_node) = *item {
            match &dom.node(dom_node).kind {
                NodeKind::Comment(_) | NodeKind::Document => {}
                NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
                NodeKind::Text(t)
                    if crate::inline_box::so_espaco_css(t)
                        && !whitespace_is_inline_separator(dom, id, dom_node) => {}
                _ => {
                    flush_ib!(child_y);
                    group_open = true;
                }
            }
            continue;
        }
        let FlowStep::Node {
            dom_node: child,
            box_id: child_box,
        } = *item
        else {
            unreachable!("the anonymous and the box-less steps left the loop above");
        };
        // Um FRAGMENTO de inline partido — o único nó com mais de uma caixa — é
        // conteúdo de linha e nunca serve o fragmento guardado do nó (abaixo).
        let child_fragment = (from_tree.boxes_of(child).len() > 1).then_some(child_box);
        let is_text = matches!(dom.node(child).kind, NodeKind::Text(_));
        // **O agrupamento é o MESMO algoritmo, e `is_box` só fala de caixas
        // que não são de texto.** Um nó de texto tem caixa, mas quem o dispõe
        // continua a ser `collect_runs`, que o agrupa com os irmãos inline.
        // Responder `true` aqui para texto mandá-lo-ia pelo caminho de bloco e
        // partia cada palavra numa linha própria — foi medido, e é por isso que
        // esta linha tem esta forma.
        //
        // Quem decide se este filho é uma caixa é a ÁRVORE: um nó sem caixa
        // já saiu do laço acima.
        let is_box = !is_text;
        // CAMINHO RÁPIDO: se existe fragmento para este filho com estas
        // constraints, ele já foi classificado como BLOCO NORMAL quando foi
        // criado — é o único caminho que produz fragmento. Encontrá-lo responde
        // a classificação inteira, que custaria estilo computado,
        // `block::lookup` e a margem resolvida por filho: mil vezes por frame
        // numa lista, para redescobrir o que não mudou.
        // `bfc.is_empty()`: um bloco com float ao lado não pode ser servido
        // pelo fragmento guardado — ele foi medido com a linha inteira e a banda
        // livre não faz parte da chave. É a mesma recusa de
        // `layout_block_reusing`, no caminho rápido que a antecede.
        // `child_fragment.is_none()`: um FRAGMENTO de inline partido aparece
        // no fluxo uma vez por corrida, e a chave de fragmento é do NÓ — servir
        // o mesmo fragmento guardado a cada corrida pintava o conteúdo inteiro
        // do inline uma vez por metade. Um nó com uma caixa só não é afectado,
        // que é toda a gente menos o inline que se partiu.
        if bfc.is_empty() && is_box && child_fragment.is_none() {
            let key = key_base.key(dom, child, child_box, None, None, false);
            let tree = dom.box_tree();
            if let Some(fragment) = dom
                .fragment_get(key)
                .and_then(|fragment| fragment.remapped_to(&tree))
            {
                crate::bump!(fragment_hits);
                flush_inline!(child_y);
                let (top_margin, bottom_margin) = (fragment.margin_top, fragment.margin_bottom);
                let (_, escaped_bottom) = crate::layout::block::block::escaped_margins_for_box(
                    dom, child, content_w, font_size, ctx,
                );
                let bottom_margin =
                    crate::layout::block::block::collapse_margin(bottom_margin, escaped_bottom);
                let with_top = join_strut(strut, top_margin);
                let edge = border + collapsed_strut(with_top);
                child_y = edge - top_margin;
                emit_fragment(
                    &fragment, list, content_x, child_y, content_w, avail_h, None, None, false,
                );
                if collapses_through(fragment.size.1, top_margin, bottom_margin) {
                    strut = join_strut(with_top, bottom_margin);
                } else {
                    border = edge + (fragment.size.1 - top_margin - bottom_margin);
                    strut = join_strut((0.0, 0.0), bottom_margin);
                }
                child_y = border + collapsed_strut(strut);
                continue;
            }
        }
        // O estilo vem da ÁRVORE (invariante I6 de `box-tree.md`): `tree.style`
        // responde pela ORIGEM do estilo — o elemento, ou, para uma caixa sem
        // nó, aquele de quem ela herda — e pergunta ao documento CADA VEZ. Não é
        // uma cópia guardada na construção: essa estaria um frame atrasada
        // durante toda uma transição, porque `Dom::box_tree` é chaveada por
        // `(revision, style_epoch)` e deliberadamente NÃO por `anim_epoch`.
        //
        // Para uma caixa de ELEMENTO a resposta é exactamente
        // `dom.computed_style_idx(child)`, que é o que estava aqui — a troca não
        // muda valor nenhum hoje e deixa de ter um `NodeIdx` no caminho.
        let child_css = is_box.then(|| {
            from_tree
                .style(dom, child_box)
                .or_else(|| dom.computed_style_idx(child))
                .unwrap_or_default()
        });
        let child_out = child_css
            .as_ref()
            .and_then(|c| c.position)
            .map(|p| p.out_of_flow())
            .unwrap_or(false);
        let child_float = child_css
            .as_ref()
            .and_then(|c| c.float_side)
            .unwrap_or(crate::style::FloatSide::None);
        // A clearance precisa de dois valores que o cursor sozinho não dá: o
        // fundo do float (só nos LADOS que este `clear` pede), e o sítio onde o
        // bloco ficaria SEM ele. Pelo CSS 2.1 §9.5.2 a aresta de borda fica no
        // MAIOR dos dois — e não no fundo do float MAIS a margem, que é o que
        // somar as duas coisas dá.
        //
        // Os dois são `Option` e não um `max` incondicional sobre o cursor: a
        // meio do laço `child_y` é o CURSOR e não uma aresta de borda, e
        // compará-los é tomar o maior de duas coisas diferentes — com uma
        // margem negativa isso devolvia o bloco para baixo em vez de o puxar
        // para cima.
        let mut clearance: Option<f32> = None;
        // (a referência de onde a aresta é medida é a `border`, que o
        // `flush_inline!` do próprio `clear` acaba de pôr no cursor.)
        // `clear` — o par do `float`: este filho começa ABAIXO dos floats
        // correntes DO LADO que declara. Fica ANTES do dispatch por tipo de
        // caixa porque vale para qualquer um deles: o caminho de bloco já lia
        // `clearance` sempre, mas um inline-block ou um texto com `clear` não
        // tinha como descer e acabava por cima do float.
        //
        // `Clear::sides()` é o que faltava para os três valores deixarem de
        // responder o mesmo fundo (ver `style::text::Clear`, que documentava o
        // corte): `left` só lê o lado esquerdo do BFC, `right` só o direito,
        // `both` os dois — a mesma pergunta que `bfc.side_bottom` existe para
        // responder.
        if let Some((left, right)) = child_css
            .as_ref()
            .and_then(|c| c.clear)
            .map(|c| c.sides())
            .filter(|&(e, d)| e || d)
        {
            flush_inline!(child_y);
            clearance = bfc.side_bottom(left, right);
            // Desce o cursor para BAIXO do float — já não é "fechar a linha": é
            // o que o `clear` pede. Um irmão sem `clear` NÃO passa por aqui:
            // passa ao lado do float. Só usado pelos caminhos que leem
            // `child_y` diretamente (inline/inline-block); o de bloco usa
            // `clearance` sozinho, combinado com a margem em vez de somado.
            if let Some(clear_to) = clearance {
                child_y = child_y.max(clear_to);
            }
        }
        let (child_block, child_inline_block) =
            super::vertical_child::classify_child(dom, child, child_css.as_deref(), child_fragment);
        match &dom.node(child).kind {
            // Metadata não-renderável (`<head>`/`<title>`/`<style>`/`<script>`):
            // pula — NÃO coleta seu texto como inline (senão o título e o CSS cru
            // vazam pra tela). Checado ANTES do caminho inline.
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            // A FLOAT or an ABSOLUTE box in the middle of TEXT joins the inline
            // group as an anchor: the inline flow places the float (§9.5.1,
            // `in_line.rs`) and records the other's static position
            // (`static_anchor.rs`). With no text pending, the arms below decide.
            NodeKind::Element { .. }
                if (child_out || child_float != crate::style::FloatSide::None)
                    && ib_run.is_empty()
                    && group_has_content(dom, &inline_group) =>
            {
                inline_group.push((child, child_box));
            }
            // Fora do fluxo sem texto pendente: não ocupa espaço aqui — pintado
            // na passada out-of-flow de layout_document. An inline-level one
            // still says where its line would have been, beside the floats.
            // (Not after a pending inline-block run: its line is not placed yet.)
            NodeKind::Element { .. } if child_out => {
                if ib_run.is_empty() {
                    crate::layout::inline::static_anchor::in_block_flow(
                        dom, child, child_box, content_x, content_w, child_y, bfc, list,
                    );
                }
            }
            // FLOAT left/right: encosta ao lado pedido, na primeira faixa a
            // partir do cursor onde CAIBA ao lado dos floats já postos.
            NodeKind::Element { .. } if child_float != crate::style::FloatSide::None => {
                flush_inline!(child_y);
                let float_box = child_box;
                let measured = crate::layout::float::placement::measure_float(
                    dom, &from_tree, child, float_box, content_w, avail_h, css, font_size, ctx,
                );
                crate::layout::float::placement::place_float(
                    dom,
                    child,
                    float_box,
                    child_float,
                    measured,
                    child_y,
                    content_x,
                    content_w,
                    avail_h,
                    bfc,
                    ctx,
                    list,
                );
                // float quebra a sequência de collapse
                border = child_y;
                strut = (0.0, 0.0);
            }
            NodeKind::Element { .. } if child_block && !child_inline_block => {
                flush_inline!(child_y);
                super::vertical_child::layout_block_flow_child(
                    dom,
                    child,
                    child_box,
                    child_css.as_deref(),
                    content_x,
                    content_w,
                    avail_h,
                    font_size,
                    clearance,
                    &mut border,
                    &mut strut,
                    &mut child_y,
                    bfc,
                    ctx,
                    list,
                );
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
            NodeKind::Element { .. } if child_inline_block && in_inline_context(dom, id, child) => {
                flush_ib!(child_y);
                inline_group.push((child, child_box));
            }
            NodeKind::Element { .. } if child_inline_block => {
                // descarrega só o TEXTO inline pendente (não o ib_run — este b
                // continua a acumular os inline-blocks IRMÃOS na mesma corrida).
                if !inline_group.is_empty() {
                    child_y = layout_inline_flow(
                        dom,
                        id,
                        &inline_group,
                        content_x,
                        child_y,
                        content_w,
                        cb_h,
                        css,
                        font_size,
                        bfc,
                        ctx,
                        list,
                    );
                    inline_group.clear();
                }
                ib_run.push((child, child_box));
                border = child_y;
                strut = (0.0, 0.0);
            }
            // Whitespace estrutural continua no DOM, mas não cria uma linha entre
            // blocos/floats. Quando está perto de texto/inline, entra no grupo e o
            // `wrap_runs` o colapsa como um espaço normal.
            NodeKind::Text(t)
                if crate::inline_box::so_espaco_css(t) && !whitespace_is_inline_separator(dom, id, child) => {}
            // Texto / elemento inline: entra no CONTEXTO INLINE corrente — flui
            // com os irmãos inline adjacentes (o flush pinta o grupo inteiro).
            _ => {
                flush_ib!(child_y); // fecha a corrida de inline-blocks
                inline_group.push((child, child_box));
            }
        }
    }
    // descarrega o fluxo inline pendente. O crescimento para conter os floats
    // DESTE container não vive mais aqui — ver o cabeçalho do módulo e
    // `layout_block`, que é quem sabe se `id` é o BFC responsável.
    flush_inline!(child_y);
    // `::after` de BLOCO com conteúdo — o último do fluxo. Ver `pseudo_block.rs`.
    // Recusado numa caixa anónima pela mesma razão do `::before`: a caixa gerada
    // é do ELEMENTO, e é emitida na descida dele.
    if !is_anonymous {
        super::pseudo_block::apply(
            dom,
            box_id,
            id,
            crate::style::PseudoElement::After,
            content_x,
            content_w,
            font_size,
            &mut border,
            &mut strut,
            &mut child_y,
            ctx,
            list,
        );
        // o clearfix (`::after{display:block;clear:both}`) desce o fim do fluxo
        // até ao fundo dos floats — ver `clearfix.rs`.
        if let Some(clear_to) = crate::layout::float::clearfix::clearfix_bottom(dom, id, bfc) {
            child_y = child_y.max(clear_to);
        }
    }
    (child_y - content_y).max(0.0)
}

/// Does the pending inline group hold anything besides collapsible space? The
/// question that decides whether a following float appears in the MIDDLE of a
/// line or before it.
fn group_has_content(dom: &Dom, group: &[(NodeIdx, BoxId)]) -> bool {
    group.iter().any(|&(n, _)| match &dom.node(n).kind {
        NodeKind::Text(t) => !crate::inline_box::so_espaco_css(t),
        _ => true,
    })
}
