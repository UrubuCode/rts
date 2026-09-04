//! Fluxo INLINE: a caixa de um inline, quebra de linha, `<br>`, links no meio
//! do parágrafo, e as fronteiras que não podem mudar o número de linhas.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de teste foi
//! alterada — a reconstrução destes blocos é byte a byte a do original.
//! A indentação de 4 espaços é a do `mod tests` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use super::*;

    #[test]
    fn elemento_inline_recebe_bounding_rect() {
        let dom = parse_html_to_dom("<div><span id='s'>texto</span></div>");
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let span = dom.query("#s").unwrap();
        let span_idx = dom.resolve(span).unwrap();
        let geo = list.geometry();
        let rect = geo.rects.get(&span_idx).expect("inline deveria ter rect");
        assert!(rect.w > 0.0);
        assert!(rect.h > 0.0);
    }

    /// Um `<a>` dentro de um parágrafo ocupa a sua fatia da linha: começa DEPOIS
    /// do texto que o antecede e é mais estreito do que o parágrafo inteiro.
    /// Sem isto respondia `0,0,0,0` — inexistente para hit-test e para medição.
    #[test]
    fn link_no_meio_do_paragrafo_tem_caixa_na_sua_fatia_da_linha() {
        let dom = parse_html_to_dom("<p>antes <a id='l'>link</a> depois</p>");
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#l").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("o <a> devia ter caixa");
        assert!(
            r.x > 0.0,
            "começa depois do texto que o antecede: x={}",
            r.x
        );
        assert!(
            r.w > 0.0 && r.w < 800.0,
            "largura da sua fatia, não a do <p>: w={}",
            r.w
        );
        assert!(r.h > 0.0);
    }

    /// Um `<a>` que quebra em duas linhas tem UMA caixa que contém as duas — é a
    /// definição da spec (bounding box dos fragmentos), e é o que
    /// `getBoundingClientRect` devolve no browser.
    #[test]
    fn link_partido_em_duas_linhas_tem_caixa_que_contem_as_duas() {
        let texto = "palavra ".repeat(40);
        let dom = parse_html_to_dom(&format!("<p><a id='l'>{texto}</a></p>"));
        let ctx = LayoutCtx {
            viewport_w: 200.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#l").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("o <a> devia ter caixa");
        // várias linhas: a altura é a soma delas, muito acima de uma linha só.
        let uma_linha = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        assert!(
            r.h > uma_linha * 2.0,
            "devia abranger várias linhas: h={}",
            r.h
        );
        // e a união é larga como a coluna, não como o último fragmento.
        assert!(
            r.w > 100.0,
            "a união abrange a largura da coluna: w={}",
            r.w
        );
    }

    /// Um `<img>` inline é um REPLACED element: tem caixa própria com a largura e
    /// a altura dos atributos, mesmo sem pixels descodificados (é o caso de uma
    /// página real medida sem rede). Antes não gerava run nenhum e ficava a zero.
    #[test]
    fn img_inline_tem_caixa_propria_dos_atributos() {
        let dom = parse_html_to_dom("<p>antes <img id='i' width='40' height='30'> depois</p>");
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#i").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("o <img> devia ter caixa");
        assert_eq!((r.w, r.h), (40.0, 30.0));
        assert!(r.x > 0.0, "está depois do texto que o antecede: x={}", r.x);
    }

    /// Um inline VAZIO (`<source>` dentro de um `<picture>`) tem POSIÇÃO (a
    /// caixa existe, na coordenada da linha) mas NENHUMA área — 0×0, como o
    /// Blink real dá a qualquer inline sem fragmento.
    ///
    /// CORRIGIDO (medido no Blink, `claude-sel-has.html`): a versão anterior
    /// fixava `r.h > 0.0` — a altura do strut vazando para um elemento sem
    /// conteúdo nenhum, o mesmo defeito de `inline_vazio_nao_tem_caixa`.
    #[test]
    fn inline_vazio_tem_posicao_sem_area() {
        let dom = parse_html_to_dom(
            "<p>antes <picture><source id='s'><img width='40' height='30'></picture></p>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#s").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("o <source> devia ter caixa (sem área, mas registada)");
        assert_eq!((r.w, r.h), (0.0, 0.0), "sem largura nem altura: {r:?}");
        assert!(r.x > 0.0, "está depois do texto que o antecede: x={}", r.x);
    }

    /// Um inline vazio SOZINHO num bloco não inventa uma linha: o bloco continua
    /// com a mesma altura. É o corte que separa "acrescentar geometria" de
    /// "mudar o layout".
    #[test]
    fn inline_vazio_sozinho_nao_cria_linha() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let com = layout_document(&parse_html_to_dom("<div><span></span></div>"), &ctx);
        let sem = layout_document(&parse_html_to_dom("<div></div>"), &ctx);
        assert_eq!(com.content_height, sem.content_height);
    }

    /// Um inline com CAIXA (fundo/padding) no meio de um parágrafo continua na
    /// linha: o parágrafo tem a altura de uma linha, não de três. Era o que
    /// multiplicava a altura de uma página real — cada `<span>` com fundo
    /// fechava o fluxo inline e abria linha nova.
    #[test]
    fn inline_com_caixa_nao_parte_a_linha() {
        let ctx = LayoutCtx {
            viewport_w: 1280.0,
            viewport_h: 800.0,
            measurer: &ApproxMeasurer,
        };
        let linhas = |html: &str| {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let mut ys: Vec<f32> = Vec::new();
            list.walk(|it, _dx, dy| {
                if let DisplayItem::Text { y, .. } = it {
                    ys.push(y + dy);
                }
            });
            ys.sort_by(f32::total_cmp);
            ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
            ys.len()
        };
        assert_eq!(linhas("<p>antes <span>simples</span> depois</p>"), 1);
        assert_eq!(
            linhas("<p>antes <span style='background:#eee'>com caixa</span> depois</p>"),
            1
        );
    }

    /// `<br>` fecha a linha: o texto depois dele começa uma linha nova, e o
    /// próprio `<br>` tem posição e altura de linha com largura zero — é o que o
    /// browser reporta. Antes não quebrava nada: as duas linhas saíam como uma.
    #[test]
    fn br_quebra_a_linha_e_tem_a_sua_caixa() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let dom = parse_html_to_dom("<div>antes<br id='b'>depois</div>");
        let list = layout_document(&dom, &ctx);
        let mut ys: Vec<f32> = Vec::new();
        list.walk(|it, _dx, dy| {
            if let DisplayItem::Text { y, .. } = it {
                ys.push(y + dy);
            }
        });
        ys.sort_by(f32::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        assert_eq!(ys.len(), 2, "duas linhas, não uma: {ys:?}");
        let idx = dom.resolve(dom.query("#b").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("o <br> devia ter caixa");
        assert_eq!(r.w, 0.0);
        assert!(r.h > 0.0, "altura de linha: {r:?}");
    }

    /// A caixa de um elemento inline é a CONTENT AREA DA FONTE, não a caixa de
    /// linha: com `line-height: 3`, o `<a>` continua a ter a altura da fonte e
    /// fica CENTRADO na linha pela meia-entrelinha. O `line-height` decide o
    /// espaçamento (onde a linha seguinte começa), não o tamanho do inline.
    ///
    /// Dar-lhe a altura da linha somava ~8px por elemento numa página com
    /// `line-height: 26px` — 3 032 `<a>` na Wikipédia, ~24 500px de excesso.
    #[test]
    fn caixa_do_inline_e_a_altura_da_fonte_nao_a_da_linha() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let fonte = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma y absoluto.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style><div style='line-height:3'>antes <a id='l'>link</a> depois</div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#l").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("o <a> devia ter caixa");
        assert_eq!(r.h, fonte, "a altura da FONTE, não os 48 da linha");
        // meia-entrelinha: (48 − 18) / 2 = 15 acima.
        assert_eq!(
            r.y,
            (3.0 * DEFAULT_FONT_SIZE - fonte) / 2.0,
            "centrado na linha: {r:?}"
        );
    }

    #[test]
    fn tmp_a() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        for html in [
            "<div style='line-height:1.625'>t <a id='x'>link</a> f</div>",
            "<div style='line-height:1.625'><a id='x'>link</a></div>",
            "<div style='line-height:1.625'><a id='x' style='color:#06c'>link</a> f</div>",
            "<li style='line-height:1.625'><a id='x'>link</a></li>",
            "<div style='line-height:1.625'><a id='x' style='padding:2px'>link</a></div>",
            "<div style='line-height:1.625'><a id='x'><img width='20' height='15'></a></div>",
        ] {
            let dom = parse_html_to_dom(html);
            let list = layout_document(&dom, &ctx);
            let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
            eprintln!("DIAG {:<66} -> {:?}", html, list.geometry().rects.get(&idx));
        }
    }

    /// `border-radius` sozinho NÃO tira um elemento do fluxo inline: um raio sem
    /// fundo nem borda não pinta nada e não cria caixa. O `has_box` conta-o
    /// porque responde a outra pergunta ("há algo a pintar por este caminho").
    ///
    /// Não é um caso de laboratório: 5 262 dos 5 263 `<a>` da Wikipédia eram
    /// blockificados só por isto, e por isso nenhuma correção do fluxo inline
    /// lhes tocava — eles nunca lá chegavam.
    #[test]
    fn radius_sozinho_nao_tira_o_elemento_do_fluxo_inline() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let fonte = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        let dom = parse_html_to_dom(
            "<div style='line-height:2'>t <a id='x' style='border-radius:2px'>link</a> f</div>",
        );
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#x").unwrap()).unwrap();
        let r = *list
            .geometry()
            .rects
            .get(&idx)
            .expect("o <a> devia ter caixa");
        // Se tivesse sido blockificado, a altura seria a da LINHA (32) e o x seria
        // zero (caixa própria a começar na margem do bloco).
        assert_eq!(r.h, fonte, "altura da fonte, logo continua inline: {r:?}");
        assert!(r.x > 0.0, "flui depois do texto que o antecede: {r:?}");
    }

    /// Um `<a>` à volta de uma imagem grande NÃO fica do tamanho dela: no browser
    /// a caixa de um inline tem a LARGURA do que ele contém e a ALTURA DA FONTE.
    ///
    /// Medido na Wikipédia antes de o corpus mudar: um `<a>` com uma imagem de
    /// 600x528 responde `600x17` no Chrome, com o topo a 254px do topo da
    /// imagem — que é a meia-entrelinha da linha que a imagem tornou alta.
    /// Nós dávamos-lhe os 528, e era o maior erro de altura da página inteira.
    #[test]
    fn inline_a_volta_de_uma_imagem_mantem_a_altura_da_fonte() {
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let fonte = ApproxMeasurer.line_height(DEFAULT_FONT_SIZE);
        // body{margin:0}: a folha de UA (lote I) dá 8px ao body; este teste
        // afirma y absoluto.
        let dom = parse_html_to_dom(
            "<style>body{margin:0}</style><div><span id='s'><a id='a'><img id='i' width='300' height='200'></a></span></div>",
        );
        let list = layout_document(&dom, &ctx);
        let geo = list.geometry();
        let caixa = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            *geo.rects
                .get(&idx)
                .unwrap_or_else(|| panic!("{sel} sem caixa"))
        };
        let (img, a, span) = (caixa("#i"), caixa("#a"), caixa("#s"));
        assert_eq!((img.w, img.h), (300.0, 200.0), "a imagem tem a sua caixa");
        assert_eq!(a.h, fonte, "o <a> mede a fonte, não a imagem: {a:?}");
        assert_eq!(a.w, 300.0, "mas ocupa a largura dela na linha: {a:?}");
        assert_eq!((span.w, span.h), (a.w, a.h), "e o <span> à volta, o mesmo");
        // e fica centrado na linha que a imagem tornou alta.
        assert_eq!(a.y, (200.0 - fonte) / 2.0, "meia-entrelinha: {a:?}");
    }

    #[test]
    fn inline_flow_links_fluem_no_paragrafo() {
        // P4 (o coracao): <p>texto <a>link</a>, fim</p> flui numa UNICA linha —
        // antes cada filho virava linha propria (o footer do cover saia em 5
        // linhas). A pontuacao NAO ganha espaco (fiel ao fonte: "Bootstrap, by").
        crate::block::define(
            "p",
            crate::block::BlockDef {
                display: 0,
                indent: 0.0,
                prefix: 0,
                flags: 0,
            },
        );
        let list = layout(
            "<p style='color:#ffffff'>Cover template for <a style='color:#ff0000'>Bootstrap</a>, by <a style='color:#00ff00'>mdo</a>.</p>",
            600.0,
        );
        let texts: Vec<(String, f32, f32, u32)> = all_texts(&list);
        // TODOS os segmentos na MESMA linha (y igual).
        let y0 = texts[0].2;
        assert!(
            texts.iter().all(|(_, _, y, _)| *y == y0),
            "uma linha so: {texts:?}"
        );
        // os segmentos avancam em x (fluem lado a lado) e preservam a cor do span.
        assert!(texts.len() >= 4, "segmentos por cor: {texts:?}");
        assert!(
            texts.windows(2).all(|w| w[1].1 > w[0].1),
            "x crescente: {texts:?}"
        );
        assert_eq!(texts[1].3, 0xFF0000FF, "cor do link preservada");
        // a virgula gruda no link (segmento seguinte comeca com ',' sem espaco).
        assert!(
            texts[2].0.starts_with(','),
            "pontuacao sem espaco: {:?}",
            texts[2].0
        );
    }

    /// Uma frase comprida, para forçar várias linhas com o `ApproxMeasurer`
    /// (0,5 × font-size por carácter): 16px × 0,5 = 8pt por carácter.
    const FRASE: &str = "alfa beta gama delta epsilon zeta eta teta iota kapa lambda mi ni xi omicron pi ro sigma tau upsilon fi qui psi omega";

    #[test]
    fn referencia_nao_e_partida_ao_meio_por_uma_quebra_de_linha() {
        // A marcação de referência da Wikipédia: `[`, `135`, `]` em spans
        // separados, SEM espaço entre eles — não há ali oportunidade de quebra
        // nenhuma, logo os três descem juntos em vez de o `[` ficar para trás.
        let list = layout(
            // A 16px/0,5 cada letra mede 8 e o espaço 8: quatro "aaaa" com os
            // espaços ocupam 152 dos 200. O quinto "aaaa" mais o `[135]` são um
            // aglomerado de 72 (não há espaço entre a palavra e a referência),
            // que pede 8+72 e não cabe — desce INTEIRO, como no browser.
            "<p style='width:200'>aaaa aaaa aaaa aaaa aaaa<span>[</span><span>135</span><span>]</span></p>",
            600.0,
        );
        let t = all_texts(&list);
        let y_de = |txt: &str| {
            t.iter()
                .find(|(s, _, _, _)| s == txt)
                .map(|(_, _, y, _)| *y)
        };
        let (abre, num, fecha) = (y_de("["), y_de("135"), y_de("]"));
        assert_eq!(abre, num, "o `[` não fica para trás do número: {t:?}");
        assert_eq!(num, fecha, "nem o `]` para a frente: {t:?}");
        // e o aglomerado desceu com a palavra a que está colado, deixando as
        // quatro primeiras na linha de cima.
        let primeira = t.first().map(|(_, _, y, _)| *y);
        assert_ne!(abre, primeira, "o conjunto desceu de linha: {t:?}");
        for (_, x, y, _) in &t {
            if Some(*y) == abre {
                assert!(*x < 200.0, "e não transborda a caixa: {t:?}");
            }
        }
    }

    #[test]
    fn fronteiras_inline_nao_mudam_o_numero_de_linhas() {
        // O MESMO texto, uma vez solto e outra partido por fronteiras de
        // elemento inline: o número de linhas tem de ser o mesmo, porque uma
        // fronteira não acrescenta nem remove conteúdo.
        //
        // Nasceu de uma hipótese REFUTADA — que o vão do espaço entre um texto e
        // o inline seguinte não entrava na largura da linha, dando uma linha de
        // graça a cada fronteira. Não entra mesmo em `cur_w`, mas o
        // `collapse_ws` prefixa o espaço e o `push_segment` separa-o preservando
        // a largura, portanto a conta fecha. Fica a pinar o que se provou já
        // funcionar: é dos sítios onde um defeito passaria despercebido, porque
        // o sintoma seria um parágrafo com a altura errada e não uma linha
        // visivelmente torta.
        let palavras: Vec<String> = (0..60).map(|i| format!("pal{i:02}")).collect();
        let solto = palavras.join(" ");
        let partido = palavras
            .iter()
            .map(|w| format!("<a>{w}</a>"))
            .collect::<Vec<_>>()
            .join(" ");
        let linhas = |html: &str| -> usize {
            let list = layout(&format!("<p style='width:400'>{html}</p>"), 600.0);
            let mut ys: Vec<f32> = all_texts(&list).iter().map(|(_, _, y, _)| *y).collect();
            ys.sort_by(f32::total_cmp);
            ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
            ys.len()
        };
        let (a, b) = (linhas(&solto), linhas(&partido));
        assert_eq!(a, b, "solto={a} linhas, partido por <a>={b} linhas");
    }
