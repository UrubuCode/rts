//! `bounding_component`/`bounding_components_many` deixaram de rodar um
//! `layout_document` fresco por chamada (lote BR, 2026-09-16): passaram a
//! reusar `layout::layout_cached`, o mesmo memo de UM slot que já serve o
//! backend com janela a cada frame.
//!
//! Os dois testes aqui pinam as duas metades da mudança: que a resposta agora
//! SAI desse memo (e não apenas de uma passada de layout descartada ao fim da
//! chamada), e que a chave desse memo continua a ver uma mudança que vem de um
//! IRMÃO do nó consultado — o buraco que `box-tree.md` §7 (I7) documenta para
//! os caches POR NÓ usados dentro de uma passada de layout (`FragmentKey`,
//! `node_epoch`), e que este cache não tem porque nunca estreita a chave a um
//! nó: é um contador por DOCUMENTO.

use super::*;

/// Sem a mudança, `bounding_component` chamava `bounding_rect`/`layout_document`
/// direto e nunca tocava `display_cache` — este teste falha nesse código
/// porque o slot fica `None` depois da chamada.
#[test]
fn bounding_component_preenche_o_cache_de_display_list() {
    let dom = parse_html_to_dom("<div id='a' style='width:100px;height:20px'></div>");
    assert!(
        dom.display_cache.borrow().is_none(),
        "pré-condição: nada foi pedido ainda"
    );
    let id = dom.query("#a").unwrap();
    let _ = dom.bounding_component(id, 2);
    assert!(
        dom.display_cache.borrow().is_some(),
        "bounding_component tem de passar pelo mesmo memo que layout_cached usa"
    );
}

/// A mesma prova para a via em lote: uma única `bounding_components_many`
/// também tem de deixar o memo preenchido, não só a via singular.
#[test]
fn bounding_components_many_preenche_o_cache_de_display_list() {
    let dom = parse_html_to_dom(
        "<div id='a' style='width:10px'></div><div id='b' style='width:20px'></div>",
    );
    assert!(dom.display_cache.borrow().is_none());
    let ids: Vec<NodeId> = vec![dom.query("#a").unwrap(), dom.query("#b").unwrap()];
    let _ = dom.bounding_components_many(&ids);
    assert!(dom.display_cache.borrow().is_some());
}

/// A mudança-chave: mutar um IRMÃO ANTERIOR de quem está a ser medido tem de
/// mudar a resposta na consulta seguinte, mesmo com o cache no meio.
///
/// `revision` é um contador por DOCUMENTO — bumpado por `touch`,
/// `touch_subtree`, `touch_structural` e `touch_attr` para QUALQUER nó, não só
/// para o nó consultado — então a chave do cache nunca fica cega para esta
/// mudança. Uma chave mais estreita (por exemplo, um epoch do PRÓPRIO `alvo`)
/// serviria aqui a resposta da frame anterior com o processo a sair a zero,
/// que é exactamente a armadilha que `box-tree.md` §7 nomeia para o caso do
/// irmão.
#[test]
fn bounding_component_ve_mudanca_de_irmao_anterior() {
    let mut dom = parse_html_to_dom(
        "<div id='antes' style='height:10px'>a</div><div id='alvo' style='height:10px'>b</div>",
    );
    let antes = dom.query("#antes").unwrap();
    let alvo = dom.query("#alvo").unwrap();

    let y0 = dom.bounding_component(alvo, 1);

    // Aumenta a altura do irmão ANTERIOR — em fluxo normal de bloco isso
    // empurra `alvo` para baixo. Nada muda no próprio `alvo`: nem estilo, nem
    // conteúdo, nem atributo dele.
    dom.set_attr(antes, "style", "height:200px");
    let y1 = dom.bounding_component(alvo, 1);

    assert_ne!(
        y0, y1,
        "o cache tem de invalidar mesmo com a mudança vinda do irmão, não do próprio nó"
    );
    assert!(
        y1 > y0,
        "e o deslocamento tem de ir na direção certa: alvo desce quando o irmão de cima cresce"
    );
}

/// A mesma prova para a via em lote — `bounding_components_many` não pode
/// responder o `alvo` velho só porque foi o `antes` que mudou.
#[test]
fn bounding_components_many_ve_mudanca_de_irmao_anterior() {
    let mut dom = parse_html_to_dom(
        "<div id='antes' style='height:10px'>a</div><div id='alvo' style='height:10px'>b</div>",
    );
    let antes = dom.query("#antes").unwrap();
    let alvo = dom.query("#alvo").unwrap();
    let ids: Vec<NodeId> = vec![alvo];

    let antes0 = dom.bounding_components_many(&ids);
    dom.set_attr(antes, "style", "height:200px");
    let depois0 = dom.bounding_components_many(&ids);

    assert_ne!(
        antes0[1], depois0[1],
        "y do alvo tem de mudar quando o irmão de cima cresce, mesmo em lote"
    );
}

/// Repetir a MESMA consulta sem mutar entre as duas chamadas devolve o mesmo
/// número — a metade "não regride" da mudança: cachear não pode fazer duas
/// chamadas idênticas divergirem uma da outra.
#[test]
fn bounding_component_repetido_sem_mutacao_da_a_mesma_resposta() {
    let dom = parse_html_to_dom("<div id='a' style='height:37px'></div>");
    let id = dom.query("#a").unwrap();
    let primeira = dom.bounding_component(id, 3);
    let segunda = dom.bounding_component(id, 3);
    assert_eq!(primeira, segunda);
}

/// The read loop builds ONE geometry, not one per call (PQ-C4). The memo left
/// `DisplayList` for `Dom::geometry_cached`; were `bounding_component` to read
/// `rect_of`, it would run `geometry_now` on every call — the per-call cost of
/// the 13.7 ms class, one layer below the layout. Pinned by identity: the
/// geometry held in the slot after the first call is the same allocation after
/// a thousand more, and so is the list (one layout).
#[test]
fn bounding_component_read_loop_builds_one_geometry() {
    let dom = parse_html_to_dom("<div id='a' style='width:100px;height:20px'></div><div>texto</div>");
    let id = dom.query("#a").unwrap();
    let slot = |dom: &Dom| {
        let cache = dom.display_cache.borrow();
        let (_, list, geometry) = cache.as_ref().expect("the slot is filled");
        let geometry = geometry.get().expect("bounding_component built the geometry through the memo");
        (std::rc::Rc::clone(list), std::rc::Rc::clone(geometry))
    };
    let expected = [
        dom.bounding_component(id, 0),
        dom.bounding_component(id, 1),
        dom.bounding_component(id, 2),
        dom.bounding_component(id, 3),
    ];
    assert_eq!(expected[2..], [100.0, 20.0]);
    let (list, geometry) = slot(&dom);
    for which in 0..1000_i64 {
        assert_eq!(dom.bounding_component(id, which % 4), expected[(which % 4) as usize]);
    }
    let (list_after, geometry_after) = slot(&dom);
    assert!(std::rc::Rc::ptr_eq(&list, &list_after), "one layout for the whole loop");
    assert!(std::rc::Rc::ptr_eq(&geometry, &geometry_after), "one geometry for the whole loop");
}
