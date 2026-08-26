//! A cascade: `<style>` de autor, especificidade, `!important`, variáveis,
//! propriedades computadas e o `style` inline.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de teste foi alterada.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! vários testes têm literais de string multi-linha em que o espaço à
//! esquerda é conteúdo, e desindentar mudaria o que eles afirmam.

    use super::*;


    #[test]
    fn style_override_por_no_e_batch() {
        use crate::style::{SLOT_BG, SLOT_COLOR};
        let mut dom = parse_html_to_dom("<div><p id='a'>x</p><p id='b'>y</p></div>");
        let a = dom.query("#a").unwrap();
        let b = dom.query("#b").unwrap();
        // setNodeStyleSlot (1 nó, 1 slot): cor vermelha no #a.
        dom.set_node_style_slot(a, SLOT_COLOR, 0xFF0000FF);
        assert_eq!(dom.computed_style(a).unwrap().color, Some(0xFF0000FF));
        assert_eq!(dom.computed_style(b).unwrap().color, None); // #b intacto
        // batch: triplas planas [id, slot, val] — bg em ambos + cor no #b.
        let triples = vec![
            a.to_abi(),
            SLOT_BG,
            0x111111FF,
            b.to_abi(),
            SLOT_BG,
            0x222222FF,
            b.to_abi(),
            SLOT_COLOR,
            0x00FF00FF,
        ];
        dom.apply_style_batch(&triples);
        assert_eq!(dom.computed_style(a).unwrap().bg, Some(0x111111FF));
        assert_eq!(dom.computed_style(b).unwrap().bg, Some(0x222222FF));
        assert_eq!(dom.computed_style(b).unwrap().color, Some(0x00FF00FF));
        // o override VENCE o estilo inline:
        let mut dom2 = parse_html_to_dom("<p id='c' style='color:#0000ff'>z</p>");
        let c = dom2.query("#c").unwrap();
        assert_eq!(dom2.computed_style(c).unwrap().color, Some(0x0000FFFF)); // inline
        dom2.set_node_style_slot(c, SLOT_COLOR, 0xFF0000FF);
        assert_eq!(dom2.computed_style(c).unwrap().color, Some(0xFF0000FF)); // override vence
    }


    #[test]
    fn style_tag_alimenta_cascade() {
        // <style> com tag/.class/#id alimenta o computed_style por especificidade.
        let dom = parse_html_to_dom(
            "<style>p { color:#ff0000; font-size:14 } .hl { color:#00ff00 } #x { color:#0000ff }</style>\
             <p>normal</p><p class='hl'>destaque</p><p id='x' class='hl'>id</p>",
        );
        let ps = dom.query_all("p");
        assert_eq!(ps.len(), 3);
        // <p> normal: regra de tag.
        let s0 = dom.computed_style(ps[0]).unwrap();
        assert_eq!(s0.color, Some(0xFF0000FF));
        assert_eq!(s0.font_size, Some(crate::style::Dimension::Px(14.0)));
        // <p class="hl">: classe vence a tag na cor; font-size herda da tag.
        let s1 = dom.computed_style(ps[1]).unwrap();
        assert_eq!(s1.color, Some(0x00FF00FF));
        assert_eq!(s1.font_size, Some(crate::style::Dimension::Px(14.0)));
        // <p id="x" class="hl">: id vence tudo.
        let s2 = dom.computed_style(ps[2]).unwrap();
        assert_eq!(s2.color, Some(0x0000FFFF));
    }


    #[test]
    fn style_tag_precede_inline_e_preserva_no() {
        // precedência: <style> autor < style="" inline.
        let dom = parse_html_to_dom(
            "<style>.c { color:#ff0000; padding:10 }</style>\
             <div class='c' style='color:#0000ff'>x</div>",
        );
        let div = dom.query(".c").unwrap();
        let s = dom.computed_style(div).unwrap();
        assert_eq!(s.color, Some(0x0000FFFF)); // inline vence o <style>
        assert_eq!(s.padding.top, crate::style::Side::px_len(10.0)); // padding só o <style> define
        // o <style> também vira NÓ no DOM (fiel), com o CSS como texto cru filho.
        let st = dom.query("style").unwrap();
        assert_eq!(dom.node_type(st), 1); // Element
        let kids = dom.child_nodes(st);
        assert_eq!(kids.len(), 1);
        assert_eq!(dom.node_type(kids[0]), 3); // Text (o CSS cru)
    }


    #[test]
    fn important_inverte_precedencia_de_origem() {
        // MDN estágio 1: `<style>` com `!important` vence o `style=""` inline NORMAL
        // (normalmente o inline venceria o autor; o `!important` inverte isso).
        let dom = parse_html_to_dom(
            "<style>.c { color:#ff0000 !important }</style>\
             <div class='c' style='color:#0000ff'>x</div>",
        );
        let div = dom.query(".c").unwrap();
        assert_eq!(dom.computed_style(div).unwrap().color, Some(0xFF0000FF)); // important vence
        // mas inline `!important` vence o autor `!important` (mesma camada, inline
        // é origem mais forte que o `<style>`):
        let dom2 = parse_html_to_dom(
            "<style>.c { color:#ff0000 !important }</style>\
             <div class='c' style='color:#0000ff !important'>x</div>",
        );
        let div2 = dom2.query(".c").unwrap();
        assert_eq!(dom2.computed_style(div2).unwrap().color, Some(0x0000FFFF)); // inline important
    }


    #[test]
    fn style_tag_conteudo_nao_vira_html() {
        // CSS com `{`, `>` em `a > b` não deve criar tags-fantasma na árvore.
        let dom = parse_html_to_dom("<style>a > b { color:red } p { color:blue }</style><p>oi</p>");
        // o `<b>` do combinador NÃO vira nó na árvore (ficou dentro do raw-text).
        assert!(dom.query("b").is_none());
        assert!(dom.query("p").is_some());
        // o combinador `a > b` é cortado (não suportado); mas `p { }` simples passa.
        assert!(!dom.stylesheet().is_empty());
        assert_eq!(
            dom.computed_style(dom.query("p").unwrap()).unwrap().color,
            Some(0x0000FFFF)
        );
    }


    #[test]
    fn hover_vivo_na_cascade() {
        // `:hover` casa o nó sob o cursor E seus ancestrais; set_hovered só bumpa
        // a revisão quando muda e quando há regra :hover.
        let mut dom = parse_html_to_dom(
            "<style>a:hover { color: #ff0000 } li:hover { color: #00ff00 }</style>\
             <ul><li id=item><a id=link href=x>l</a></li></ul>",
        );
        let link = dom.query("#link").unwrap();
        let item = dom.query("#item").unwrap();
        let link_idx = dom.resolve(link).unwrap();
        // sem hover: nenhum casa.
        let c0 = dom.computed_style(link).unwrap();
        assert_ne!(c0.color, Some(0xFF0000FF));
        let rev0 = dom.render_revision();
        // hovered no <a>: a regra a:hover casa o link; li:hover casa o PAI (propaga).
        dom.set_hovered(Some(link_idx));
        assert!(
            dom.render_revision() != rev0,
            "hover com regra :hover deve invalidar"
        );
        let c1 = dom.computed_style(link).unwrap();
        assert_eq!(c1.color, Some(0xFF0000FF));
        let c2 = dom.computed_style(item).unwrap();
        assert_eq!(c2.color, Some(0x00FF00FF));
        // repetir o MESMO hovered não bumpa (guarda de perf).
        let rev1 = dom.render_revision();
        dom.set_hovered(Some(link_idx));
        assert_eq!(dom.render_revision(), rev1);
        // sair: volta ao normal.
        dom.set_hovered(None);
        let c3 = dom.computed_style(link).unwrap();
        assert_ne!(c3.color, Some(0xFF0000FF));
    }


    #[test]
    fn var_por_elemento_na_cascade() {
        // #1779: .btn usa var(--btn-bg); cada VARIANTE redefine a var NO SELETOR
        // do componente — cada botao pega a SUA cor. (O antigo mapa global dava a
        // mesma cor a todos: a ultima declaracao do arquivo vencia.)
        let dom = parse_html_to_dom(
            "<html><head><style>               :root { --btn-bg: #000000 }               .btn { background: var(--btn-bg) }               .btn-primary { --btn-bg: #0000ff }               .btn-danger { --btn-bg: #ff0000 }             </style></head><body>             <div id=\"a\" class=\"btn btn-primary\">a</div>             <div id=\"b\" class=\"btn btn-danger\">b</div>             <div id=\"c\" class=\"btn\">c</div>             </body></html>",
        );
        let bg = |sel: &str| {
            let n = dom.query(sel).unwrap();
            dom.computed_property(n, "background-color")
        };
        assert_eq!(bg("#a"), "rgb(0, 0, 255)", "btn-primary redefine a var");
        assert_eq!(bg("#b"), "rgb(255, 0, 0)", "btn-danger redefine a var");
        assert_eq!(bg("#c"), "rgb(0, 0, 0)", "sem variante: o :root vale");
    }


    #[test]
    fn var_heranca_fallback_e_aninhado() {
        // heranca: o filho usa a var declarada no ANCESTRAL; fallback quando
        // ausente; var aninhada (--a referencia --b).
        let dom = parse_html_to_dom(
            "<style>               #pai { --c: #00ff00; --a: var(--b); --b: #112233 }               span { color: var(--c) }               em { color: var(--a) }               p { color: var(--nada, #123456) }             </style>             <div id=\"pai\"><span id=\"f\">x</span><em id=\"e\">y</em></div>             <p id=\"p\">z</p>",
        );
        let color = |sel: &str| {
            let n = dom.query(sel).unwrap();
            dom.computed_property(n, "color")
        };
        assert_eq!(color("#f"), "rgb(0, 255, 0)", "var herdada do pai");
        assert_eq!(color("#e"), "rgb(17, 34, 51)", "var aninhada resolve");
        assert_eq!(color("#p"), "rgb(18, 52, 86)", "fallback quando ausente");
    }


    #[test]
    fn cascade_com_seletor_composto() {
        let dom = parse_html_to_dom(
            "<style>p { color:#000000 } p.hi { color:#ff0000 } div > p.hi { color:#00ff00 }</style>\
             <div><p class=\"hi\">x</p></div>",
        );
        let p = dom.query("p.hi").unwrap();
        assert_eq!(dom.computed_style(p).unwrap().color, Some(0x00FF00FF));
    }


    #[test]
    fn computed_property_formato_browser() {
        // getComputedStyle por nome, formato do browser (#1759).
        let dom = parse_html_to_dom(
            "<style>#a{color:#ff0000;background:rgba(0,0,255,0.5);font-size:18px;padding:10px}</style><div id=\"a\">x</div>",
        );
        let a = dom.query("#a").unwrap();
        assert_eq!(dom.computed_property(a, "color"), "rgb(255, 0, 0)");
        // alpha a 2 casas — VALIDADO no Chrome (#..80 / rgba(.5) → "0.5", não 0.501961).
        assert_eq!(
            dom.computed_property(a, "background-color"),
            "rgba(0, 0, 255, 0.5)"
        );
        assert_eq!(dom.computed_property(a, "font-size"), "18px");
        assert_eq!(dom.computed_property(a, "padding-top"), "10px");
        // NÃO declarado responde o valor USADO, não vazio: `getComputedStyle` de
        // um browser devolve sempre um valor computado, e para uma margem que
        // ninguém declarou esse valor é `0px`. O vazio que aqui se esperava era
        // a nossa resposta antiga, e um programa que compare com o browser via
        // a diferença.
        assert_eq!(dom.computed_property(a, "margin-top"), "0px");
    }


    #[test]
    fn style_set_get_remove_property() {
        // el.style.setProperty/getPropertyValue/removeProperty + cssText (#1759).
        let mut dom = parse_html_to_dom("<div id=\"a\" style=\"color: red; padding: 5px\">x</div>");
        let a = dom.query("#a").unwrap();
        // get inline.
        assert_eq!(dom.inline_property(a, "color"), "rgb(255, 0, 0)");
        // set nova prop preserva as outras.
        dom.set_style_property(a, "font-size", "20px");
        assert_eq!(dom.inline_property(a, "font-size"), "20px");
        assert_eq!(dom.inline_property(a, "color"), "rgb(255, 0, 0)"); // mantida
        // atualizar prop existente.
        dom.set_style_property(a, "color", "blue");
        assert_eq!(dom.inline_property(a, "color"), "rgb(0, 0, 255)");
        // remover.
        dom.remove_style_property(a, "padding");
        assert_eq!(dom.inline_property(a, "padding-top"), "");
        // cssText reflete o estado.
        assert!(dom.css_text(a).contains("color: blue"));
        assert!(dom.css_text(a).contains("font-size: 20px"));
        assert!(!dom.css_text(a).contains("padding"));
    }


    #[test]
    fn upsert_preserva_important() {
        // editar uma prop com !important NÃO perde a prioridade (verificação adversarial).
        let r = upsert_css_decl("color: red !important; margin: 0", "color", "blue");
        assert!(r.contains("color: blue !important"), "got: {r}");
        assert!(r.contains("margin: 0"));
        // prop sem important continua sem.
        let r2 = upsert_css_decl("color: red; margin: 0", "color", "blue");
        assert!(!r2.contains("!important"), "got: {r2}");
    }


    #[test]
    fn display_keyword_valido() {
        // FlexWrap → "flex" (não "flexwrap" inválido); flex-wrap é prop separada.
        let dom = parse_html_to_dom(
            "<style>#a{display:flex;flex-wrap:wrap}</style><div id=\"a\">x</div>",
        );
        let a = dom.query("#a").unwrap();
        assert_eq!(dom.computed_property(a, "display"), "flex");
    }


    #[test]
    fn css_text_set_substitui_tudo() {
        let mut dom = parse_html_to_dom("<div id=\"a\" style=\"color: red\">x</div>");
        let a = dom.query("#a").unwrap();
        dom.set_css_text(a, "background: green; margin: 4px");
        assert_eq!(dom.inline_property(a, "color"), ""); // o color sumiu
        assert_eq!(dom.inline_property(a, "background-color"), "rgb(0, 128, 0)");
    }


    #[test]
    fn style_inserido_por_inner_html_entra_e_sai_da_cascade() {
        let mut dom = parse_html_to_dom("<div id='root'></div>");
        let root = dom.query("#root").unwrap();

        dom.set_inner_html(
            root,
            "<style>.x { color: #ff0000 }</style><p id='p' class='x'>x</p>",
        );
        let p = dom.query("#p").unwrap();
        assert_eq!(dom.computed_property(p, "color"), "rgb(255, 0, 0)");

        dom.set_inner_html(root, "<p id='p' class='x'>x</p>");
        let p = dom.query("#p").unwrap();
        assert_eq!(dom.computed_property(p, "color"), "rgb(0, 0, 0)");
    }

    #[test]
    fn style_embutido_removido_nao_apaga_css_externo() {
        let mut dom = parse_html_to_dom("<div id='root'></div>");
        dom.add_stylesheet(".x { color: #0000ff }");
        let root = dom.query("#root").unwrap();
        dom.set_inner_html(
            root,
            "<style>.x { color: #ff0000 }</style><p id='p' class='x'>x</p>",
        );
        let p = dom.query("#p").unwrap();
        assert_eq!(dom.computed_property(p, "color"), "rgb(255, 0, 0)");

        dom.set_inner_html(root, "<p id='p' class='x'>x</p>");
        let p = dom.query("#p").unwrap();
        assert_eq!(dom.computed_property(p, "color"), "rgb(0, 0, 255)");
    }
