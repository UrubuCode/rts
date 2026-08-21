//! CONTEÚDO GERADO — `::before` e `::after`.
//!
//! Um pseudo-elemento não é um nó do DOM: é uma caixa que a cascata manda
//! existir. Isso obriga a uma decisão de arquitetura logo à entrada, e a que
//! está tomada aqui é: **nada é acrescentado à árvore de nós**. Um `::before`
//! não aparece em `childNodes`, `childCount`, `querySelectorAll` nem no
//! `innerHTML`, exatamente como no browser — e a forma de garantir isso não é
//! filtrar em cada uma dessas consultas (que seria preciso lembrar em todas as
//! futuras), mas nunca criar o nó.
//!
//! A alternativa rejeitada foi a do Blink, que cria um `PseudoElement` real
//! ligado ao elemento originante e o mantém fora da lista de filhos. Faz
//! sentido lá, onde a árvore de layout é uma estrutura separada da árvore de
//! nós; aqui o layout é indexado por `NodeIdx` e um nó a mais no arena teria de
//! ser criado e destruído a cada recascata (a existência da caixa depende da
//! cascata), tocando no memo por epoch e na numeração documental. O custo
//! estava todo fora do problema.
//!
//! O que se faz em vez disso: a caixa gerada é resolvida sob procura, a partir
//! do elemento originante, e entregue ao fluxo inline como um RUN de texto — a
//! representação que o fluxo já tem para "texto com um estilo, pertencente a um
//! elemento". Ver [`crate::layout`], onde entra, e o teste
//! `before_nao_muda_a_arvore_de_nos`, que é o que prova que a árvore ficou
//! limpa.

use crate::style::ComputedStyle;

/// Uma caixa gerada, já resolvida: o texto que ela pinta e o estilo com que o
/// pinta.
#[derive(Clone, Debug, PartialEq)]
pub struct PseudoBox {
    /// O texto de `content`, já com `attr()` substituído.
    pub texto: String,
    /// O estilo computado da caixa — herdado do elemento originante e depois
    /// sobreposto pelas regras `::before`/`::after` que casaram.
    pub css: ComputedStyle,
}

/// O valor de `content`, decomposto nas peças que a spec permite concatenar
/// (`content: "[" attr(data-x) "]"`).
///
/// `content` não é uma propriedade do [`ComputedStyle`]: só se aplica a
/// pseudo-elementos, e pô-la na tabela de propriedades daria um campo a mais em
/// cada um dos milhares de `ComputedStyle` de uma página para servir umas
/// dezenas de caixas. Fica guardada na regra, ao lado das declarações.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// `none` / `normal` — não gera caixa nenhuma.
    Nenhum,
    /// As peças a concatenar, em ordem.
    Pecas(Vec<Peca>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Peca {
    /// Uma string literal do CSS, com os escapes já resolvidos.
    Texto(String),
    /// `attr(nome)` — o valor do atributo do elemento ORIGINANTE (não da caixa,
    /// que não tem atributos). Ausente resolve para string vazia, como na spec.
    Attr(String),
}

/// Parseia o valor de uma declaração `content`.
///
/// `None` significa "não sei gerar isto", e é diferente de
/// [`Content::Nenhum`]: o primeiro descarta a declaração e deixa a cascata
/// continuar com o que outra regra disser, o segundo é a resposta `none` da
/// própria folha e vence como qualquer outro valor.
///
/// FICAM DE FORA, e é aqui que se diz quais e porquê:
/// - `url(...)` — gera uma caixa SUBSTITUÍDA (uma imagem), que não é texto e
///   precisa do caminho de imagem do layout, com carregamento e tamanho
///   intrínseco. Medido na folha da Wikipédia: 6 das 100 regras.
/// - `counter(...)` — precisa de contadores de documento (`counter-reset` e
///   `counter-increment` propagados na ordem da árvore), que o motor não tem.
///   4 das 100 regras.
/// - `open-quote`/`close-quote` — dependem de `quotes` e do nível de aninhamento.
///
/// Em todos, gerar uma caixa vazia seria pior do que não gerar: reservaria
/// espaço e deslocaria o que está à volta sem pintar nada.
pub fn parse_content(valor: &str) -> Option<Content> {
    let v = valor.trim();
    if v.eq_ignore_ascii_case("none") || v.eq_ignore_ascii_case("normal") {
        return Some(Content::Nenhum);
    }
    let mut pecas = Vec::new();
    let mut resto = v;
    while !resto.trim().is_empty() {
        resto = resto.trim_start();
        let primeiro = resto.chars().next()?;
        if primeiro == '"' || primeiro == '\'' {
            let (texto, depois) = string_css(resto)?;
            pecas.push(Peca::Texto(texto));
            resto = depois;
        } else if let Some(depois) = tira_prefixo_sem_caso(resto, "attr(") {
            let fecha = depois.find(')')?;
            let nome = depois[..fecha].trim().to_ascii_lowercase();
            if nome.is_empty() {
                return None;
            }
            pecas.push(Peca::Attr(nome));
            resto = &depois[fecha + 1..];
        } else {
            return None; // url(), counter(), open-quote, um identificador solto…
        }
    }
    (!pecas.is_empty()).then_some(Content::Pecas(pecas))
}

/// `s` sem o prefixo `pref`, comparado sem distinguir maiúsculas.
fn tira_prefixo_sem_caso<'a>(s: &'a str, pref: &str) -> Option<&'a str> {
    (s.len() >= pref.len() && s[..pref.len()].eq_ignore_ascii_case(pref)).then(|| &s[pref.len()..])
}

