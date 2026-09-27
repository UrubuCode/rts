//! O diferencial "incremental == do zero" pelos caminhos que `cache.rs` e
//! `cache_flex.rs` não exercitam: o `touch()` GLOBAL (uma folha de estilo que
//! entra ou muda) e a costura de um container com HTML INDENTADO.
//!
//! "Do zero" aqui é um `Dom` NOVO, parseado já no estado final — e não o mesmo
//! `Dom` com `clear_fragment_cache()`, que é o que os testes vizinhos fazem.
//! Essa limpeza só esvazia os fragmentos: os caches de MEDIDA e de largura
//! intrínseca sobrevivem a ela, e são precisamente eles que um `touch()` sem
//! limpeza deixava servir tamanhos anteriores à mudança (invariante I7 de
//! `docs/ui/html-engine/box-tree.md`). Um "do zero" que partilhasse esses
//! caches repetiria o erro dos dois lados e passaria.

    use super::*;

    fn ctx() -> LayoutCtx<'static> {
        LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        }
    }

    /// Compara o layout incremental de `dom` com o de `fresco`, item a item e
    /// retângulo a retângulo dos `ids` pedidos. Os `NodeIdx` dos dois documentos
    /// não são comparáveis (uma mutação aloca nós novos), por isso a geometria
    /// é lida pelo id do elemento.
    fn igual_ao_do_zero(dom: &Dom, fresco: &Dom, ids: &[&str]) {
        let ctx = ctx();
        let incremental = layout_cached(dom, &ctx);
        let zero = layout_document(fresco, &ctx);
        for id in ids {
            let seletor = format!("#{id}");
            let a = dom.resolve(dom.query(&seletor).unwrap()).unwrap();
            let b = fresco.resolve(fresco.query(&seletor).unwrap()).unwrap();
            let (ra, rb) = (incremental.geometry_now().rects[&a], zero.geometry_now().rects[&b]);
            assert!(
                rects_equivalentes(&ra, &rb),
                "#{id}: incremental {ra:?} != do zero {rb:?}"
            );
        }
        let (a, b) = (incremental.materialized(), zero.materialized());
        assert_eq!(a.len(), b.len(), "nº de itens de pintura diverge");
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            assert!(
                itens_equivalentes(x, y),
                "item {i} diverge:\n  incremental: {x:?}\n  do zero:     {y:?}"
            );
        }
    }

    /// Um flex com itens de largura pelo CONTEÚDO e um absoluto shrink-to-fit:
    /// os três passam pelo cache de medida e pelo de largura intrínseca.
    const MEDIDOS: &str = "<div id='f' style='display:flex;width:600px'>\
        <div id='a'><div class='x'>abc</div></div>\
        <div id='b'>defgh</div>\
        </div>\
        <div style='position:relative;width:500px;height:100px'>\
        <div id='abs' style='position:absolute'><div class='x'>ij</div></div>\
        </div>";

    /// Uma folha de estilo NOVA muda a largura de um neto. Nenhum epoch de nó
    /// sobe — `rebuild_author_stylesheet` chama o `touch()` global — e por isso
    /// só a limpeza dos caches de medida impede que o item flex e o absoluto
    /// sejam dispostos com a largura de antes (36px de texto em vez de 300px).
    #[test]
    fn folha_nova_nao_serve_medidas_anteriores_a_ela() {
        let mut dom = parse_html_to_dom(MEDIDOS);
        let _ = layout_cached(&dom, &ctx());
        dom.add_stylesheet(".x{width:300px}");

        let mut fresco = parse_html_to_dom(MEDIDOS);
        fresco.add_stylesheet(".x{width:300px}");
        igual_ao_do_zero(&dom, &fresco, &["f", "a", "b", "abs"]);
    }

    /// O mesmo pela via de um `<style>` que já está no documento e cujo texto
    /// muda — `set_text` num `<style>` reconstrói a folha, e é outro chamador
    /// do `touch()` global.
    #[test]
    fn style_reescrito_nao_serve_medidas_anteriores_a_ele() {
        let com_folha = |css: &str| format!("<style id='s'>{css}</style>{MEDIDOS}");
        let mut dom = parse_html_to_dom(&com_folha(".x{width:10px}"));
        let _ = layout_cached(&dom, &ctx());
        let s = dom.query("#s").unwrap();
        dom.set_text(s, ".x{width:300px}");

        let fresco = parse_html_to_dom(&com_folha(".x{width:300px}"));
        igual_ao_do_zero(&dom, &fresco, &["f", "a", "b", "abs"]);
    }

    /// HTML INDENTADO, como toda página real: entre os blocos há nós de texto
    /// só de espaço, e a árvore de caixas tem uma caixa de TEXTO para cada um.
    /// Um cartão muda de texto sem mudar de altura; o container tem de ser
    /// COSTURADO (só o cartão refeito) e o resultado tem de ser o do zero.
    const INDENTADO: &str = "<main id='m'>
      <div id='c1' style='height:40px'>um</div>
      <div id='c2' style='height:40px'>dois</div>
      <div id='c3' style='height:40px'>tres</div>
    </main>";

    #[test]
    fn texto_mudado_em_html_indentado_bate_com_o_do_zero() {
        let mut dom = parse_html_to_dom(INDENTADO);
        let _ = layout_cached(&dom, &ctx());
        dom.set_text(dom.query("#c2").unwrap(), "DOIS");

        let fresco = parse_html_to_dom(&INDENTADO.replace(">dois<", ">DOIS<"));
        igual_ao_do_zero(&dom, &fresco, &["m", "c1", "c2", "c3"]);
    }

    /// A prova de que a costura ACONTECE com espaço entre os blocos. Comparar
    /// a lista de filhos da caixa com uma lista de fragmentos que nunca tem
    /// texto não casava nunca, e a costura morria em silêncio em toda página
    /// indentada — sem erro nenhum, só o `<main>` inteiro refeito.
    ///
    /// Os números distinguem os dois caminhos: costurado, só `#c2` é refeito
    /// (1 miss) e `#c1`/`#c3` nem são consultados (0 hits); refeito, o `<main>`
    /// também falha (2 misses) e os dois irmãos batem no cache (2 hits). Antes
    /// da correção media-se exatamente o segundo: 1 patch (o `<body>`, cujo
    /// único filho é o `<main>`), 2 misses, 2 hits.
    #[test]
    #[cfg(feature = "metrics")]
    fn html_indentado_e_costurado() {
        let mut dom = parse_html_to_dom(INDENTADO);
        let _ = layout_cached(&dom, &ctx());
        dom.set_text(dom.query("#c2").unwrap(), "DOIS");

        crate::metrics::counters::reset();
        let _ = layout_cached(&dom, &ctx());
        let m = crate::metrics::counters::snapshot();
        assert_eq!(
            (m.fragment_misses, m.fragment_hits),
            (1, 0),
            "só o cartão mudado devia ser refeito; patches={}",
            m.fragment_patches
        );
    }

    /// Dois containers indentados: remover um cartão do primeiro e devolver o
    /// slot à freelist (`release_subtree`, o que a fachada TS faz a cada
    /// `remove()`) não pode desligar a costura do SEGUNDO. O `recycle`
    /// esvaziava o mapa de últimos fragmentos inteiro por nó reciclado.
    const DOIS: &str = "<section id='s1'>
      <div id='r1' style='height:40px'>um</div>
      <div id='r2' style='height:40px'>dois</div>
    </section>
    <section id='s2'>
      <div id='k1' style='height:40px'>um</div>
      <div id='k2' style='height:40px'>dois</div>
      <div id='k3' style='height:40px'>tres</div>
    </section>";

    fn remove_e_recicla(dom: &mut Dom, seletor: &str) {
        let alvo = dom.query(seletor).unwrap();
        dom.remove_node(alvo);
        dom.release_subtree(alvo);
    }

    /// Um filho DESLOCADO e depois refeito dentro de uma costura. `#s2` sobe
    /// 40px quando `#r2` sai e é servido do cache com `dy = -40`; depois muda
    /// de cor (sujo, mesma altura) e é refeito já NA posição nova. A costura
    /// do `<body>` trocava o fragmento e mantinha o `dy` antigo, e o `#s2`
    /// refeito em y=48 era pintado em y=8, por cima do `#s1`.
    #[test]
    fn filho_deslocado_e_refeito_na_costura_bate_com_o_do_zero() {
        let mut dom = parse_html_to_dom(DOIS);
        let _ = layout_cached(&dom, &ctx());
        let r2 = dom.query("#r2").unwrap();
        dom.remove_node(r2);
        let _ = layout_cached(&dom, &ctx());
        dom.set_attr(dom.query("#s2").unwrap(), "style", "color:red");

        let fresco = parse_html_to_dom(
            &DOIS
                .replace("<div id='r2' style='height:40px'>dois</div>", "")
                .replace("<section id='s2'>", "<section id='s2' style='color:red'>"),
        );
        igual_ao_do_zero(&dom, &fresco, &["s1", "r1", "s2", "k1", "k2", "k3"]);
    }

    #[test]
    fn remocao_reciclada_bate_com_o_do_zero() {
        let mut dom = parse_html_to_dom(DOIS);
        let _ = layout_cached(&dom, &ctx());
        remove_e_recicla(&mut dom, "#r2");
        let _ = layout_cached(&dom, &ctx());
        dom.set_text(dom.query("#k2").unwrap(), "DOIS");

        let fresco = parse_html_to_dom(
            &DOIS
                .replace("<div id='r2' style='height:40px'>dois</div>", "")
                .replace("<div id='k2' style='height:40px'>dois", "<div id='k2' style='height:40px'>DOIS"),
        );
        igual_ao_do_zero(&dom, &fresco, &["s1", "r1", "s2", "k1", "k2", "k3"]);
    }

    /// Os números: depois da remoção reciclada, mudar `#k2` refaz só ele —
    /// 1 miss e nenhum hit, a mesma assinatura de `html_indentado_e_costurado`.
    #[test]
    #[cfg(feature = "metrics")]
    fn remocao_reciclada_nao_desliga_a_costura_do_resto() {
        let mut dom = parse_html_to_dom(DOIS);
        let _ = layout_cached(&dom, &ctx());
        remove_e_recicla(&mut dom, "#r2");
        let _ = layout_cached(&dom, &ctx());
        dom.set_text(dom.query("#k2").unwrap(), "DOIS");

        crate::metrics::counters::reset();
        let _ = layout_cached(&dom, &ctx());
        let m = crate::metrics::counters::snapshot();
        assert_eq!(
            (m.fragment_misses, m.fragment_hits),
            (1, 0),
            "só #k2 devia ser refeito; patches={}",
            m.fragment_patches
        );
    }

    /// Um filho que passa de inline a bloco PARTE o `<span>` que o contém
    /// (CSS 2.1 §9.2.1.1): a lista de nós do DOM não muda, a de caixas ganha
    /// duas anónimas. É o caso que comparar nós em vez de caixas deixaria
    /// passar, e o resultado tem de ser o do zero.
    const PARTIDO: &str = "<main id='m'>
      <div id='c1' style='height:40px'>um</div>
      <p id='p'><span id='s'>a<b id='x'>b</b>c</span></p>
    </main>";

    #[test]
    fn inline_que_se_parte_bate_com_o_do_zero() {
        let mut dom = parse_html_to_dom(PARTIDO);
        let _ = layout_cached(&dom, &ctx());
        dom.set_attr(dom.query("#x").unwrap(), "style", "display:block");

        let fresco =
            parse_html_to_dom(&PARTIDO.replace("<b id='x'>", "<b id='x' style='display:block'>"));
        igual_ao_do_zero(&dom, &fresco, &["m", "c1", "p", "x"]);
    }
