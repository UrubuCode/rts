//! FLUXO VERTICAL: empilhar filhos de bloco uns sobre os outros, colapsar as
//! margens entre eles, e a linha de `inline-block`.
//!
//! **517 linhas, dezassete acima do teto, e a fronteira não se mexeu para as
//! evitar.** O colapso de margens fica aqui e não num módulo de perguntas sobre
//! caixas, porque é aqui que ele é usado — e porque é a próxima frente de
//! correção conhecida (conta a dobro e acumula por nível de aninhamento). Mover
//! uma fronteira para servir um número é escolher o número em vez do desenho.
//!
//! Uma das cinco cópias da pergunta "é de bloco?" vive DENTRO do laço de
//! `layout_children_vertical`, escrita à mão — ver o cabeçalho de `caixa.rs`.
//! Não é movível sem extrair uma função, o que deixa de ser um `move`.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;
/// Empilha os filhos VERTICAL (cada um abaixo do anterior), ocupando a largura do
/// content. Devolve a altura TOTAL do content (soma das alturas dos filhos).
/// `avail_h` = altura do content DESTE container quando explícita (containing
/// block dos filhos p/ `height:%`).
// as macros de estado (close_floats!/flush_inline!) escrevem no cursor a cada
// fechamento — a ÚLTIMA atribuição (no flush final) é estruturalmente morta, o
// que dispara unused_assignments sem haver bug.
/// As duas margens adjacentes colapsadas numa só, pela regra do CSS 2.1 §8.3.1.
///
/// Não é `max(a, b)`: essa é a regra apenas quando as DUAS são positivas.
/// - as duas ≥ 0 → a maior;
/// - as duas < 0 → a mais negativa (a que puxa mais);
/// - uma de cada sinal → a SOMA, e é por isso que uma margem negativa cancela
///   uma positiva em vez de ser ignorada por ela.
fn colapso_de_margens(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a < 0.0 && b < 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

/// Quanto é que a soma das duas margens excede a colapsada — o que há a
/// descontar ao cursor, já que cada bloco traz a sua margem dentro da altura.
///
/// A forma antiga era `min(a, b)`, que dá o mesmo resultado enquanto as duas
/// forem positivas e o resultado ERRADO assim que uma é negativa: com `a = 0` e
/// `b = -10px` descontava −10, ou seja SOMAVA 10 ao cursor, e a margem negativa
/// que devia puxar o bloco para cima empurrava-o para baixo. É o `margin-top`
/// negativo dos gutters `.row` do Bootstrap, e a razão de um teste que o pinava
/// ter começado a falhar.
fn excesso_de_margens(a: f32, b: f32) -> f32 {
    a + b - colapso_de_margens(a, b)
}

#[allow(unused_assignments)]
pub(in crate::layout) fn layout_children_vertical(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    // Os floats abertos HERDADOS do contexto de cima (ver `layout_block`).
    herdadas: &[Exclusao],
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut child_y = content_y;
    // A base da chave de fragmento é a mesma para todos os filhos deste
    // container — só o nó e o epoch dele mudam.
    let key_base = KeyBase::new(dom, content_w, avail_h, ctx);
    // MARGIN-COLLAPSE: as margens verticais de blocos ADJACENTES colapsam numa
    // só, não somam. Como o `outer_h` de cada bloco já inclui a sua margem dos
    // dois lados, ao empilhar dois blocos a soma conta
    // `margin_bottom_anterior + margin_top_atual`; subtrai-se o excesso para
    // ficar a colapsada ([`colapso_de_margens`]). `prev_margin` guarda a margem
    // do último bloco posto.
    let mut prev_margin = 0.0f32;
    // ── FLOATS COMO EXCLUSÕES: cada float colocado deixa uma faixa vertical
    // ocupada de um dos lados, e o conteúdo seguinte CONTORNA-A em vez de descer
    // abaixo dela. Ver [`Exclusao`] para a medição no Chrome que fixa o modelo.
    // Os herdados vêm primeiro; os deste container são acrescentados depois, e
    // `proprios` é a fronteira entre uns e outros. A distinção importa no fecho:
    // `clear` desce abaixo de QUALQUER float que o estorve, incluindo os de
    // cima, mas este container só cresce para conter os SEUS — crescer para
    // conter um float do pai punha altura no sítio errado.
    let mut floats: Vec<Exclusao> = herdadas.to_vec();
    let proprios = floats.len();
    // Desce o cursor para BAIXO dos floats. Já não é "fechar a linha": é o que o
    // `clear` pede. Um irmão sem `clear` NÃO chama isto: passa ao lado do float.
    macro_rules! close_floats {
        ($y:expr) => {
            if let Some(fundo) = fundo_dos_floats(&floats) {
                $y = $y.max(fundo);
            }
        };
    }
    // ── CONTEXTO INLINE (P4): irmãos inline CONSECUTIVOS (texto + <a>/<b>/<span>)
    // fluem JUNTOS numa sequência de linhas — acumulados aqui e descarregados por
    // `flush_inline!` quando um bloco/float/fim interrompe o fluxo.
    let mut inline_group: Vec<NodeIdx> = Vec::new();
    // Corrida de INLINE-BLOCKS consecutivos (botões/pills lado a lado). Pintada
    // por `flush_ib` — mede cada um (shrink), põe lado a lado quebrando linha ao
    // encher, e alinha a linha pelo text-align do pai (center do google).
    let mut ib_run: Vec<NodeIdx> = Vec::new();
    macro_rules! flush_ib {
        ($y:expr) => {
            if !ib_run.is_empty() {
                $y = layout_inline_block_line(
                    dom, &ib_run, content_x, $y, content_w, avail_h, css, ctx, list,
                );
                ib_run.clear();
                prev_margin = 0.0;
            }
        };
    }
    macro_rules! flush_inline {
        ($y:expr) => {
            if !ib_run.is_empty() {
                flush_ib!($y);
            }
            if !inline_group.is_empty() {
                // NÃO desce abaixo dos floats: as linhas CONTORNAM-NOS. As
                // exclusões vão com o grupo — é a travessia de camada que o
                // comentário de `layout_inline_flow` justifica.
                $y = layout_inline_flow(
                    dom,
                    id,
                    &inline_group,
                    content_x,
                    $y,
                    content_w,
                    css,
                    font_size,
                    &floats,
                    ctx,
                    list,
                );
                inline_group.clear();
                prev_margin = 0.0; // texto quebra a sequência de margin-collapse
            }
        };
    }
    for &child in &dom.node(id).children {
        // CAMINHO RÁPIDO: se existe fragmento para este filho com estas
        // constraints, ele já foi classificado como BLOCO NORMAL quando foi
        // criado — é o único caminho que produz fragmento. Encontrá-lo responde
        // a classificação inteira, que custaria estilo computado,
        // `block::lookup` e a margem resolvida por filho: mil vezes por frame
        // numa lista, para redescobrir o que não mudou.
        // `floats.is_empty()`: um bloco com float ao lado não pode ser servido
        // pelo fragmento guardado — ele foi medido com a linha inteira e a banda
        // livre não faz parte da chave. É a mesma recusa de
        // `layout_block_reusing`, no caminho rápido que a antecede.
        if floats.is_empty() && matches!(dom.node(child).kind, NodeKind::Element { .. }) {
            let key = key_base.key(dom, child);
            if let Some(fragment) = dom.fragment_get(key) {
                crate::bump!(fragment_hits);
                flush_inline!(child_y);
                child_y -= excesso_de_margens(prev_margin, fragment.margin_top);
                emit_fragment(&fragment, list, content_x, child_y, content_w, avail_h);
                child_y += fragment.size.1;
                prev_margin = fragment.margin_bottom;
                continue;
            }
        }
        let child_css = match &dom.node(child).kind {
            NodeKind::Element { .. } => Some(dom.computed_style_idx(child).unwrap_or_default()),
            _ => None,
        };
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
        // fundo do float, e o sítio onde o bloco ficaria SEM ele. Pelo CSS 2.1
        // §9.5.2 a aresta de borda fica no MAIOR dos dois — e não no fundo do
        // float MAIS a margem, que é o que somar as duas coisas dá.
        //
        // Os dois são `Option` e não um `max` incondicional sobre o cursor: a
        // meio do laço `child_y` é o CURSOR e não uma aresta de borda, e
        // compará-los é tomar o maior de duas coisas diferentes — com uma
        // margem negativa isso devolvia o bloco para baixo em vez de o puxar
        // para cima.
        let mut clearance: Option<f32> = None;
        // O cursor DEPOIS de o `clear` descarregar o fluxo inline e ANTES de o
        // descer: é essa a base da posição hipotética. Guardá-lo antes do
        // `flush_inline!` perdia a altura do texto acabado de descarregar, e o
        // teste de equivalência do cache foi quem o disse — o mesmo bloco caía
        // uma linha acima quando refeito do zero.
        let mut y_sem_clearance: Option<f32> = None;
        // `clear` — o par do `float`: este filho começa ABAIXO dos floats
        // correntes. Fica ANTES do dispatch por tipo de caixa porque vale para
        // qualquer um deles: o caminho de bloco já fechava a linha de floats
        // sempre, mas um inline-block ou um texto com `clear` não fechava nada e
        // acabava por cima do float. Os três valores agem como `both` (ver
        // `style::text::Clear` para porquê).
        if child_css
            .as_ref()
            .and_then(|c| c.clear)
            .map(|c| c.clears())
            .unwrap_or(false)
        {
            flush_inline!(child_y);
            clearance = fundo_dos_floats(&floats);
            y_sem_clearance = Some(child_y);
            close_floats!(child_y);
        }
        let (child_block, child_inline_block) = match &dom.node(child).kind {
            NodeKind::Element { tag } => {
                let replaced = (tag == "img" && dom.image_of(child).is_some())
                    || tag == "svg"
                    || tag == "canvas";
                let effective = child_css.as_ref().and_then(|c| c.effective_display());
                // "é de bloco?" e NÃO "não é inline?" — e o `InlineBlock` é o
                // valor que as duas leituras separam. Por `d != Inline` um
                // `display:inline-block` contava como bloco: o elemento saía do
                // fluxo da linha, empilhava-se em vez de fluir e tomava a largura
                // do contentor. Um `<span style="display:inline-block">` entre
                // duas palavras descia para a linha seguinte, e a caixa que o
                // browser põe ao lado do texto ficava sozinha numa linha só.
                //
                // Esta é a QUINTA aparição da mesma pergunta mal posta, e as
                // outras quatro estão em `is_block_level`, `is_inline_block` e
                // duas decisões de fluxo. A causa é esta cópia: o laço reescreve
                // à mão o que `is_inline_block` já responde, em vez de lhe
                // perguntar. Substituir a cópia pela chamada é a correção de
                // fundo e muda mais do que o inline-block — fica para um lote
                // próprio, medido à parte, para que o efeito seja atribuível.
                let explicit_block = effective
                    .map(|d| {
                        d != crate::style::DisplayKind::Inline
                            && d != crate::style::DisplayKind::InlineBlock
                    })
                    .unwrap_or(false);
                // `display:inline` DECLARADO vence a tag e a UA-stylesheet: um
                // `<h3 style="display:inline">` — a forma dos cabeçalhos
                // colapsáveis do MediaWiki — é conteúdo de linha e mede o seu
                // texto, não os 752px do contentor. `effective.is_some()`
                // respondia "há display declarado", não "é de bloco", e por ela
                // entrava também o inline.
                // A pergunta que resta é `cria_caixa_apesar_de_inline` e não
                // `has_box()`: esta última conta a margem e a `height` que o
                // próprio `display:inline` torna inoperantes, e devolvia o
                // elemento ao caminho de bloco de onde a declaração o tirou.
                let inline_declarado = effective == Some(crate::style::DisplayKind::Inline);
                let block = if inline_declarado {
                    replaced
                        || child_css
                            .as_ref()
                            .map(|c| crate::inline_box::cria_caixa_apesar_de_inline(c))
                            .unwrap_or(false)
                } else {
                    replaced
                        || effective.is_some()
                        || crate::block::lookup(tag).is_some()
                        || child_css
                            .as_ref()
                            .map(|c| c.has_box() || c.height.is_some())
                            .unwrap_or(false)
                };
                let inline_block =
                    // Um `display:inline-block` DECLARADO responde antes da TAG:
                    // `.mw-list-item{display:inline-block}` sobre um `<li>` batia
                    // no `block::lookup("li")` e voltava ao caminho de bloco, com
                    // os itens do menu empilhados e cada um com a largura do
                    // contentor. São 27 dos 55 inline-blocks desta página.
                    if effective == Some(crate::style::DisplayKind::InlineBlock) {
                        true
                    } else if matches!(tag.as_str(), "input" | "button" | "select" | "textarea") {
                        !explicit_block
                    } else if crate::block::lookup(tag).is_some() || explicit_block {
                        false
                    } else {
                        child_css
                            .as_ref()
                            .map(|c| c.has_box() || c.height.is_some())
                            .unwrap_or(false)
                    };
                (block, inline_block)
            }
            _ => (false, false),
        };
        match &dom.node(child).kind {
            // Metadata não-renderável (`<head>`/`<title>`/`<style>`/`<script>`):
            // pula — NÃO coleta seu texto como inline (senão o título e o CSS cru
            // vazam pra tela). Checado ANTES do caminho inline.
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            // Fora do fluxo (`position:absolute/fixed`): não ocupa espaço aqui —
            // pintado na passada out-of-flow de layout_document.
            NodeKind::Element { .. } if child_out => {}
            // FLOAT left/right: encosta ao lado pedido, na primeira faixa a
            // partir do cursor onde CAIBA ao lado dos floats já postos.
            NodeKind::Element { .. } if child_float != crate::style::FloatSide::None => {
                flush_inline!(child_y);
                let side = child_float;
                let w = child_outer_width(dom, child, content_w, font_size, ctx);
                let h = child_outer_height(dom, child, content_w, avail_h, css, font_size, ctx);
                // Onde cabe: tenta o cursor; se a banda livre aí é estreita
                // demais, desce para o fundo de cada float que a estorva, pela
                // ordem em que eles acabam. Dois floats do mesmo lado que cabem
                // lado a lado continuam lado a lado — é o header brand+nav do
                // Bootstrap, e é o que a primeira tentativa já responde.
                let mut top = child_y;
                let mut fundos: Vec<f32> = floats.iter().map(|e| e.bottom).collect();
                fundos.sort_by(f32::total_cmp);
                let (mut bx, mut bw) = banda_livre(&floats, top, h, content_x, content_w);
                for f in fundos {
                    if bw >= w || f <= top {
                        continue;
                    }
                    top = f;
                    (bx, bw) = banda_livre(&floats, top, h, content_x, content_w);
                }
                let x = if side == crate::style::FloatSide::Left {
                    bx
                } else {
                    bx + bw - w
                };
                layout_block(
                    dom,
                    child,
                    x,
                    top,
                    content_w,
                    avail_h,
                    None,
                    None,
                    true,
                    &[],
                    ctx,
                    list,
                );
                floats.push(Exclusao {
                    top,
                    bottom: top + h,
                    side,
                    edge: if side == crate::style::FloatSide::Left {
                        x + w
                    } else {
                        x
                    },
                });
                prev_margin = 0.0; // float quebra a sequência de collapse
            }
            NodeKind::Element { .. } if child_block && !child_inline_block => {
                flush_inline!(child_y);
                // Sem `close_floats!`: pelo CSS a caixa de bloco ao lado de um
                // float NÃO desce nem encolhe — mantém a largura e sobrepõe-se
                // ao float; quem encolhe são as linhas lá dentro. Ver
                // [`Exclusao`] para os números do Chrome que o fixam.
                // margin VERTICAL TOP do filho (para o collapse com o anterior):
                // margin.top + margin_v da UA.
                // As DUAS margens verticais do filho. A de baixo entrou aqui
                // porque o colapso entre irmãos compara a margem de BAIXO do
                // anterior com a de CIMA do seguinte (CSS 2.1 §8.3.1) e este
                // laço guardava a de cima — com as margens assimétricas da
                // UA-stylesheet (`h2`, `h3`, `ul`) o excesso descontado era
                // sempre o errado. Medido num Chrome real: dois irmãos com
                // `margin-bottom:30` e `margin-top:10` ficavam a 40 de
                // intervalo onde o browser dá 30.
                //
                // O `margin_v` da UA só vale no lado que o AUTOR não declarou,
                // lado a lado — é a mesma regra de `layout_block`, e escrevê-la
                // aqui outra vez é uma cópia que um lote futuro deve juntar.
                let (m, m_baixo) = child_css
                    .as_ref()
                    .map(|c| {
                        // unidades relativas resolvem contra o content deste
                        // container.
                        let r = ResolveCtx {
                            parent_content_w: content_w,
                            node_font_size: font_px(&c, font_size),
                            root_font_size: crate::style::root_font_size(),
                            viewport_w: ctx.viewport_w,
                            viewport_h: ctx.viewport_h,
                        };
                        let mv = c.margin_v.unwrap_or(0.0);
                        let mv_topo = if c.margin.top == crate::style::Side::Unset {
                            mv
                        } else {
                            0.0
                        };
                        let mv_baixo = if c.margin.bottom == crate::style::Side::Unset {
                            mv
                        } else {
                            0.0
                        };
                        (
                            c.margin.top.resolve(&r).unwrap_or(0.0) + mv_topo,
                            c.margin.bottom.resolve(&r).unwrap_or(0.0) + mv_baixo,
                        )
                    })
                    .unwrap_or((0.0, 0.0));
                // A aresta de borda que este bloco teria sem floats: o cursor de
                // antes da descida, menos o excesso colapsado, mais a própria
                // margem de topo.
                let base = y_sem_clearance.unwrap_or(child_y);
                let mut aresta = base - excesso_de_margens(prev_margin, m) + m;
                // CLEARANCE (CSS 2.1 §9.5.2): com `clear`, a aresta fica no
                // MAIOR entre a hipotética e o fundo do float. Somar a margem
                // por cima da descida era o defeito medido — o bloco ficava
                // 10 px abaixo do fundo do float onde o Chrome o põe
                // exactamente no fundo. Note-se que a auditoria previa o erro
                // no sentido CONTRÁRIO.
                if let Some(fundo) = clearance {
                    aresta = aresta.max(fundo);
                }
                child_y = aresta - m;
                let ((_, h), _) = layout_block_reusing(
                    dom,
                    child,
                    content_x,
                    child_y,
                    content_w,
                    avail_h,
                    || (m, m_baixo),
                    &floats,
                    ctx,
                    list,
                );
                child_y += h;
                prev_margin = m_baixo;
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
            NodeKind::Element { .. }
                if child_inline_block && em_contexto_inline(dom, id, child) =>
            {
                flush_ib!(child_y);
                inline_group.push(child);
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
                        css,
                        font_size,
                        &floats,
                        ctx,
                        list,
                    );
                    inline_group.clear();
                }
                ib_run.push(child);
                prev_margin = 0.0;
            }
            // Whitespace estrutural continua no DOM, mas não cria uma linha entre
            // blocos/floats. Quando está perto de texto/inline, entra no grupo e o
            // `wrap_runs` o colapsa como um espaço normal.
            NodeKind::Text(t)
                if t.trim().is_empty() && !whitespace_is_inline_separator(dom, id, child) => {}
            // Texto / elemento inline: entra no CONTEXTO INLINE corrente — flui
            // com os irmãos inline adjacentes (o flush pinta o grupo inteiro).
            _ => {
                flush_ib!(child_y); // fecha a corrida de inline-blocks
                inline_group.push(child);
            }
        }
    }
    // descarrega o fluxo inline pendente e cresce para conter os floats DESTE
    // container. ⚠️ DIVERGÊNCIA CONHECIDA, não é um bug à espera de correção:
    // pelo CSS um float só faz o pai crescer se o pai for um BFC (`overflow`,
    // `flow-root`, flex, tabela) ou houver clearfix; aqui cresce sempre. Foi
    // decidido manter — mexer nisso muda a altura de TODO o contentor com float
    // e é um segundo eixo de regressão por cima deste lote. O BFC é outro lote,
    // com medição própria. Ver `float_left_right_dividem_a_linha`.
    flush_inline!(child_y);
    if let Some(fundo) = fundo_dos_floats(&floats[proprios..]) {
        child_y = child_y.max(fundo);
    }
    (child_y - content_y).max(0.0)
}