/// Lê uma string CSS entre aspas a partir de `s`, devolvendo (conteúdo, resto).
///
/// Trata `\` como escape porque é assim que uma folha real escreve um caractere
/// que não consegue pôr no ficheiro: `content: "\2192"` é a seta que aparece nos
/// menus da Wikipédia. Sem isto, o utilizador via `2192` escrito na página.
fn string_css(s: &str) -> Option<(String, &str)> {
    let aspa = s.chars().next()?;
    let mut out = String::new();
    let mut chars = s.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        if c == aspa {
            return Some((out, &s[i + c.len_utf8()..]));
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        // Escape: ou um código hexadecimal (até 6 dígitos, terminado por espaço
        // opcional), ou o caractere seguinte à letra.
        let hex: String = s[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .take(6)
            .collect();
        if hex.is_empty() {
            if let Some((_, lit)) = chars.next() {
                out.push(lit);
            }
            continue;
        }
        if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
            out.push(ch);
        }
        // consome os dígitos e o espaço que os termina, se houver.
        let mut consumidos = hex.len();
        if s[i + 1 + hex.len()..].starts_with(' ') {
            consumidos += 1;
        }
        for _ in 0..consumidos {
            chars.next();
        }
    }
    None // string sem fecho — declaração inválida
}

/// Materializa o texto de um [`Content`] contra o elemento originante.
pub fn texto_de(content: &Content, attr: &impl Fn(&str) -> Option<String>) -> Option<String> {
    let Content::Pecas(pecas) = content else {
        return None;
    };
    let mut out = String::new();
    for p in pecas {
        match p {
            Peca::Texto(t) => out.push_str(t),
            // Atributo ausente dá string vazia (spec), NÃO cancela a caixa: uma
            // folha que escreve `content: "[" attr(x) "]"` ainda quer os
            // colchetes quando `x` não existe.
            Peca::Attr(nome) => out.push_str(&attr(nome).unwrap_or_default()),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_html_to_dom;
    use crate::layout::{ApproxMeasurer, DisplayItem, LayoutCtx, layout_document};

    /// Os textos pintados, em ordem de pintura — é o que prova que a caixa
    /// gerada existe e onde ficou.
    fn textos(html: &str) -> Vec<String> {
        let dom = parse_html_to_dom(html);
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let lista = layout_document(&dom, &ctx);
        let mut out = Vec::new();
        lista.walk(|item, _, _| {
            if let DisplayItem::Text { text, .. } = item {
                out.push(text.to_string());
            }
        });
        out
    }

    /// A cor com que cada texto foi pintado.
    fn textos_e_cores(html: &str) -> Vec<(String, u32)> {
        let dom = parse_html_to_dom(html);
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let lista = layout_document(&dom, &ctx);
        let mut out = Vec::new();
        lista.walk(|item, _, _| {
            if let DisplayItem::Text { text, color, .. } = item {
                out.push((text.to_string(), *color));
            }
        });
        out
    }

    #[test]
    fn before_com_content_acrescenta_uma_caixa_antes_do_conteudo() {
        let t = textos("<style>p::before { content:\"→\" }</style><p>oi</p>");
        assert_eq!(t, vec!["→".to_string(), "oi".to_string()]);
    }

    #[test]
    fn after_vem_depois_do_conteudo() {
        let t = textos("<style>p::after { content:\"!\" }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string(), "!".to_string()]);
        // e os dois juntos envolvem o conteúdo.
        let t2 = textos("<style>p::before{content:\"[\"} p::after{content:\"]\"}</style><p>oi</p>");
        assert_eq!(t2, vec!["[".to_string(), "oi".to_string(), "]".to_string()]);
    }

    #[test]
    fn before_nao_muda_a_arvore_de_nos() {
        // O TESTE que prova que não sujámos a árvore: a caixa gerada pinta, mas
        // o programa não a vê em consulta nenhuma — como no browser.
        let dom = parse_html_to_dom("<style>p::before { content:\"→\" }</style><p>oi</p>");
        let p = dom.query("p").unwrap();
        assert_eq!(dom.child_nodes(p).len(), 1); // só o nó de texto "oi"
        assert_eq!(dom.child_elements(p).len(), 0);
        assert_eq!(dom.inner_html(p).unwrap(), "oi");
        // e nenhuma consulta a alcança.
        assert_eq!(dom.query_all("*").len(), dom.query_all("*").len().min(64));
        assert!(dom.query("::before").is_none());
        // mas ela PINTA — senão este teste estaria a provar o nada.
        let t = textos("<style>p::before { content:\"→\" }</style><p>oi</p>");
        assert!(t.contains(&"→".to_string()));
    }

    #[test]
    fn sem_content_nao_ha_caixa() {
        // Uma regra `::before` que só declara cor não gera nada (spec): sem
        // `content` não há caixa.
        let t = textos("<style>p::before { color:#ff0000 }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string()]);
        // `content:none` também não.
        let t2 = textos("<style>p::before { content:none }</style><p>oi</p>");
        assert_eq!(t2, vec!["oi".to_string()]);
    }

    #[test]
    fn display_none_no_pseudo_nao_gera_caixa() {
        let t = textos("<style>p::before { content:\"→\"; display:none }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string()]);
    }

    #[test]
    fn regra_de_pseudo_elemento_nao_estiliza_o_elemento() {
        // O erro que fazia o `::before` ser recusado no parse durante tanto
        // tempo: as declarações são da caixa gerada, não do `<p>`.
        let v =
            textos_e_cores("<style>p::before { content:\"→\"; color:#ff0000 }</style><p>oi</p>");
        let oi = v.iter().find(|(t, _)| t == "oi").unwrap();
        assert_ne!(
            oi.1, 0xFF0000FF,
            "o vermelho era da caixa gerada, não do <p>"
        );
        let seta = v.iter().find(|(t, _)| t == "→").unwrap();
        assert_eq!(seta.1, 0xFF0000FF);
    }

    #[test]
    fn pseudo_herda_a_cor_do_elemento_e_a_propria_vence() {
        // Sem `color` próprio, sai da cor do texto à volta.
        let v = textos_e_cores("<style>p{color:#00ff00} p::before{content:\"→\"}</style><p>oi</p>");
        assert_eq!(v.iter().find(|(t, _)| t == "→").unwrap().1, 0x00FF00FF);
    }

    #[test]
    fn content_vazio_nao_pinta_texto_nenhum() {
        // `content:""` é o caso MAIORITÁRIO da folha real (16 das 80 regras com
        // pseudo-elemento): a caixa existe para levar um `background` ou uma
        // `border`, não texto. Aqui ela não pinta glifo nenhum — o que se fixa é
        // que também não inventa um run vazio a deslocar o que vem a seguir.
        let t = textos("<style>p::before { content:\"\" }</style><p>oi</p>");
        assert_eq!(t, vec!["oi".to_string()]);
    }

    #[test]
    fn display_block_no_pseudo_e_tratado_como_inline() {
        // CORTE DECLARADO, fixado aqui para não passar por acidente: a caixa
        // gerada entra sempre como run inline. Com `display:block` o browser
        // punha-a numa linha só dela; nós pomo-la na mesma linha do conteúdo.
        // A alternativa — gerar uma caixa de bloco — exige um ponto de enxerto
        // no fluxo de `layout.rs` que ainda não foi aberto.
        let t = textos("<style>p::before { content:\"→\"; display:block }</style><p>oi</p>");
        // Idêntico ao caso sem `display` — é essa igualdade que diz que a
        // declaração não foi lida, e não uma ordem que por acaso coincide.
        assert_eq!(
            t,
            textos("<style>p::before { content:\"→\" }</style><p>oi</p>")
        );
        assert_eq!(t, vec!["→".to_string(), "oi".to_string()]);
    }

    #[test]
    fn content_com_attr_le_o_atributo_do_originante() {
        let t = textos(
            "<style>p::after { content:\" (\" attr(data-nota) \")\" }</style>\
             <p data-nota='n1'>oi</p>",
        );
        // O espaço inicial do `content` SOBREVIVE. Este teste esperava que ele
        // sumisse e dizia porquê: o fluxo inline apagava o espaço em toda
        // fronteira de run, e a expectativa registava o defeito à espera de quem
        // o corrigisse. Corrigido no `collapse_ws`/`wrap_runs`, a expectativa
        // passa a ser a do browser.
        assert_eq!(t, vec!["oi".to_string(), " (n1)".to_string()]);
    }

    #[test]
    fn pseudo_de_elemento_inline_entra_no_meio_da_linha() {
        // O `::after` de um `<a>` dentro de um parágrafo fica ENTRE o texto do
        // link e o que vem a seguir — é o caso do ícone de link externo.
        let t = textos("<style>a::after { content:\"↗\" }</style><p>ver <a>aqui</a> agora</p>");
        // O ícone entra COLADO ao fim do texto do link — a linha pinta-o no
        // mesmo segmento, que é a prova de que ficou dentro da linha e não numa
        // caixa à parte.
        assert!(t.contains(&"aqui↗".to_string()), "ordem: {t:?}");
        let pos_icone = t.iter().position(|s| s.contains('↗')).unwrap();
        let pos_fim = t.iter().position(|s| s.contains("agora")).unwrap();
        assert!(pos_icone < pos_fim, "ordem: {t:?}");
    }

    #[test]
    fn cascata_do_content_segue_a_especificidade() {
        // A regra mais específica dita o `content`, e uma regra que não o
        // declara não o apaga — é o caso maioritário numa folha real.
        let t = textos(
            "<style>p::before{content:\"a\"} .x::before{content:\"b\"} \
             p.x::before{color:#ff0000}</style><p class='x'>oi</p>",
        );
        assert_eq!(t[0], "b");
    }

    #[test]
    fn pseudo_elemento_que_nao_geramos_descarta_a_regra() {
        // `::marker` não é conteúdo gerado por `content`; aplicá-lo ao elemento
        // pintaria o `<p>` inteiro.
        let v = textos_e_cores("<style>p::marker { color:#ff0000 }</style><p>oi</p>");
        assert_ne!(v[0].1, 0xFF0000FF);
    }

    #[test]
    fn content_string_com_escape_hexadecimal_vira_o_caractere() {
        // `"\2192"` é como uma folha real escreve a seta — se não se resolvesse
        // o escape, a página mostrava os dígitos.
        let Some(Content::Pecas(p)) = parse_content(r#""\2192""#) else {
            panic!()
        };
        assert_eq!(p, vec![Peca::Texto("→".to_string())]);
        // e o espaço que termina o código hexadecimal é consumido, não pintado.
        let Some(Content::Pecas(p)) = parse_content(r#""\2192 x""#) else {
            panic!()
        };
        assert_eq!(p, vec![Peca::Texto("→x".to_string())]);
    }

    #[test]
    fn content_none_e_diferente_de_content_desconhecido() {
        // `none` é uma resposta da folha e vence na cascata; `url()` é uma
        // declaração que não sabemos cumprir e é descartada.
        assert_eq!(parse_content("none"), Some(Content::Nenhum));
        assert_eq!(parse_content("normal"), Some(Content::Nenhum));
        assert_eq!(parse_content("url(seta.png)"), None);
        assert_eq!(parse_content("counter(item)"), None);
    }

    #[test]
    fn content_concatena_string_e_attr() {
        let c = parse_content(r#""[" attr(data-x) "]""#).unwrap();
        let attr = |n: &str| (n == "data-x").then(|| "oi".to_string());
        assert_eq!(texto_de(&c, &attr).unwrap(), "[oi]");
        // atributo ausente é string vazia, e os literais ficam.
        let vazio = |_: &str| None;
        assert_eq!(texto_de(&c, &vazio).unwrap(), "[]");
    }
}
