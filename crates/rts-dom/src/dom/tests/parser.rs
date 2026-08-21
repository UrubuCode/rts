//! O parser de HTML: tags implícitas, void, entidades, comentários, `dump`.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de teste foi alterada.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! vários testes têm literais de string multi-linha em que o espaço à
//! esquerda é conteúdo, e desindentar mudaria o que eles afirmam.

    use super::*;


    #[test]
    fn parser_preserva_comentarios() {
        // DOM fiel: <!-- --> vira nó Comment (nodeType 8), não é descartado.
        let dom = parse_html_to_dom("<div><!-- nota --><p>oi</p></div>");
        let div = dom.query("div").unwrap();
        let kids = dom.child_nodes(div); // childNodes inclui o comentário
        assert_eq!(kids.len(), 2); // Comment + <p>
        assert_eq!(dom.node_type(kids[0]), 8); // Comment
        assert_eq!(dom.node_name(kids[0]).as_deref(), Some("#comment"));
        assert_eq!(dom.node_type(kids[1]), 1); // <p>
        // o `>` DENTRO do comentário não encerra cedo:
        let dom2 = parse_html_to_dom("<!-- a > b --><span>x</span>");
        let span = dom2.query("span").unwrap();
        assert_eq!(dom2.node_type(span), 1); // span foi parseado corretamente
    }


    #[test]
    fn atributos_class_id_href_preservados() {
        let dom =
            parse_html_to_dom("<div class='card' id=\"alvo\"><a href='https://x'>l</a></div>");
        let div = topo(&dom)[0];
        assert_eq!(dom.node(div).attr("class"), Some("card"));
        assert_eq!(dom.node(div).attr("id"), Some("alvo"));
        assert_eq!(dom.node(div).attr("naoexiste"), None);
        let a = dom.node(div).children[0];
        assert_eq!(tag(&dom, a), "a");
        assert_eq!(dom.node(a).attr("href"), Some("https://x"));
    }


    #[test]
    fn atributos_variantes_aspas_e_booleano() {
        // aspas duplas, simples, sem aspas, e atributo sem valor.
        let dom = parse_html_to_dom("<input type=text value='oi' disabled checked=\"x\">");
        let inp = topo(&dom)[0];
        assert_eq!(dom.node(inp).attr("type"), Some("text")); // sem aspas
        assert_eq!(dom.node(inp).attr("value"), Some("oi")); // aspas simples
        assert_eq!(dom.node(inp).attr("disabled"), Some("")); // booleano
        assert_eq!(dom.node(inp).attr("checked"), Some("x")); // aspas duplas
        // `input` é void: não empilha, não tem filhos.
        assert!(dom.node(inp).children.is_empty());
    }


    #[test]
    fn valor_de_atributo_decodifica_entidades() {
        let dom = parse_html_to_dom("<a title='Tom &amp; Jerry'>x</a>");
        let a = topo(&dom)[0];
        assert_eq!(dom.node(a).attr("title"), Some("Tom & Jerry"));
    }


    #[test]
    fn dump_mostra_atributos() {
        let dom = parse_html_to_dom("<div class='card' id='x'>oi</div>");
        // O dump mostra as três tags que o browser cria — `html`, `body` e o
        // elemento escrito. A expectativa muda porque a ESTRUTURA de topo é o
        // que este teste imprime; sem `<body>` na árvore uma regra `body{…}`
        // não casava com elemento nenhum e toda a propriedade herdada
        // declarada aí desaparecia em silêncio.
        let esperado = "\
#document
  <html>
    <body>
      <div class=\"card\" id=\"x\">
        \"oi\"
";
        assert_eq!(dom.dump(), esperado);
    }


    #[test]
    fn arvore_simples_heading_e_paragrafo() {
        let dom = parse_html_to_dom("<h1>Titulo</h1><p>Corpo</p>");
        // 2 filhos de topo do fluxo: h1 e p — hoje sob o `<body>` implícito.
        let top = topo(&dom);
        assert_eq!(top.len(), 2);
        assert_eq!(tag(&dom, top[0]), "h1");
        assert_eq!(tag(&dom, top[1]), "p");
        // h1 tem um único filho de texto "Titulo".
        let h1_kids = &dom.node(top[0]).children;
        assert_eq!(h1_kids.len(), 1);
        assert_eq!(dom.node(h1_kids[0]).kind, NodeKind::Text("Titulo".into()));
    }


    #[test]
    fn inline_aninhado_vira_subarvore() {
        // <b> com <i> dentro precisa virar b → i → texto (aninhamento real).
        let dom = parse_html_to_dom("<p>a <b>forte <i>e it</i></b> z</p>");
        let p = topo(&dom)[0];
        assert_eq!(tag(&dom, p), "p");
        let pk = &dom.node(p).children;
        // p: "a ", <b>, " z"
        assert_eq!(pk.len(), 3);
        assert_eq!(dom.node(pk[0]).kind, NodeKind::Text("a ".into()));
        assert_eq!(tag(&dom, pk[1]), "b");
        assert_eq!(dom.node(pk[2]).kind, NodeKind::Text(" z".into()));
        // <b>: "forte ", <i>
        let bk = &dom.node(pk[1]).children;
        assert_eq!(bk.len(), 2);
        assert_eq!(dom.node(bk[0]).kind, NodeKind::Text("forte ".into()));
        assert_eq!(tag(&dom, bk[1]), "i");
        // <i>: "e it"
        assert_eq!(dom.node(bk[1]).children.len(), 1);
    }


    #[test]
    fn cada_no_conhece_o_pai() {
        let dom = parse_html_to_dom("<p><b>x</b></p>");
        let p = topo(&dom)[0];
        let b = dom.node(p).children[0];
        let x = dom.node(b).children[0];
        assert_eq!(dom.node(p).parent, Some(body_idx(&dom)));
        assert_eq!(dom.node(b).parent, Some(p));
        assert_eq!(dom.node(x).parent, Some(b));
    }


    #[test]
    fn tag_desconhecida_e_preservada_como_no() {
        // No caminho de fila <span> some; na árvore ele PERSISTE como elemento.
        let dom = parse_html_to_dom("<p>oi <span>spn</span> tchau</p>");
        let p = topo(&dom)[0];
        let pk = &dom.node(p).children;
        assert_eq!(pk.len(), 3);
        assert_eq!(tag(&dom, pk[1]), "span");
        assert_eq!(dom.node(pk[1]).children.len(), 1);
    }


    #[test]
    fn entidades_decodificadas() {
        let dom = parse_html_to_dom("<p>a &lt; b &amp; c &gt; d</p>");
        let p = topo(&dom)[0];
        let txt = dom.node(dom.node(p).children[0]).kind.clone();
        assert_eq!(txt, NodeKind::Text("a < b & c > d".into()));
    }


    #[test]
    fn fechamento_orfao_nao_quebra() {
        // </div> sem abertura é ignorado; texto ao redor preservado.
        let dom = parse_html_to_dom("</div><p>ok</p>");
        let top = topo(&dom);
        assert_eq!(top.len(), 1);
        assert_eq!(tag(&dom, top[0]), "p");
    }


    #[test]
    fn void_tag_nao_empilha() {
        // <br> não tem fechamento; o <p> seguinte deve ser irmão, não filho.
        let dom = parse_html_to_dom("<br><p>depois</p>");
        let top = topo(&dom);
        assert_eq!(top.len(), 2);
        assert_eq!(tag(&dom, top[0]), "br");
        assert_eq!(tag(&dom, top[1]), "p");
        assert!(dom.node(top[0]).children.is_empty());
    }


    #[test]
    fn dump_legivel_para_inspecao() {
        let dom = parse_html_to_dom("<h1>Oi</h1><p>antes <b>forte</b></p>");
        // `<html>`/`<body>` implícitos, como em qualquer browser — ver
        // `body_idx` para o defeito que a sua ausência causava.
        let esperado = "\
#document
  <html>
    <body>
      <h1>
        \"Oi\"
      <p>
        \"antes \"
        <b>
          \"forte\"
";
        assert_eq!(dom.dump(), esperado);
    }


    // ── Parser para páginas REAIS: DOCTYPE, `>` em atributo, void tags, ──────────
    // ── auto-fechamento implícito (HTML5 tag omission) ───────────────────────────

    #[test]
    fn doctype_nao_vira_elemento() {
        // Antes, `<!DOCTYPE html>` virava `Element { tag: "!doctype" }` que
        // EMPILHAVA na pilha de abertos (a "tag" nunca fecha) — o documento
        // INTEIRO aninhava como filho dele. Agora o tokenizador ignora `<!…>`
        // (não modelamos DocumentType, nodeType 10 fora do escopo) e `html`
        // é filho direto do #document.
        let dom = parse_html_to_dom("<!DOCTYPE html><html><body><p>x</p></body></html>");
        let html_el = dom.query("html").unwrap();
        assert_eq!(dom.parent_of(html_el).map(|p| idx(&dom, p)), Some(dom.root));
        // único elemento de topo — nada de "!doctype" fantasma na raiz.
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 1);
        assert_eq!(tag(&dom, top[0]), "html");
        // body é filho do html, e o texto chega intacto.
        let body = dom.query("body").unwrap();
        assert_eq!(dom.parent_of(body), Some(html_el));
        assert_eq!(dom.text_content(dom.query("p").unwrap()).unwrap(), "x");
    }


    #[test]
    fn atributo_com_maior_que_no_valor() {
        // `<div title="a>b">`: o `>` dentro do valor com aspas não termina a
        // tag — antes o tokenizador cortava no primeiro `>` cru, o atributo
        // vinha truncado (`title="a`) e `b">` vazava como texto.
        let dom = parse_html_to_dom(r#"<div title="a>b">x</div>"#);
        let div = dom.query("div").unwrap();
        let n = dom.node(idx(&dom, div));
        assert_eq!(n.attr("title"), Some("a>b"));
        assert_eq!(dom.text_content(div).unwrap(), "x");
        // um único elemento de topo (nenhuma tag-fantasma criada pela quebra).
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 1);
    }


    #[test]
    fn void_tags_completas_nao_empilham() {
        // `source`/`track` (e as demais void novas: area/base/col/embed/wbr)
        // não têm fechamento — se empilhassem, o `</video>` não casaria e o
        // `<p>` seguinte viraria DESCENDENTE do video em vez de irmão.
        let dom = parse_html_to_dom("<video><source src=\"a.mp4\"><track></video><p>x</p>");
        let video = dom.query("video").unwrap();
        let p = dom.query("p").unwrap();
        let source = dom.query("source").unwrap();
        let track = dom.query("track").unwrap();
        // source/track são filhos do video (não empilham nem engolem irmãos)…
        assert_eq!(dom.parent_of(source), Some(video));
        assert_eq!(dom.parent_of(track), Some(video));
        // …e p é IRMÃO do video (filho do `<body>` implícito), não descendente.
        assert_eq!(dom.parent_of(p).map(|x| idx(&dom, x)), Some(body_idx(&dom)));
        assert_eq!(dom.next_sibling(video), Some(p));
    }


    #[test]
    fn li_fecha_li_implicito() {
        // HTML5 tag omission: um `<li>` novo fecha o `<li>` corrente — os dois
        // viram IRMÃOS dentro do `<ul>` (antes o segundo aninhava no primeiro).
        let dom = parse_html_to_dom("<ul><li>a<li>b</ul>");
        let ul = dom.query("ul").unwrap();
        let kids = dom.child_nodes(ul);
        assert_eq!(kids.len(), 2);
        assert_eq!(tag(&dom, idx(&dom, kids[0])), "li");
        assert_eq!(tag(&dom, idx(&dom, kids[1])), "li");
        assert_eq!(dom.text_content(kids[0]).unwrap(), "a");
        assert_eq!(dom.text_content(kids[1]).unwrap(), "b");
    }


    #[test]
    fn dt_dd_e_option_implicitos() {
        // <dt>/<dd> fecham o dt/dd corrente (termo e definição são irmãos)…
        let dom = parse_html_to_dom("<dl><dt>t<dd>d</dl>");
        let dl = dom.query("dl").unwrap();
        let kids = dom.child_nodes(dl);
        assert_eq!(kids.len(), 2);
        assert_eq!(tag(&dom, idx(&dom, kids[0])), "dt");
        assert_eq!(tag(&dom, idx(&dom, kids[1])), "dd");
        // …e <option> fecha o option corrente.
        let dom2 = parse_html_to_dom("<select><option>1<option>2</select>");
        let sel = dom2.query("select").unwrap();
        let opts = dom2.child_nodes(sel);
        assert_eq!(opts.len(), 2);
        assert_eq!(dom2.text_content(opts[0]).unwrap(), "1");
        assert_eq!(dom2.text_content(opts[1]).unwrap(), "2");
    }


    #[test]
    fn p_fecha_p_e_bloco_fecha_p() {
        // `<p>a<p>b`: p nunca aninha em p — o segundo fecha o primeiro (irmãos).
        let dom = parse_html_to_dom("<p>a<p>b");
        let p1 = dom.query("p").unwrap();
        let p2 = dom.next_sibling(p1).expect("segundo <p> deveria ser irmão");
        assert_eq!(dom.text_content(p1).unwrap(), "a");
        assert_eq!(dom.text_content(p2).unwrap(), "b");
        // `<p>texto<div>`: a regra do HTML5 que MAIS aparece em páginas reais —
        // a abertura de um elemento de bloco fecha o <p> aberto.
        let dom2 = parse_html_to_dom("<p>texto<div>x</div>");
        let p = dom2.query("p").unwrap();
        let div = dom2.query("div").unwrap();
        assert_eq!(
            dom2.parent_of(div).map(|x| idx(&dom2, x)),
            Some(body_idx(&dom2))
        );
        assert_eq!(dom2.next_sibling(p), Some(div));
        assert_eq!(dom2.text_content(p).unwrap(), "texto"); // o "x" NÃO entrou no p
    }


    #[test]
    fn tabela_com_td_tr_implicitos() {
        // `<td>` fecha a célula corrente; `<tr>` fecha a célula E o tr do topo.
        // (Divergência consciente da spec: não sintetizamos `<tbody>` — os tr
        // ficam filhos diretos do table.)
        let dom = parse_html_to_dom("<table><tr><td>a<td>b<tr><td>c</table>");
        let table = dom.query("table").unwrap();
        let trs = dom.child_nodes(table);
        assert_eq!(trs.len(), 2);
        assert_eq!(tag(&dom, idx(&dom, trs[0])), "tr");
        assert_eq!(tag(&dom, idx(&dom, trs[1])), "tr");
        let tds1 = dom.child_nodes(trs[0]);
        assert_eq!(tds1.len(), 2);
        assert_eq!(dom.text_content(tds1[0]).unwrap(), "a");
        assert_eq!(dom.text_content(tds1[1]).unwrap(), "b");
        let tds2 = dom.child_nodes(trs[1]);
        assert_eq!(tds2.len(), 1);
        assert_eq!(dom.text_content(tds2[0]).unwrap(), "c");
    }


    #[test]
    fn li_novo_nao_fecha_li_de_lista_ancestral() {
        // O fechamento implícito só olha o TOPO da pilha: o `<li>` de uma
        // sublista NÃO fecha o `<li>` do `<ul>` ancestral (o topo ali é `ul`,
        // nada casa). Fechar "através" do container colapsaria a sublista.
        let dom = parse_html_to_dom("<ul><li>a<ul><li>b</li></ul></li></ul>");
        let outer_ul = dom.query("ul").unwrap();
        let outer_kids = dom.child_nodes(outer_ul);
        assert_eq!(outer_kids.len(), 1); // só o li "a…"
        let li_a = outer_kids[0];
        assert_eq!(tag(&dom, idx(&dom, li_a)), "li");
        // dentro do li: o texto "a" + a sublista com o li "b".
        let inner_ul = dom
            .child_nodes(li_a)
            .into_iter()
            .find(|&k| dom.node_type(k) == 1)
            .expect("sublista deveria estar DENTRO do li externo");
        assert_eq!(tag(&dom, idx(&dom, inner_ul)), "ul");
        let inner_kids = dom.child_nodes(inner_ul);
        assert_eq!(inner_kids.len(), 1);
        assert_eq!(dom.text_content(inner_kids[0]).unwrap(), "b");
    }


    #[test]
    fn parser_preserva_whitespace_e_descarta_atributos_duplicados() {
        let dom = parse_html_to_dom("<div><span>A</span> <span>B</span></div>");
        let div = dom.query("div").unwrap();
        assert_eq!(dom.child_nodes(div).len(), 3);

        let dup = parse_html_to_dom("<div id='first' id='second'></div>");
        let element = dup.query("div").unwrap();
        let idx = idx(&dup, element);
        assert_eq!(dup.node(idx).attrs.len(), 1);
        assert_eq!(dup.node(idx).attr("id"), Some("first"));
    }


    #[test]
    fn pagina_real_bootstrap_cover() {
        // Valida contra uma página REAL (Bootstrap 5.3 "cover": `<!doctype html>`
        // minúsculo, tags multi-linha, `<meta/>`/`<link/>` autofecháveis, <svg>,
        // <style> longo): `html` deve ser filho DIRETO do #document — o doctype
        // não pode virar elemento que aninha o documento — e head/body filhos
        // de html. O corpus vive em `examples/` (ainda não versionado); se
        // ausente (ex.: CI antes do corpus entrar), o teste é um no-op EXPLÍCITO.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/bootstrap-5.3.8-examples/cover/index.html"
        );
        let Ok(src) = std::fs::read_to_string(path) else {
            eprintln!("corpus real ausente ({path}) — validação da página pulada");
            return;
        };
        let dom = parse_html_to_dom(&src);
        let html_el = dom.query("html").unwrap();
        assert_eq!(dom.parent_of(html_el).map(|p| idx(&dom, p)), Some(dom.root));
        // único elemento de topo (sem "!doctype" fantasma).
        let top: Vec<_> = dom
            .node(dom.root)
            .children
            .iter()
            .filter(|&&c| matches!(dom.node(c).kind, NodeKind::Element { .. }))
            .collect();
        assert_eq!(top.len(), 1);
        // head e body são filhos de html.
        let head = dom.query("head").unwrap();
        let body = dom.query("body").unwrap();
        assert_eq!(dom.parent_of(head), Some(html_el));
        assert_eq!(dom.parent_of(body), Some(html_el));
        // conteúdo real chegou: o h1 do template.
        let h1 = dom.query("h1").unwrap();
        assert_eq!(dom.text_content(h1).unwrap(), "Cover your page.");
    }
