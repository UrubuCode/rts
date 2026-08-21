    use super::*;

    /// Os bullets emitidos: quadrados pequenos e redondos.
    fn bullets(list: &DisplayList) -> Vec<(f32, f32)> {
        list.materialized()
            .iter()
            .filter_map(|i| match i {
                DisplayItem::SolidRect { rect, radius, .. }
                    if radius.tl > 0.0 && (rect.w - rect.h).abs() < 0.01 && rect.w < 12.0 =>
                {
                    Some((rect.x, rect.y))
                }
                _ => None,
            })
            .collect()
    }

    /// O marcador acompanha o item quando a subárvore dele é DESLOCADA — por um
    /// `transform:translate`, por `position`, ou por ser um item de flex/grid.
    ///
    /// O que isto fixa é uma PROPRIEDADE e não uma coordenada: o marcador cai à
    /// esquerda da caixa do seu item e dentro de um `em`, seja qual for a
    /// posição final dela. Um teste com números fixos falharia à próxima
    /// recalibração do medidor aproximado, que já houve duas esta semana.
    ///
    /// Existe porque um deslocamento neste motor NÃO reescreve os itens: um
    /// translate puro soma ao `dx`/`dy` do `ChildRef` que aponta para a
    /// subárvore (ver `layout.rs`, no bloco do transform). Um marcador emitido
    /// fora dessa subárvore ficaria parado enquanto a lista se move — o que
    /// estas formas todas provam que hoje não acontece.
    #[test]
    fn o_marcador_acompanha_o_item_quando_a_subarvore_e_deslocada() {
        let casos: &[(&str, &str)] = &[
            ("transform no pai", "<div style='transform:translate(200px,50px)'><ul><li>aa</li></ul></div>"),
            ("transform no ul", "<ul style='transform:translate(200px,50px)'><li>aa</li></ul>"),
            ("transform no li", "<ul><li style='transform:translate(200px,50px)'>aa</li></ul>"),
            ("transform no avo", "<div style='transform:translate(200px,0)'><div><div><ul><li>aa</li></ul></div></div></div>"),
            ("transform com irmao antes", "<div style='transform:translate(200px,0)'><p>x</p><ul><li>aa</li><li>bb</li></ul></div>"),
            ("absolute no pai", "<div style='position:absolute; left:200px; top:50px'><ul><li>aa</li></ul></div>"),
            ("absolute no li", "<div style='position:relative'><ul><li style='position:absolute; left:300px; top:100px'>aa</li></ul></div>"),
            ("fixed no li", "<ul><li style='position:fixed; left:300px; top:100px'>aa</li></ul>"),
            ("item de flex", "<ul style='display:flex'><li>aa</li><li>bb</li></ul>"),
            ("flex centrado", "<div style='display:flex; justify-content:center; width:600px'><div><ul><li>aa</li></ul></div></div>"),
            ("dentro de grid", "<div style='display:grid'><ul><li>aa</li></ul></div>"),
            ("dentro de celula", "<table><tr><td><ul><li>aa</li></ul></td></tr></table>"),
            ("li centrado por margin auto", "<ul><li style='width:100px; margin-left:auto; margin-right:auto'>aa</li></ul>"),
            ("contentor rolavel", "<div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li></ul></div>"),
        ];
        for (nome, html) in casos {
            // A caixa do item é o medidor: um fundo declarado, que é pintado no
            // border-box do `<li>` e sofre exatamente o mesmo deslocamento que a
            // subárvore dele. O TEXTO não serviria — `text-align` move-o dentro
            // da caixa e o marcador (que é `outside`) fica onde deve, à borda.
            let html = html
                .replace("<li style='", "<li style='background:#900;")
                .replace("<li>", "<li style='background:#900'>");
            let (_, list) = crate::table::tests::geometria(&html, 600.0);
            let caixas: Vec<(f32, f32)> = list
                .materialized()
                .iter()
                .filter_map(|i| match i {
                    DisplayItem::SolidRect { rect, color, .. } if *color == 0x9900_00FF => {
                        Some((rect.x, rect.y))
                    }
                    _ => None,
                })
                .collect();
            let pontos = bullets(&list);
            assert_eq!(pontos.len(), caixas.len(), "{nome}: um marcador por item");
            for (i, (&(bx, by), &(cx, cy))) in pontos.iter().zip(caixas.iter()).enumerate() {
                assert!(
                    bx < cx && cx - bx <= 16.0,
                    "{nome}[{i}]: marcador em {bx} devia estar à esquerda de {cx} e dentro de um em"
                );
                assert!(
                    (by - cy).abs() <= 16.0,
                    "{nome}[{i}]: marcador em y={by} longe da caixa em y={cy}"
                );
            }
        }
    }

    /// Os textos pintados — o que prova que o marcador textual existe.
    fn textos(html: &str) -> Vec<String> {
        let (_, list) = crate::table::tests::geometria(html, 600.0);
        list.materialized()
            .iter()
            .filter_map(|i| match i {
                DisplayItem::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect()
    }

    /// `list-style-image: none` NÃO é uma imagem — o marcador do `type` continua
    /// a ser desenhado.
    ///
    /// Este teste vale os 457 números que faltavam na página da Wikipédia. A
    /// folha dela tem `ol{…;list-style-image:none}`, e essa linha sozinha
    /// apagava o marcador de TODOS os `<ol>` do documento — com a numeração a
    /// funcionar por trás, que é o que tornava o defeito invisível: um `<ol>`
    /// isolado numerava, e por isso nenhum teste o apanhava.
    ///
    /// As três formas em que uma folha real escreve isto estão aqui, porque foi
    /// a variação entre elas que fez a busca demorar: a longa, a longa herdada
    /// do `<ol>` para o `<li>`, e o shorthand com dois `none`.
    #[test]
    fn list_style_image_none_nao_apaga_o_marcador() {
        for (nome, html) in [
            ("<ol> nu", "<ol><li>aa</li><li>bb</li></ol>"),
            (
                "image:none no ol (a regra da Wikipédia)",
                "<style>ol{list-style-image:none}</style><ol><li>aa</li><li>bb</li></ol>",
            ),
            (
                "image:none no li",
                "<style>ol li{list-style-image:none}</style><ol><li>aa</li><li>bb</li></ol>",
            ),
        ] {
            let t = textos(html);
            assert!(
                t.contains(&"1.".to_string()) && t.contains(&"2.".to_string()),
                "{nome}: os marcadores deviam estar lá — {t:?}"
            );
        }
    }

    /// `list-style: none none` continua a apagar o marcador — pelo TYPE.
    ///
    /// A folha da Wikipédia escreve-o assim em `.plainlist ol` e no índice, e é
    /// a forma que mais se parece com a que se acabou de corrigir. Aqui o
    /// marcador tem MESMO de desaparecer, e por outra razão: o primeiro `none`
    /// é um `list-style-type` válido. Se a correção de cima tivesse sido feita
    /// no shorthand em vez de na pergunta sobre a imagem, este caso passava a
    /// desenhar bullets onde a página não os tem.
    #[test]
    fn list_style_none_none_continua_a_apagar_pelo_type() {
        let t = textos("<style>ol{list-style:none none}</style><ol><li>aa</li></ol>");
        assert!(!t.contains(&"1.".to_string()), "{t:?}");
        let (_, list) =
            crate::table::tests::geometria("<style>ul{list-style:none none}</style><ul><li>aa</li></ul>", 600.0);
        assert_eq!(bullets(&list).len(), 0);
    }

    /// E o outro lado da mesma regra, que é o que impede a correção de ser um
    /// `is_some()` trocado por `true`: uma imagem A SÉRIO continua a substituir
    /// o marcador. Sem esta metade, "não apagar com `none`" e "nunca apagar"
    /// passariam os dois no teste de cima.
    #[test]
    fn uma_imagem_a_serio_continua_a_substituir_o_marcador() {
        let t = textos("<style>ol{list-style-image:url(p.png)}</style><ol><li>aa</li></ol>");
        assert!(!t.contains(&"1.".to_string()), "{t:?}");
        // e o bullet do `<ul>` também não é desenhado por baixo da imagem.
        let (_, list) =
            crate::table::tests::geometria("<style>ul{list-style-image:url(p.png)}</style><ul><li>aa</li></ul>", 600.0);
        assert_eq!(bullets(&list).len(), 0);
    }

    #[test]
    fn romano_cobre_os_subtrativos() {
        assert_eq!(roman(4).unwrap(), "IV");
        assert_eq!(roman(9).unwrap(), "IX");
        assert_eq!(roman(1994).unwrap(), "MCMXCIV");
        assert!(roman(0).is_none());
    }

    #[test]
    fn alfabetico_e_bijetivo_no_salto_de_z_para_aa() {
        assert_eq!(alphabetic(1, b'a'), "a");
        assert_eq!(alphabetic(26, b'a'), "z");
        assert_eq!(alphabetic(27, b'a'), "aa");
        assert_eq!(alphabetic(52, b'A'), "AZ");
        assert_eq!(alphabetic(53, b'A'), "BA");
    }

    /// Zerar o `padding-left` do `<ul>` põe o marcador FORA da caixa da lista, em
    /// coordenada negativa quando a lista encosta à margem esquerda.
    ///
    /// **É o que o Chrome faz**, e foi medido, não deduzido: um `<ul>` com
    /// `padding-left:0` encostado a `x=0` desenha o marcador em `x` negativo e
    /// ele simplesmente não aparece — sem `clamp` que o encoste à margem, sem o
    /// sobrepor ao texto do item, e sem abrir scroll horizontal
    /// (`scrollWidth == clientWidth`). O mesmo `<ul>` com `margin-left:100px`
    /// mostra-o a ~14px à esquerda do item, que é a mesma distância que este
    /// ficheiro usa.
    ///
    /// Por isso NÃO há aqui um `max(0.0)` a impedir o negativo: passaria neste
    /// teste e desenharia num sítio onde o browser não desenha. O marcador está
    /// ancorado ao content-box do `<li>` e o recuo onde ele cabe é o padding que
    /// o `<ul>` reserva — tirar o padding tira o sítio.
    ///
    /// Fica fixado porque é a única forma que faz o marcador sair da caixa do
    /// pai sem nada estar errado, e um `reset` de folha de estilo escreve
    /// exatamente isto. Quem investigar um marcador "no sítio errado" deve
    /// começar por perguntar se ele está em `x` negativo; nesse caso a pergunta
    /// é da camada que PINTA, que ao contrário do browser pode não estar a
    /// descartar o que cai fora.
    #[test]
    fn zerar_o_padding_do_ul_poe_o_marcador_fora_da_caixa_como_o_browser() {
        let (dom, list) = crate::table::tests::geometria("<ul><li>aa</li></ul>", 600.0);
        let ul = crate::table::tests::rect(&dom, &list, "ul", 0);
        let dentro = bullets(&list)[0];
        assert!(
            dentro.0 > ul.x,
            "com o padding da UA o marcador cabe dentro do <ul>: {} vs {}",
            dentro.0,
            ul.x
        );

        let (dom, list) =
            crate::table::tests::geometria("<ul style='padding-left:0'><li>aa</li></ul>", 600.0);
        let ul = crate::table::tests::rect(&dom, &list, "ul", 0);
        let fora = bullets(&list)[0];
        assert!(
            fora.0 < ul.x,
            "sem padding o marcador sai pela esquerda do <ul>: {} vs {}",
            fora.0,
            ul.x
        );
        assert!(fora.0 < 0.0, "e encostado à margem cai em x negativo: {}", fora.0);
    }

    /// O marcador continua colado ao seu item depois de a lista ser ACHATADA, e
    /// cai dentro dos mesmos clips que ele.
    ///
    /// O achatamento não é hipotético: a camada que pinta chama `materialize()`
    /// sempre que existe uma região com scroll próprio, para poder escrever o
    /// offset dentro do `BeginClip` dessa região — um `BeginClip` pode viver numa
    /// subárvore partilhada, e mutá-lo no lugar mexeria no desenho de todos os
    /// nós que a reusam. Ou seja: há um caminho que só corre quando há scroll de
    /// região e que reescreve `items` inteiro, e nenhuma medição de página
    /// parada passa por ele.
    ///
    /// A segunda metade é a que interessa e não se via na primeira: o offset da
    /// região é aplicado ao que está ENTRE o `BeginClip` e o `EndClip` dela. Um
    /// marcador do lado de fora dessa fronteira ficaria parado enquanto o item
    /// dele rola — o marcador a flutuar sobre o conteúdo, que é o sintoma. Por
    /// isso o teste compara a JANELA DE CLIPS de cada marcador com a do seu
    /// item, e não só as coordenadas.
    #[test]
    fn o_marcador_fica_nos_mesmos_clips_que_o_item_depois_de_achatar() {
        let casos: &[(&str, &str)] = &[
            ("lista simples", "<ul><li>aa</li><li>bb</li></ul>"),
            ("regiao rolavel", "<div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li><li>cc</li></ul></div>"),
            ("regiao rolavel deslocada", "<div style='transform:translate(200px,0)'><div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li><li>cc</li></ul></div></div>"),
            ("duas regioes", "<div style='overflow:auto; height:20px'><ul><li>aa</li><li>bb</li></ul></div><div style='overflow:auto; height:20px'><ul><li>cc</li><li>dd</li></ul></div>"),
            ("clip-path por cima", "<div style='clip-path:inset(0)'><div style='overflow:auto;height:20px'><ul><li>aa</li><li>bb</li></ul></div></div>"),
        ];
        for (nome, html) in casos {
            let html = html
                .replace("<li style='", "<li style='background:#900;")
                .replace("<li>", "<li style='background:#900'>");
            let (_, mut list) = crate::table::tests::geometria(&html, 600.0);
            let antes = marcadores_e_itens(&list);
            list.materialize();
            let depois = marcadores_e_itens(&list);
            assert_eq!(antes, depois, "{nome}: o achatamento mudou o desenho");

            // A janela de clips ABERTOS em cada índice da lista já plana.
            let mut abertos: Vec<usize> = Vec::new();
            let mut janela: Vec<Vec<usize>> = Vec::new();
            for (i, it) in list.items.iter().enumerate() {
                match it {
                    DisplayItem::BeginClip { .. } => {
                        janela.push(abertos.clone());
                        abertos.push(i);
                    }
                    DisplayItem::EndClip { .. } => {
                        abertos.pop();
                        janela.push(abertos.clone());
                    }
                    _ => janela.push(abertos.clone()),
                }
            }
            let mut caixas = Vec::new();
            let mut pontos = Vec::new();
            for (i, it) in list.items.iter().enumerate() {
                if let DisplayItem::SolidRect { rect, radius, color } = it {
                    if radius.tl > 0.0 && (rect.w - rect.h).abs() < 0.01 && rect.w < 12.0 {
                        pontos.push((rect.x, janela[i].clone()));
                    } else if *color == 0x9900_00FF {
                        caixas.push((rect.x, janela[i].clone()));
                    }
                }
            }
            assert_eq!(pontos.len(), caixas.len(), "{nome}: um marcador por item");
            for (i, ((bx, bj), (cx, cj))) in pontos.iter().zip(caixas.iter()).enumerate() {
                assert_eq!(
                    bj, cj,
                    "{nome}[{i}]: o marcador está noutra janela de clips que o item —                      o offset de scroll da região move um e não o outro"
                );
                assert!(bx < cx && cx - bx <= 16.0, "{nome}[{i}]: {bx} vs {cx}");
            }
        }
    }

    /// Os marcadores e as caixas dos itens, na ordem de pintura — o desenho que
    /// o achatamento não pode alterar.
    fn marcadores_e_itens(list: &DisplayList) -> Vec<(bool, u32, u32)> {
        list.materialized()
            .iter()
            .filter_map(|i| match i {
                DisplayItem::SolidRect { rect, radius, color } => {
                    if radius.tl > 0.0 && (rect.w - rect.h).abs() < 0.01 && rect.w < 12.0 {
                        Some((true, rect.x.to_bits(), rect.y.to_bits()))
                    } else if *color == 0x9900_00FF {
                        Some((false, rect.x.to_bits(), rect.y.to_bits()))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }
