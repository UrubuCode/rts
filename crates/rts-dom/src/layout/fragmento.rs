//! FRAGMENTOS: o desenho de uma subárvore guardado em coordenadas relativas, a
//! chave que o valida, e a costura que o reinsere numa lista sem o recalcular.
//!
//! É o que torna o layout incremental: mudar uma folha invalida o epoch dela e
//! dos ancestrais, e todo irmão intacto reusa o fragmento em vez de refazer
//! cascade, medição de texto e box model.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;
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
#[allow(clippy::too_many_arguments)]
/// A chave do fragmento de um nó com certas constraints. Extraída porque o laço
/// do fluxo vertical CONSULTA o cache antes de classificar o filho: um fragmento
/// só existe para bloco-normal, então encontrá-lo já responde o que a
/// classificação responderia — e a classificação custa estilo computado,
/// `block::lookup` e a margem resolvida, mil vezes por frame.
fn fragment_key(
    dom: &Dom,
    id: NodeIdx,
    avail_w: f32,
    avail_h: Option<f32>,
    ctx: &LayoutCtx,
) -> crate::dom::FragmentKey {
    KeyBase::new(dom, avail_w, avail_h, ctx).key(dom, id)
}

/// A parte da chave de fragmento que NÃO varia entre os filhos de um container:
/// identidade da árvore, epochs globais, viewport, medidor e as constraints.
///
/// Montar a chave inteira por filho relia um `thread_local` (o epoch de estilo)
/// e refazia as conversões mil vezes por container — o laço do fluxo vertical
/// pergunta o mesmo a cada iteração e só o nó muda.
#[derive(Clone, Copy)]
pub(in crate::layout) struct KeyBase {
    tree: u64,
    style_epoch: u64,
    anim_epoch: u64,
    avail_w: u32,
    avail_h: Option<u32>,
    viewport_w: u32,
    viewport_h: u32,
    measurer: u64,
}

impl KeyBase {
    pub(in crate::layout) fn new(dom: &Dom, avail_w: f32, avail_h: Option<f32>, ctx: &LayoutCtx) -> KeyBase {
        KeyBase {
            tree: dom.cache_identity(),
            style_epoch: crate::style::props::style_epoch(),
            anim_epoch: dom.anim_epoch(),
            avail_w: avail_w.to_bits(),
            avail_h: avail_h.map(f32::to_bits),
            viewport_w: ctx.viewport_w.to_bits(),
            viewport_h: ctx.viewport_h.to_bits(),
            measurer: ctx.measurer.identity(),
        }
    }

    pub(in crate::layout) fn key(&self, dom: &Dom, id: NodeIdx) -> crate::dom::FragmentKey {
        crate::dom::FragmentKey {
            tree: self.tree,
            node_epoch: dom.layout_epoch(id),
            style_epoch: self.style_epoch,
            anim_epoch: self.anim_epoch,
            node: id,
            avail_w: self.avail_w,
            avail_h: self.avail_h,
            viewport_w: self.viewport_w,
            viewport_h: self.viewport_h,
            measurer: self.measurer,
        }
    }
}

