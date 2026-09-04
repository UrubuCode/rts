//! Lote O — os seletores que faltavam: `:has()`, `:target`, `:scope`,
//! `:default`, `:placeholder-shown`, `:active`, `:visited`, `::marker`.
//! Cada teste nomeia o COMPORTAMENTO que a pseudo pina, não a função Rust.

    use super::*;

    // ── :has() ─────────────────────────────────────────────────────────────

    #[test]
    fn has_casa_o_ancestral_que_tem_um_descendente_com_a_classe() {
        let dom = parse_html_to_dom(
            "<style>.card:has(.err){color:#ff0000}</style>\
             <div class='card' id='com-erro'><span class='err'>x</span></div>\
             <div class='card' id='sem-erro'><span>y</span></div>",
        );
        let com_erro = dom.query("#com-erro").unwrap();
        let sem_erro = dom.query("#sem-erro").unwrap();
        assert_eq!(
            dom.computed_style(com_erro).unwrap().color,
            Some(0xff0000ff),
            ".card:has(.err) tem de colorir o card QUE tem .err"
        );
        assert_ne!(
            dom.computed_style(sem_erro).unwrap().color,
            Some(0xff0000ff),
            ".card:has(.err) NÃO pode colorir o card sem .err"
        );
    }

    #[test]
    fn has_com_combinador_filho_so_conta_filho_direto() {
        let dom = parse_html_to_dom(
            "<style>.box:has(> img){color:#ff0000}</style>\
             <div class='box' id='direto'><img></div>\
             <div class='box' id='neto'><span><img></span></div>",
        );
        let direto = dom.query("#direto").unwrap();
        let neto = dom.query("#neto").unwrap();
        assert_eq!(dom.computed_style(direto).unwrap().color, Some(0xff0000ff));
        assert_ne!(
            dom.computed_style(neto).unwrap().color,
            Some(0xff0000ff),
            ":has(> img) não pode casar um <img> NETO"
        );
    }

    #[test]
    fn has_com_combinador_irmao_seguinte() {
        let dom = parse_html_to_dom(
            "<style>.rotulo:has(+ .obrigatorio){color:#ff0000}</style>\
             <span class='rotulo' id='com'>Nome</span><i class='obrigatorio'>*</i>\
             <span class='rotulo' id='sem'>Idade</span><i>opcional</i>",
        );
        let com = dom.query("#com").unwrap();
        let sem = dom.query("#sem").unwrap();
        assert_eq!(dom.computed_style(com).unwrap().color, Some(0xff0000ff));
        assert_ne!(dom.computed_style(sem).unwrap().color, Some(0xff0000ff));
    }

    /// A FRONTEIRA: um `:has(.a .b)` só casa se `.a` está DENTRO do alvo, não
    /// em qualquer parte da árvore acima de `.b`. Sem a fronteira, isto casaria
    /// (existe um `.fora .dentro` na página, só que `.fora` fica FORA da
    /// `<section>`) — é o teste que distingue `match_combinators_bounded` de um
    /// `matches_complex` sem fronteira nenhuma.
    #[test]
    fn has_nao_ultrapassa_a_fronteira_do_alvo() {
        let dom = parse_html_to_dom(
            "<style>section:has(.fora .dentro){color:#ff0000}</style>\
             <div class='fora'><section><span class='dentro'>x</span></section></div>",
        );
        let section = dom.query("section").unwrap();
        assert_ne!(
            dom.computed_style(section).unwrap().color,
            Some(0xff0000ff),
            "`.fora` está ACIMA da section, não dentro dela — :has() não pode casar"
        );
    }

    /// O CUSTO declarado: com `:has()` na folha, uma mutação estrutural
    /// invalida o DOCUMENTO inteiro (não só a subárvore do pai, que basta para
    /// `:nth-child`/`:first-child` — ver o comentário em `touch_structural`).
    /// Pina o comportamento e não o número: o que importa aqui é que um nó
    /// LONGE da mutação perde o memo, que é o sinal de que o `touch()` global
    /// rodou em vez do `touch_subtrees` estreito.
    #[test]
    fn has_na_folha_invalida_o_documento_inteiro_numa_mutacao_estrutural() {
        let mut dom = parse_html_to_dom(
            "<style>body:has(.gatilho){color:#ff0000}</style>\
             <div id='longe'>x</div><div id='pai'></div>",
        );
        let longe = dom.resolve(dom.query("#longe").unwrap()).unwrap();
        let _ = dom.computed_style_idx(longe); // preenche o memo
        assert!(memoizado(&dom, longe));
        let pai = dom.query("#pai").unwrap();
        let novo = dom.create_element("span");
        dom.set_attr(novo, "class", "gatilho");
        dom.append_child(pai, novo);
        assert!(
            !memoizado(&dom, longe),
            "com `:has()` na folha, appendChild em QUALQUER lugar tem de esquecer \
             o memo de nós longe — é o custo declarado, não uma subárvore estreita"
        );
    }

    // ── :target ────────────────────────────────────────────────────────────

    #[test]
    fn target_casa_o_id_do_fragmento_corrente() {
        let mut dom = parse_html_to_dom(
            "<style>:target{color:#ff0000}</style><div id='a'>x</div><div id='b'>y</div>",
        );
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        // sem fragmento, ninguém é :target.
        assert_ne!(dom.computed_style(a).unwrap().color, Some(0xff0000ff));
        dom.set_location_hash("#a");
        assert_eq!(dom.computed_style(a).unwrap().color, Some(0xff0000ff));
        assert_ne!(dom.computed_style(b).unwrap().color, Some(0xff0000ff));
    }

    // ── :scope ─────────────────────────────────────────────────────────────

    #[test]
    fn scope_em_query_within_e_o_elemento_da_consulta_nao_o_documento() {
        let dom = parse_html_to_dom(
            "<div id='raiz'><ul><li id='item'>x</li></ul></div><li id='fora'>y</li>",
        );
        let raiz = dom.query("#raiz").unwrap();
        // `:scope li` a partir de #raiz só pode achar o <li> DENTRO dele.
        let achado = dom.query_within(raiz, ":scope li").unwrap();
        assert_eq!(achado, dom.query("#item").unwrap());
    }

    // ── :default ───────────────────────────────────────────────────────────

    #[test]
    fn default_casa_a_opcao_pre_selecionada_e_o_checkbox_marcado() {
        let dom = parse_html_to_dom(
            "<select><option id='n' value='1'>um</option>\
             <option id='s' value='2' selected>dois</option></select>\
             <input id='c' type='checkbox' checked><input id='c2' type='checkbox'>",
        );
        assert!(dom.matches(dom.resolve(dom.query("#s").unwrap()).unwrap(), ":default"));
        assert!(!dom.matches(dom.resolve(dom.query("#n").unwrap()).unwrap(), ":default"));
        assert!(dom.matches(dom.resolve(dom.query("#c").unwrap()).unwrap(), ":default"));
        assert!(!dom.matches(dom.resolve(dom.query("#c2").unwrap()).unwrap(), ":default"));
    }

    #[test]
    fn default_casa_o_primeiro_botao_submit_do_formulario() {
        let dom = parse_html_to_dom(
            "<form><button id='primeiro' type='submit'>ok</button>\
             <button id='segundo' type='submit'>tambem ok</button></form>",
        );
        assert!(dom.matches(dom.resolve(dom.query("#primeiro").unwrap()).unwrap(), ":default"));
        assert!(!dom.matches(dom.resolve(dom.query("#segundo").unwrap()).unwrap(), ":default"));
    }

    // ── :placeholder-shown ────────────────────────────────────────────────

    #[test]
    fn placeholder_shown_so_com_placeholder_e_valor_vazio() {
        let mut dom = parse_html_to_dom(
            "<input id='vazio' placeholder='nome'><input id='com-valor' placeholder='nome' value='x'>\
             <input id='sem-placeholder'>",
        );
        let vazio = dom.query("#vazio").unwrap();
        let com_valor = dom.query("#com-valor").unwrap();
        let sem_placeholder = dom.query("#sem-placeholder").unwrap();
        assert!(dom.matches(dom.resolve(vazio).unwrap(), ":placeholder-shown"));
        assert!(!dom.matches(dom.resolve(com_valor).unwrap(), ":placeholder-shown"));
        assert!(!dom.matches(dom.resolve(sem_placeholder).unwrap(), ":placeholder-shown"));
        // digitar no campo vazio esconde o placeholder, como no browser.
        dom.set_input_value(dom.resolve(vazio).unwrap(), "agora tem texto");
        assert!(!dom.matches(dom.resolve(vazio).unwrap(), ":placeholder-shown"));
    }

    // ── :active / :visited ────────────────────────────────────────────────

    #[test]
    fn active_casa_so_o_no_marcado_por_set_active() {
        let mut dom = parse_html_to_dom("<button id='a'>x</button><button id='b'>y</button>");
        let a = dom.resolve(dom.query("#a").unwrap()).unwrap();
        let b = dom.resolve(dom.query("#b").unwrap()).unwrap();
        assert!(!dom.matches(a, ":active"));
        dom.set_active(Some(a));
        assert!(dom.matches(a, ":active"));
        assert!(!dom.matches(b, ":active"));
        dom.set_active(None);
        assert!(!dom.matches(a, ":active"));
    }

    #[test]
    fn visited_casa_so_o_link_cujo_href_foi_marcado() {
        let mut dom =
            parse_html_to_dom("<a id='v' href='/um'>x</a><a id='nv' href='/dois'>y</a>");
        let v = dom.resolve(dom.query("#v").unwrap()).unwrap();
        let nv = dom.resolve(dom.query("#nv").unwrap()).unwrap();
        assert!(!dom.matches(v, ":visited"));
        dom.mark_visited("/um");
        assert!(dom.matches(v, ":visited"));
        assert!(!dom.matches(nv, ":visited"));
    }

    // ── ::marker ───────────────────────────────────────────────────────────

    #[test]
    fn marker_color_vem_da_regra_e_nao_do_li() {
        let dom = parse_html_to_dom(
            "<style>li{color:#0000ff}li::marker{color:#ff0000}</style><ul><li id='x'>a</li></ul>",
        );
        let x = dom.resolve(dom.query("#x").unwrap()).unwrap();
        let herdado = dom.computed_style_idx(x).unwrap();
        assert_eq!(herdado.color, Some(0x0000ffff), "o texto do <li> continua azul");
        assert_eq!(
            dom.marker_color(x, &herdado),
            Some(0xff0000ff),
            "o ::marker tem cor PRÓPRIA, vermelha"
        );
    }

    #[test]
    fn marker_sem_regra_devolve_none_e_quem_chama_usa_a_cor_herdada() {
        let dom = parse_html_to_dom("<ul><li id='x'>a</li></ul>");
        let x = dom.resolve(dom.query("#x").unwrap()).unwrap();
        let herdado = dom.computed_style_idx(x).unwrap();
        assert_eq!(dom.marker_color(x, &herdado), None);
    }
