//! Ciclo de vida do nó (PLAN §4.M): a freelist recicla um `idx` desanexado
//! sem crescer a arena, e um `NodeId` do ocupante anterior desse `idx` deixa
//! de resolver depois — é a régua que o lote promete: "inserir e remover N
//! vezes não faz a arena crescer além do pico vivo".

use super::*;

/// Insere e remove 100 000 elementos folha, um a um, chamando
/// `release_subtree` a cada remoção (o caminho que a fachada TS chama
/// quando não há wrapper vivo). A arena não pode crescer com N — o pico
/// vivo é sempre a árvore base mais o elemento do momento.
#[test]
fn insercao_e_remocao_em_massa_nao_cresce_a_arena() {
    let mut dom = parse_html_to_dom("<div id='raiz'></div>");
    let raiz = dom.query("#raiz").unwrap();
    let pico = dom.nodes.len();

    for _ in 0..100_000 {
        let el = dom.create_element("li");
        dom.append_child(raiz, el);
        dom.remove_node(el);
        dom.release_subtree(el);
    }

    // Folga generosa (2×) contra o pico medido ANTES do loop — não um número
    // mágico: prova que a arena não cresce COM N, não que ela nunca varia.
    assert!(
        dom.nodes.len() <= pico * 2 + 8,
        "arena cresceu sem limite: pico inicial {pico}, agora {}",
        dom.nodes.len()
    );
}

/// Um `NodeId` do nó reciclado resolve a `None`; o novo ocupante do mesmo
/// `idx` cru tem outra geração — a garantia central do lote: a geração é
/// POR NÓ, não por árvore, e `resolve` distingue os dois sem ambiguidade.
#[test]
fn id_do_no_reciclado_deixa_de_resolver_e_o_novo_ocupante_tem_outra_geracao() {
    let mut dom = parse_html_to_dom("<div id='raiz'></div>");
    let raiz = dom.query("#raiz").unwrap();

    let velho_id = dom.create_element("li");
    let velho_idx = dom.resolve(velho_id).unwrap();
    dom.append_child(raiz, velho_id);
    dom.remove_node(velho_id);
    dom.release_subtree(velho_id);

    assert_eq!(
        dom.resolve(velho_id),
        None,
        "NodeId reciclado ainda resolve — a geração não mudou"
    );

    // O PRÓXIMO create_element reusa o slot livre (LIFO da freelist) — mesmo
    // `idx` cru, geração diferente.
    let novo_id = dom.create_element("li");
    let novo_idx = dom.resolve(novo_id).unwrap();
    assert_eq!(
        novo_idx, velho_idx,
        "este teste pina o reuso do MESMO idx cru; se a freelist mudar de \
         política (não-LIFO), reescrever para achar o idx reciclado em vez \
         de assumir que é o próximo"
    );
    assert_ne!(
        novo_id.generation, velho_id.generation,
        "o novo ocupante do idx reciclado tem a MESMA geração do antigo"
    );
    assert_eq!(dom.resolve(novo_id), Some(novo_idx));
}

/// Reciclar não deixa o estado por-nó do ocupante ANTERIOR vazar para o
/// próximo — sem isto, o novo elemento herdaria o valor de input de quem
/// ocupou o `idx` antes dele.
#[test]
fn reciclagem_purga_estado_lateral_por_no() {
    let mut dom = parse_html_to_dom("<div id='raiz'></div>");
    let raiz = dom.query("#raiz").unwrap();

    let velho_id = dom.create_element("input");
    dom.append_child(raiz, velho_id);
    let velho_idx = dom.resolve(velho_id).unwrap();
    dom.set_input_value(velho_idx, "segredo");
    dom.remove_node(velho_id);
    dom.release_subtree(velho_id);

    let novo_id = dom.create_element("input");
    let novo_idx = dom.resolve(novo_id).unwrap();
    assert_eq!(novo_idx, velho_idx, "pina o reuso do mesmo idx — ver o teste acima");
    assert_eq!(
        dom.input_value(novo_idx),
        "",
        "o novo <input> no idx reciclado herdou o valor do antigo"
    );
}

/// `release_subtree` sobre um nó AINDA anexado não faz nada — reciclar algo
/// alcançável corromperia a árvore. É a guarda que separa "a fachada chamou
/// fora de ordem" de um bug silencioso.
#[test]
fn release_subtree_de_no_ainda_anexado_e_um_no_op() {
    let mut dom = parse_html_to_dom("<div id='raiz'><p>oi</p></div>");
    let raiz = dom.query("#raiz").unwrap();
    let p = dom.query("p").unwrap();
    let pico = dom.nodes.len();

    dom.release_subtree(p); // p continua filho de raiz — não desanexado
    assert!(dom.resolve(p).is_some(), "release_subtree reciclou um nó vivo");
    let raiz_idx = dom.resolve(raiz).unwrap();
    assert_eq!(dom.node(raiz_idx).children.len(), 1);
    assert_eq!(dom.nodes.len(), pico);
}

/// Uma subárvore de vários nós recicla TODOS os descendentes, não só a
/// raiz da remoção — senão o vazamento continua um nível abaixo.
#[test]
fn release_subtree_recicla_a_subarvore_inteira() {
    let mut dom = parse_html_to_dom("<div id='raiz'></div>");
    let raiz = dom.query("#raiz").unwrap();
    let sub_id = dom.create_element("ul");
    dom.append_child(raiz, sub_id);
    for _ in 0..5 {
        let li = dom.create_element("li");
        dom.append_child(sub_id, li);
    }
    let pico = dom.nodes.len();

    dom.remove_node(sub_id);
    dom.release_subtree(sub_id);

    // Nenhum crescimento adicional acontece porque os slots ficaram todos na
    // freelist; alocar 6 nós novos (a `<ul>` + 5 `<li>`) não deve crescer a
    // arena além do pico medido acima.
    for _ in 0..6 {
        dom.create_element("li");
    }
    assert_eq!(
        dom.nodes.len(),
        pico,
        "6 slots deveriam ter sido reciclados — a arena cresceu em vez disso"
    );
}