/// Insere um item numa posição, corrigindo o ponto de entrada das SUBÁRVORES.
///
/// O box model emite os filhos primeiro e insere o fundo e a borda atrás deles;
/// as subárvores reusadas guardam o índice antes do qual entram, e sem esta
/// correção elas passariam a ser pintadas na frente do próprio fundo. Foi o que
/// um teste de altura percentual acusou, ao ver os retângulos na ordem trocada.
/// Insere um item em `at` e corrige o `at` das subárvores que ficam depois dele.
///
/// `filhos_antes` é quantas subárvores já existiam quando `at` foi RESERVADO, e
/// é o que distingue "esta subárvore é minha, empurra-a" de "esta subárvore já
/// cá estava, não lhe toques". Sem essa fronteira, um `at >= at` sozinho não
/// consegue separar os dois casos quando os índices coincidem — e coincidem
/// exatamente no caso que interessa: um `position:fixed`, pintado no fim do
/// documento, reserva o índice 0 da lista de topo, que é também o `at` do
/// fragmento onde vive a página inteira. O fixed empurrava a página para
/// depois de si e ficava ATRÁS dela; numa página real é o dropdown a
/// desaparecer por trás do conteúdo.
///
/// É a mesma distinção que o `BeginClip { filhos_antes }` já fazia, e pela mesma
/// razão: o índice sozinho não carrega a ordem de criação.
pub(crate) fn insert_item(
    list: &mut DisplayList,
    at: usize,
    filhos_antes: usize,
    item: DisplayItem,
) {
    list.items.insert(at, item);
    for child in list.children.iter_mut().skip(filhos_antes) {
        if child.at >= at {
            child.at += 1;
        }
    }
}

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
    let (antiga, anterior) = dom.last_fragment_of(id)?;
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
    if anterior.children.is_empty() {
        return None;
    }
    let sujos = dom.dirty_children_of(id)?;
    // A SEQUÊNCIA de filhos precisa ser a mesma, não só o tamanho: inserção,
    // remoção e reordenação mudam quem desenha o quê, e trocar uma referência
    // não daria conta. Comparar índice a índice é uma passada de leitura.
    if !mesma_sequencia_de_filhos(dom, id, &anterior.children) {
        return None;
    }
    let _phase = crate::metrics::phases::scope("fragment-patch");

    let mut children = anterior.children.clone();
    let mut trocou = false;
    for child in &mut children {
        if !sujos.contains(&child.node) {
            continue;
        }
        let mut own = DisplayList::default();
        // Onde o filho FOI POSTO: a origem em que o fragmento dele foi calculado
        // mais o deslocamento com que entrou aqui. Somar à origem do PAI daria
        // uma posição sem sentido — foi o que o teste de equivalência mostrou,
        // com o texto reaparecendo em (0,16) em vez de (12, 67.4).
        let origem = (
            child.fragment.origin.0 + child.dx,
            child.fragment.origin.1 + child.dy,
        );
        let margem = child.margin_top;
        let ((_, altura), nova_margem) = layout_block_reusing(
            dom,
            child.node,
            origem.0,
            origem.1,
            child.avail_w,
            child.avail_h,
            || margem,
            // A costura só alcança o que virou fragmento, e um bloco estorvado
            // por float nunca vira (ver o guard em `layout_block_reusing`).
            &[],
            ctx,
            &mut own,
        );
        if (altura - child.height).abs() > 0.001 || (nova_margem - child.margin_top).abs() > 0.001 {
            return None;
        }
        // O `layout_block_reusing` emitiu numa lista própria; o que interessa é a
        // referência que ele acabou de registrar para este nó.
        let novo = own.children.first()?.fragment.clone();
        child.fragment = novo;
        trocou = true;
    }
    if !trocou {
        return None;
    }
    let fragment = std::rc::Rc::new(Fragment {
        node: id,
        // Compartilha o que NÃO mudou — só a lista de subárvores é nova.
        items: std::rc::Rc::clone(&anterior.items),
        children,
        rects: std::rc::Rc::clone(&anterior.rects),
        hit_order: std::rc::Rc::clone(&anterior.hit_order),
        scroll_regions: anterior.scroll_regions.clone(),
        origin: anterior.origin,
        size: anterior.size,
        margin_top: anterior.margin_top,
    });
    dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    Some(fragment)
}

/// `true` se os filhos-elemento do nó são exatamente os que o desenho anterior
/// referencia, na mesma ordem. Uma passada de leitura; o que não é barato é o
/// layout deles.
fn mesma_sequencia_de_filhos(dom: &Dom, id: NodeIdx, children: &[ChildRef]) -> bool {
    let mut esperados = children.iter().map(|c| c.node);
    let mut atuais = dom
        .node(id)
        .children
        .iter()
        .copied()
        .filter(|&c| matches!(dom.node(c).kind, NodeKind::Element { .. }));
    loop {
        match (esperados.next(), atuais.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) if a == b => continue,
            _ => return false,
        }
    }
}

pub(in crate::layout) fn emit_fragment(
    fragment: &std::rc::Rc<Fragment>,
    list: &mut DisplayList,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) {
    let _phase = crate::metrics::phases::scope("fragment-emit");
    fragment.emit_at(list, x, y, avail_w, avail_h);
}

pub(in crate::layout) fn layout_block_reusing(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    margem_de_topo: impl FnOnce() -> f32,
    exclusoes: &[Exclusao],
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> ((f32, f32), f32) {
    // Um bloco ESTORVADO por um float não entra no cache de fragmentos, nem sai
    // dele: a chave é feita das constraints (largura, altura, viewport) e a
    // banda livre não é nenhuma delas. Sem esta recusa, o parágrafo ao lado da
    // figura seria servido pela versão de largura cheia guardada antes — e o
    // contrário também, a versão estreita reusada longe do float. Acrescentar a
    // banda à chave era a outra saída; recusar custa só nos blocos que têm
    // float ao lado, que são poucos, e não põe um campo novo em todas as
    // chaves da página.
    if !exclusoes.is_empty() {
        let size = layout_block(
            dom, id, x, y, avail_w, avail_h, None, None, false, exclusoes, ctx, list,
        );
        return (size, margem_de_topo());
    }
    let key = fragment_key(dom, id, avail_w, avail_h, ctx);
    if let Some(fragment) = dom.fragment_get(key) {
        crate::bump!(fragment_hits);
        emit_fragment(&fragment, list, x, y, avail_w, avail_h);
        return (fragment.size, fragment.margin_top);
    }
    // COSTURA: trocar no desenho anterior só a subárvore que ficou suja. Agora
    // que a saída é uma ÁRVORE, costurar é substituir uma REFERÊNCIA num vetor
    // de mil entradas de 48 bytes — a primeira versão disto (revertida) copiava
    // 3000 itens com String e por isso não ganhava nada.
    if let Some(fragment) = costurar(dom, id, key, ctx) {
        crate::bump!(fragment_patches);
        emit_fragment(&fragment, list, x, y, avail_w, avail_h);
        return (fragment.size, fragment.margin_top);
    }
    crate::bump!(fragment_misses);
    let _phase = crate::metrics::phases::scope("fragment-build");
    // Lista PRÓPRIA: o fragmento precisa saber exatamente quais itens são dele,
    // e a única forma de saber isso é não misturá-los com os dos irmãos.
    let mut own = DisplayList::default();
    let size = layout_block(
        dom,
        id,
        x,
        y,
        avail_w,
        avail_h,
        None,
        None,
        false,
        &[],
        ctx,
        &mut own,
    );
    let fragment = std::rc::Rc::new(Fragment {
        node: id,
        rects: std::rc::Rc::new(
            own.node_rects
                .iter()
                .map(|(idx, rect)| (*idx, *rect))
                .collect(),
        ),
        hit_order: std::rc::Rc::new(std::mem::take(&mut own.hit_order)),
        scroll_regions: std::mem::take(&mut own.scroll_regions),
        items: std::rc::Rc::new(std::mem::take(&mut own.items)),
        children: std::mem::take(&mut own.children),
        origin: (x, y),
        size,
        margin_top: margem_de_topo(),
    });
    dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    fragment.emit_at(list, x, y, avail_w, avail_h);
    (fragment.size, fragment.margin_top)
}

/// Uma subárvore emitida por referência dentro de uma lista ou de outro
/// fragmento.
#[derive(Clone, Debug)]
pub struct ChildRef {
    /// O nó que esta subárvore desenha — a costura precisa saber quem é.
    pub node: NodeIdx,
    /// Altura externa que ele ocupou e a margem de topo resolvida: se qualquer
    /// uma mudar ao refazê-lo, tudo abaixo desloca e a costura não serve.
    pub height: f32,
    pub margin_top: f32,
    /// As CONSTRAINTS com que ele foi layoutado — as do CONTEÚDO do pai, não as
    /// do pai. Refazer um filho com a largura do container em vez da do conteúdo
    /// dá uma caixa larga demais pela soma do padding e da margem.
    pub avail_w: f32,
    pub avail_h: Option<f32>,
    /// Posição em `items` ANTES da qual esta subárvore é pintada.
    pub at: usize,
    /// Posição em `hit_order` antes da qual a ordem de hit-test dela entra.
    ///
    /// Separada do `at` porque as duas sequências crescem por motivos
    /// diferentes: nem todo item de pintura registra um nó, e nem todo nó
    /// registrado pinta um item. Montar a ordem de hit-test com os próprios
    /// primeiro e os das subárvores depois inverte o z-order — foi o que o teste
    /// de `z-index` acusou.
    pub hit_at: usize,
    pub fragment: std::rc::Rc<Fragment>,
    pub dx: f32,
    pub dy: f32,
}

impl PartialEq for ChildRef {
    /// Compara CONTEÚDO — duas listas equivalentes podem ter chegado ao mesmo
    /// desenho por caminhos diferentes.
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
            && self.dx == other.dx
            && self.dy == other.dy
            && self.fragment.items == other.fragment.items
            && self.fragment.children == other.fragment.children
    }
}

