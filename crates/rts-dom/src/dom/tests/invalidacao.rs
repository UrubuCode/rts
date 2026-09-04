//! Invalidação: dirty bits por subárvore, memos de estilo, `render_revision`
//! e o alcance do `:hover`.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de teste foi alterada.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! vários testes têm literais de string multi-linha em que o espaço à
//! esquerda é conteúdo, e desindentar mudaria o que eles afirmam.

    use super::*;


    #[test]
    fn dirty_bits_invalidam_apenas_subarvore_local() {
        let mut dom = parse_html_to_dom("<div><p id='a'>a</p><p id='b'>b</p></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        let a_idx = idx(&dom, a);
        let b_idx = idx(&dom, b);

        // Preenche os dois memos antes da mutação.
        let _ = dom.computed_style(a);
        let _ = dom.computed_style(b);
        assert!(memoizado(&dom, a_idx));
        assert!(memoizado(&dom, b_idx));

        let before_noop = dom.render_revision();
        dom.set_attr(a, "data-state", "on");
        assert_ne!(dom.render_revision(), before_noop);
        assert!(!memoizado(&dom, a_idx));
        assert!(memoizado(&dom, b_idx));

        // Repetir o mesmo setAttribute é um no-op e não cria nova invalidação.
        let before_repeat = dom.render_revision();
        dom.set_attr(a, "data-state", "on");
        assert_eq!(dom.render_revision(), before_repeat);
    }


    #[test]
    fn atributo_invalida_irmaos_quando_usado_em_seletor() {
        let mut dom = parse_html_to_dom(
            "<style>[data-state='on'] + p { color:#ff0000 }</style>\
             <div><p id='first'>a</p><p id='second'>b</p></div>",
        );
        let first = dom.query("#first").unwrap();
        let second = dom.query("#second").unwrap();
        assert_eq!(dom.computed_style(second).unwrap().color, None);

        dom.set_attr(first, "data-state", "on");
        assert_eq!(dom.computed_style(second).unwrap().color, Some(0xFF0000FF));
    }


    #[test]
    fn classe_no_pai_invalida_descendentes_herdados() {
        let mut dom = parse_html_to_dom(
            "<style>.hot { color:#ff0000 }</style><div id='parent'><p id='child'>x</p></div>",
        );
        let parent = dom.query("#parent").unwrap();
        let child = dom.query("#child").unwrap();
        assert_eq!(dom.computed_style(child).unwrap().color, None);

        dom.set_attr(parent, "class", "hot");
        assert_eq!(dom.computed_style(child).unwrap().color, Some(0xFF0000FF));
    }


    #[test]
    fn stylesheet_invalida_todos_os_memos() {
        let mut dom = parse_html_to_dom("<p id='a'>a</p><p id='b'>b</p>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        let _ = dom.computed_style(a);
        let _ = dom.computed_style(b);
        assert!(!dom.computed_memo.borrow().is_empty());
        assert!(!dom.base_memo.borrow().is_empty());

        dom.add_stylesheet("p { color:#0000ff }");
        assert!(dom.computed_memo.borrow().is_empty());
        assert!(dom.base_memo.borrow().is_empty());
        assert_eq!(dom.computed_style(a).unwrap().color, Some(0x0000FFFF));
        assert_eq!(dom.computed_style(b).unwrap().color, Some(0x0000FFFF));
    }


    #[test]
    fn animacao_preserva_memo_base_entre_frames() {
        let mut dom = parse_html_to_dom(
            "<style>@keyframes pulse { from { opacity:0 } to { opacity:1 } } \
             #a { animation:pulse 100ms infinite linear }</style><p id='a'>x</p>",
        );
        let node = dom.query("#a").unwrap();
        let node_idx = idx(&dom, node);

        assert!(dom.advance(0.0));
        let base_revision = dom.base_memo_revision.get();
        let base_before = dom.base_memo.borrow().get(node_idx).cloned().flatten();
        assert!(base_before.is_some());
        let _ = dom.computed_style(node);
        assert!(memoizado(&dom, node_idx));

        assert!(dom.advance(50.0));
        assert_eq!(dom.base_memo_revision.get(), base_revision);
        assert_eq!(
            dom.base_memo.borrow().get(node_idx).cloned().flatten(),
            base_before
        );
        // O epoch de animação muda, portanto o memo interpolado é recalculado sob
        // demanda; o alvo-base continua sendo o mesmo objeto lógico.
        let _ = dom.computed_style(node);
        assert!(memoizado(&dom, node_idx));
    }


    #[test]
    fn hover_sem_regra_nao_invalida() {
        // página SEM :hover: mover o mouse não pode custar re-layout.
        let mut dom = parse_html_to_dom("<div id=a>x</div>");
        let a = dom.query("#a").unwrap();
        let idx = dom.resolve(a).unwrap();
        let rev0 = dom.render_revision();
        dom.set_hovered(Some(idx));
        assert_eq!(dom.render_revision(), rev0);
    }


    #[test]
    fn revision_bumpa_na_mutacao_e_nao_na_leitura() {
        // O contrato dos caches de layout (backend + GEOM_CACHE): a revisão muda a
        // cada MUTAÇÃO que afeta render, e NÃO muda em leituras — inclusive o
        // computed_style memoizado (que preenche o memo mas não altera o estado).
        let mut dom = parse_html_to_dom("<div id='a' style='color:#fff'>x</div>");
        let r0 = dom.render_revision();
        // leituras não bumpam (o memo do computed usa interior mutability).
        let a = dom.query("#a").unwrap();
        let _ = dom.computed_style(a);
        let _ = dom.computed_style(a); // 2ª leitura = hit do memo
        assert_eq!(dom.render_revision(), r0, "leitura não muda a revisão");
        // mutações bumpam — e o computed reflete a mudança (memo invalidado).
        dom.set_attr(a, "style", "color:#ff0000");
        assert_ne!(dom.render_revision(), r0, "set_attr bumpa");
        let css = dom.computed_style(a).unwrap();
        assert_eq!(css.color, Some(0xFF0000FF), "memo invalidado pela revisão");
        let r1 = dom.render_revision();
        dom.set_text(a, "novo");
        assert_ne!(dom.render_revision(), r1, "set_text bumpa");
        // defineStyle (estado global por-tag, fora do Dom) também invalida.
        let r2 = dom.render_revision();
        crate::style::define_style("tag_rev_teste", crate::style::SLOT_COLOR, 0x11223344);
        assert_ne!(
            dom.render_revision(),
            r2,
            "defineStyle bumpa o epoch global"
        );
    }


    #[test]
    fn no_op_mutations_nao_invalidam_layout() {
        let mut dom = parse_html_to_dom("<div id='a' style='color:#fff'>x</div>");
        let a = dom.query("#a").unwrap();
        let r0 = dom.render_revision();
        dom.set_attr(a, "style", "color:#fff");
        assert_eq!(dom.render_revision(), r0, "set_attr igual deve ser no-op");
        dom.remove_attr(a, "data-ausente");
        assert_eq!(
            dom.render_revision(),
            r0,
            "remove_attr ausente deve ser no-op"
        );
        dom.set_css_text(a, "color:#fff");
        assert_eq!(
            dom.render_revision(),
            r0,
            "set_css_text igual deve ser no-op"
        );
    }


    /// O ALCANCE de `:hover` decide o que a mudança de hover invalida, e os
    /// quatro casos exigem coisas diferentes — trocar um pelo outro ou invalida
    /// demais (a página inteira, o que era o comportamento) ou de menos (um
    /// irmão que fica com estilo velho).
    #[test]
    fn alcance_de_hover_por_forma_do_seletor() {
        use crate::style::HoverReach;
        let caso = |css: &str| {
            let mut sheet = crate::style::Stylesheet::new();
            sheet.append_css(css);
            sheet.hover_reach()
        };
        assert_eq!(caso(".btn { color: red }"), HoverReach::None);
        assert_eq!(caso(".btn:hover { color: red }"), HoverReach::SelfOnly);
        assert_eq!(
            caso(".card:hover .title { color: red }"),
            HoverReach::Subtree
        );
        assert_eq!(caso(".a:hover + .b { color: red }"), HoverReach::Siblings);
    }


    /// Passar o mouse não pode invalidar o estilo de um ramo que o `:hover` não
    /// alcança. É o teste do INVALIDATION SET: sem ele a implementação correta e
    /// a que re-cascadeia a página inteira são indistinguíveis por asserção.
    #[cfg(feature = "metrics")]
    #[test]
    fn hover_recascadeia_a_cadeia_e_nao_a_pagina() {
        let filhos: String = (0..50)
            .map(|i| format!("<p class=\"linha\">l{i}</p>"))
            .collect();
        let mut dom = parse_html_to_dom(&format!(
            "<style>.btn:hover {{ color: red }}</style>             <div id=\"lado\">{filhos}</div><a class=\"btn\" id=\"alvo\">x</a>"
        ));
        // Preenche os memos de todos os nós.
        for idx in 0..dom.nodes.len() {
            let _ = dom.computed_style_idx(idx);
        }
        let alvo = dom.resolve(dom.query("#alvo").unwrap()).unwrap();
        crate::metrics::counters::reset();
        dom.set_hovered(Some(alvo));
        for idx in 0..dom.nodes.len() {
            let _ = dom.computed_style_idx(idx);
        }
        let cascades = crate::metrics::snapshot().cascade_runs;
        assert!(
            cascades < 10,
            "hover re-cascadeou {cascades} nós; a cadeia do alvo tem 2 e a página tem 50+"
        );
    }


    /// O caso que obriga o fallback: `.a:hover + .b` muda um nó FORA da subárvore
    /// de quem casa, e uma invalidação por subárvore o deixaria com estilo velho.
    #[test]
    fn hover_com_irmao_invalida_o_irmao() {
        let mut dom = parse_html_to_dom(
            "<style>.a:hover + .b { color: #ff0000 }</style>             <div><span class=\"a\">a</span><span class=\"b\">b</span></div>",
        );
        let b = dom.resolve(dom.query(".b").unwrap()).unwrap();
        assert_eq!(dom.computed_style_idx(b).unwrap().color, None);
        let a = dom.resolve(dom.query(".a").unwrap()).unwrap();
        dom.set_hovered(Some(a));
        assert_eq!(
            dom.computed_style_idx(b).unwrap().color,
            Some(0xFF0000FF),
            "o irmão precisa reagir ao hover do anterior"
        );
    }

    /// Com um seletor sensível a posição na folha, inserir um irmão muda a
    /// paridade dos outros — e SÓ deles: um nó fora da subárvore do pai mutado
    /// mantém o memo. Antes de 2026-09-04 isto era um `touch()` global, e um
    /// `tr:nth-child(odd)` em qualquer ponto da folha fazia cada `appendChild`
    /// esquecer o estilo da página inteira.
    #[test]
    fn nth_child_na_folha_invalida_os_irmaos_e_nao_o_documento() {
        let mut dom = parse_html_to_dom(
            "<style>li:nth-child(odd) { color:#ff0000 }</style><ul id='lista'><li id='l1'>a</li><li id='l2'>b</li></ul><div id='outro'><p id='p'>x</p></div>",
        );
        let lista = dom.query("#lista").unwrap();
        let l1 = dom.query("#l1").unwrap();
        let l2 = dom.query("#l2").unwrap();
        let p = dom.query("#p").unwrap();
        let (l1_idx, l2_idx, p_idx) = (idx(&dom, l1), idx(&dom, l2), idx(&dom, p));
        assert_eq!(dom.computed_style(l1).unwrap().color, Some(0xFF0000FF));
        assert_eq!(dom.computed_style(l2).unwrap().color, None);
        let _ = dom.computed_style(p);
        assert!(memoizado(&dom, l1_idx) && memoizado(&dom, l2_idx) && memoizado(&dom, p_idx));

        let novo = dom.create_element("li");
        dom.insert_before(lista, novo, Some(l1));

        // Os irmãos esquecem; quem está fora da subárvore do pai não.
        assert!(!memoizado(&dom, l1_idx), "l1 mudou de paridade e tem de ser recalculado");
        assert!(!memoizado(&dom, l2_idx), "l2 mudou de paridade e tem de ser recalculado");
        assert!(memoizado(&dom, p_idx), "#p está fora da subárvore do pai mutado");

        // E a resposta recalculada é a certa: novo=1.º (ímpar), l1=2.º, l2=3.º.
        assert_eq!(dom.computed_style(novo).unwrap().color, Some(0xFF0000FF));
        assert_eq!(dom.computed_style(l1).unwrap().color, None);
        assert_eq!(dom.computed_style(l2).unwrap().color, Some(0xFF0000FF));
    }


    /// Remover um irmão (o `former_parent` é o único pai envolvido) segue a
    /// mesma regra: a lista é recalculada, o resto do documento não.
    #[test]
    fn remover_irmao_com_nth_child_invalida_so_a_lista() {
        let mut dom = parse_html_to_dom(
            "<style>li:first-child { color:#00ff00 }</style><ul id='lista'><li id='l1'>a</li><li id='l2'>b</li></ul><div id='outro'><p id='p'>x</p></div>",
        );
        let l1 = dom.query("#l1").unwrap();
        let l2 = dom.query("#l2").unwrap();
        let p = dom.query("#p").unwrap();
        let (l2_idx, p_idx) = (idx(&dom, l2), idx(&dom, p));
        assert_eq!(dom.computed_style(l2).unwrap().color, None);
        let _ = dom.computed_style(p);

        dom.remove_node(l1);
        assert!(!memoizado(&dom, l2_idx), "l2 passou a ser o primeiro");
        assert!(memoizado(&dom, p_idx), "#p está fora da lista");
        assert_eq!(dom.computed_style(l2).unwrap().color, Some(0x00FF00FF));
    }