/// O DESENHO de uma subárvore posta com certas constraints, guardado para ser
/// reusado numa posição diferente.
///
/// Coordenadas ABSOLUTAS, como saíram do layout: a origem em que foi calculado
/// fica registrada em `origin`, e reusar é somar a diferença. Guardar já
/// relativo daria na mesma e custaria uma passada extra na hora de gravar — o
/// caso comum é justamente reusar na MESMA posição (nada acima dele mudou de
/// altura), e aí a soma é zero e nem se percorre.
#[derive(Clone, Debug, Default)]
pub struct Fragment {
    /// O nó que este fragmento desenha.
    pub node: NodeIdx,
    /// Itens de pintura PRÓPRIOS desta subárvore.
    ///
    /// Os três vetores grandes são `Rc`: quando um container é COSTURADO, só a
    /// lista de subárvores muda, e clonar retângulos e ordem de hit-test de um
    /// container de mil filhos custaria mais do que a costura economiza.
    pub items: std::rc::Rc<Vec<DisplayItem>>,
    /// As subárvores que ela reusou, por referência — o desenho é uma árvore.
    pub children: Vec<ChildRef>,
    /// Geometria por nó (o que alimenta `getBoundingClientRect`).
    pub rects: std::rc::Rc<Vec<(NodeIdx, Rect)>>,
    /// Ordem de pintura para o hit-test (ancestral antes de descendente).
    pub hit_order: std::rc::Rc<Vec<NodeIdx>>,
    /// Regiões roláveis internas descobertas dentro da subárvore.
    pub scroll_regions: Vec<ScrollRegion>,
    /// Onde este fragmento foi calculado.
    pub origin: (f32, f32),
    /// Tamanho externo devolvido pelo `layout_block` (o que o chamador usa para
    /// avançar o cursor).
    pub size: (f32, f32),
    /// A MARGEM DE TOPO resolvida deste bloco, para o colapso com o irmão
    /// anterior.
    ///
    /// Guardada junto porque o laço a calculava ANTES de descobrir que o
    /// fragmento servia: resolver a margem pede o estilo computado, o
    /// `font-size` do contexto e um `ResolveCtx` — por filho, mil vezes por
    /// frame, para um valor que não muda enquanto o epoch do nó não muda.
    pub margin_top: f32,
}

impl Fragment {
    /// Emite este fragmento numa `DisplayList`, deslocado para `(x, y)`.
    pub fn emit_at(
        self: &std::rc::Rc<Self>,
        list: &mut DisplayList,
        x: f32,
        y: f32,
        avail_w: f32,
        avail_h: Option<f32>,
    ) {
        let (dx, dy) = (x - self.origin.0, y - self.origin.1);
        // APONTA, não copia: os itens desta subárvore já existem e não mudaram.
        // Os RETÂNGULOS abaixo continuam sendo materializados, porque a consulta
        // de geometria é por nó e precisa do valor pronto — são 16 bytes contra
        // os 48 de um item, numa quantidade menor.
        list.children.push(ChildRef {
            node: self.node,
            height: self.size.1,
            margin_top: self.margin_top,
            avail_w,
            avail_h,
            at: list.items.len(),
            hit_at: list.hit_order.len(),
            fragment: std::rc::Rc::clone(self),
            dx,
            dy,
        });
        // A GEOMETRIA da subárvore (retângulos, ordem de hit-test, regiões
        // roláveis) também fica na referência: materializá-la aqui era metade do
        // custo de um frame parado — três inserções em mapa por fragmento, mil
        // fragmentos. Quem precisa dela chama `geometry()`, que percorre a
        // árvore uma vez e guarda o resultado.
    }
}

impl Fragment {
    /// Quantos itens este fragmento pinta, contando as subárvores que ele reusa.
    pub fn total_items(&self) -> usize {
        self.items.len()
            + self
                .children
                .iter()
                .map(|c| c.fragment.total_items())
                .sum::<usize>()
    }
}
