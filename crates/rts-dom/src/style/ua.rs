//! A folha de UA (agente de utilizador) — lote I do `crates/rts-dom/PLAN.md` §4.I.
//!
//! O texto vive em `ua.css`, ao lado deste ficheiro (o cabeçalho dele diz o
//! que ficou de fora e porquê). Este módulo PARSEIA-a uma vez por thread com
//! o mesmo `style::stylesheet::parse_rules` que um `<style>` de autor usa —
//! nenhuma segunda gramática, nenhuma segunda tabela — e devolve as regras já
//! marcadas com [`Rule::is_ua`], para [`Stylesheet::new`](super::stylesheet::Stylesheet::new)
//! as anexar a CADA folha construída.
//!
//! Substitui dois mecanismos que a auditoria estrutural de 2026-09-04
//! encontrou (`docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/03-estilo-e-cascade.md`,
//! findings 2 e 4): `block::ua::UA_TABLE`/`install_ua_defaults` (uma tabela de
//! 5 campos fixos, sem seletor nem propriedade fora do conjunto) e
//! `block::ua::ua_display` (um `match` chamado pelo LAYOUT depois da cascade,
//! e não pela cascade em si). `used_display` (`layout/caixa.rs`) já não
//! consulta `ua_display`: o `display` de `<li>`/`<table>`/`<td>`/… agora
//! chega pela mesma cascade que qualquer outra propriedade, e uma regra de
//! autor (`td { display: block }`) vence-o naturalmente em vez de nunca ser
//! perguntada.
//!
//! `UA_TABLE`, `install_ua_defaults` e `ua_display` continuam a existir
//! (`block/ua.rs`) para o EIXO bloco/inline/flex de baixo nível
//! (`layout::caixa::css_display`, o código inteiro que `block::lookup`
//! devolve) — esse é um mecanismo maior, partilhado com o `defineBlock`/
//! `defineInline` que outras partes do motor ainda chamam por fora da
//! cascade CSS, e o PLAN.md §4.I só pede a morte de `ua_display`
//! especificamente. Deixar os dois por enquanto é o que a acção 5 do
//! enunciado ("se for maior… deixe desenhado") pede para o resto do
//! `UA_TABLE`: fechá-lo por completo exigiria que `css_display` deixasse de
//! ter QUALQUER fallback fora da cascade, o que move margem/display de toda
//! tag HTML na mesma revisão — risco maior do que o resto deste lote junto,
//! e sem uma régua de pintura (lote N, ainda por medir) para o confirmar.

use super::stylesheet::Rule;

const UA_CSS: &str = include_str!("ua.css");

thread_local! {
    static UA_RULES: Vec<Rule> = build();
}

fn build() -> Vec<Rule> {
    // `parse_rules` reaproveita a MESMA gramática/lowering de um `<style>` de
    // autor — não há uma segunda leitura de CSS aqui. `order` vem sempre 0 do
    // parser genérico (quem o define é `append_css`, que não corre para a
    // UA); a UA precisa da própria ordem de documento como desempate dentro
    // da SUA origem, por isso é reatribuída aqui pela posição na folha.
    let mut rules = crate::style::stylesheet::parse_rules(UA_CSS);
    for (i, r) in rules.iter_mut().enumerate() {
        r.is_ua = true;
        r.order = i as u32;
    }
    rules
}

/// As regras da UA-stylesheet, parseadas uma vez por THREAD (como as outras
/// caches deste crate — `RuleIndex`, `hover_reach`) e clonadas por chamada:
/// `Rule` carrega os seus campos grandes em `Rc`, então clonar o vetor é
/// barato (contagens de referência, não os `ComputedStyle`).
pub(crate) fn rules() -> Vec<Rule> {
    UA_RULES.with(std::clone::Clone::clone)
}

#[cfg(test)]
mod tests {
    use crate::style::stylesheet::Stylesheet;

    /// `<th>` fica negrito e centrado SEM nenhum CSS de autor — o que a
    /// UA_TABLE antiga não dava (findings 2 da auditoria).
    #[test]
    fn th_e_negrito_e_centrado_sem_css_de_autor() {
        let sheet = Stylesheet::new();
        let computed = sheet.computed_for("th", None, &[]);
        assert_eq!(computed.normal.bold, Some(true));
        assert_eq!(
            computed.normal.text_align,
            Some(crate::style::TextAlign::Center)
        );
    }

    /// `<ul>` recua os itens 40px — o que substitui `UA_LIST_INDENT`.
    #[test]
    fn ul_tem_recuo_de_lista_via_padding_inline_start() {
        let sheet = Stylesheet::new();
        let computed = sheet.computed_for("ul", None, &[]);
        assert_eq!(
            computed.normal.padding.left,
            crate::style::Side::px_len(40.0)
        );
    }

    /// `body` herda a margem de 8px da UA, e uma regra de autor (`margin: 0`)
    /// vence-a — a forma normal da cascade, sem nada especial para a UA.
    #[test]
    fn body_tem_margem_ua_vencida_pelo_autor() {
        let mut sheet = Stylesheet::new();
        let sem_autor = sheet.computed_for("body", None, &[]);
        assert_eq!(sem_autor.normal.margin.top, crate::style::Side::px_len(8.0));

        sheet.append_css("body { margin: 0; }");
        let com_autor = sheet.computed_for("body", None, &[]);
        assert_eq!(com_autor.normal.margin.top, crate::style::Side::px_len(0.0));
    }

    /// `input:disabled` fica com a cor de "desactivado" — um seletor composto
    /// que a `UA_TABLE` (5 campos fixos, sem pseudo-classe) não conseguia
    /// exprimir.
    #[test]
    fn input_disabled_fica_acinzentado() {
        let sheet = Stylesheet::new();
        let matches_disabled = |sel: &crate::style::ComplexSelector| {
            crate::style::selector::compound_matches(
                &sel.compounds[0],
                "input",
                None,
                &[],
                &|_| None,
                &|p| matches!(p, crate::style::PseudoClass::Disabled),
            )
        };
        let ctx = crate::style::MediaContext { width: 1280.0, height: 800.0, ..Default::default() };
        let matched = sheet.matched_for_node(&ctx, "input", None, &[], matches_disabled);
        let computed = sheet.declarations_from(&matched, None);
        // rgb(84,84,84) — medido no Chrome real, não a aproximação `gray`
        // (#808080) do primeiro rascunho; ver `claude-ua-form-disabled`.
        assert_eq!(computed.normal.color, Some(0x545454ff));
    }

    /// `display` de `li`/`table`/`td` continua certo SEM `ua_display`: chega
    /// pela cascade normal, como qualquer outra propriedade CSS.
    #[test]
    fn display_de_papeis_de_tabela_vem_da_cascade() {
        let sheet = Stylesheet::new();
        assert_eq!(
            sheet.computed_for("li", None, &[]).normal.display,
            Some(crate::style::DisplayKind::ListItem)
        );
        assert_eq!(
            sheet.computed_for("table", None, &[]).normal.display,
            Some(crate::style::DisplayKind::Table)
        );
        assert_eq!(
            sheet.computed_for("td", None, &[]).normal.display,
            Some(crate::style::DisplayKind::TableCell)
        );
    }

    /// As margens de `h2`-`h6` resolvem o `em` contra o PRÓPRIO `font-size`
    /// (já resolvido pela mesma regra), não o do body — medido no Chrome
    /// real (`tests/css/claude-ua-headings.esperado.json`): `h2` é
    /// `0.83em` de `24px` = `19.92px`, não `16px` (o que dava quando a
    /// folha usava `px` fixos para as duas propriedades).
    #[test]
    fn margem_de_heading_resolve_contra_o_proprio_font_size() {
        // Valores medidos no Chrome real (`claude-ua-headings.esperado.json`):
        // `h3`..`h6` são os que expõem casas decimais e por isso o teste da
        // formatação — `h1`/`h2` já bateriam com um `Px` fixo por coincidência.
        let dom = crate::parse_html_to_dom(
            "<h1>1</h1><h2>2</h2><h3>3</h3><h4>4</h4><h5>5</h5><h6>6</h6>",
        );
        let casos = [
            ("h1", "32px", "21.44px"),
            ("h2", "24px", "19.92px"),
            ("h3", "18.72px", "18.72px"),
            ("h4", "16px", "21.28px"),
            ("h5", "13.28px", "22.1776px"),
            ("h6", "10.72px", "24.9776px"),
        ];
        for (sel, font_size, margin) in casos {
            let id = dom.query(sel).unwrap();
            assert_eq!(dom.computed_property(id, "font-size"), font_size, "{sel}");
            assert_eq!(dom.computed_property(id, "margin-top"), margin, "{sel}");
            assert_eq!(dom.computed_property(id, "margin-bottom"), margin, "{sel}");
        }
    }

    /// A altura de `input`/`button` bate EXATA com o Chrome
    /// (`claude-ua-form-disabled.esperado.json`: 21px os dois) — é o que
    /// prova que o modelo de caixa (padding+borda do lote I) está certo na
    /// vertical. A LARGURA fica de fora deste teste: depende de quanto o
    /// `ApproxMeasurer` mede o texto/placeholder, que já diverge do Chrome
    /// noutros lugares (documentado em `layout/medida.rs`) — não é uma
    /// lacuna desta folha.
    #[test]
    fn altura_de_botao_e_input_bate_com_o_chrome() {
        let dom = crate::parse_html_to_dom(
            "<style>body{margin:0}</style><input id='normal' value='a'><button id='botao'>ok</button>",
        );
        let ctx = crate::layout::LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let list = crate::layout::layout_document(&dom, &ctx);
        let geo = list.geometry();
        let rect = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            geo.rects[&idx]
        };
        assert_eq!(rect("#normal").h, 21.0);
        assert_eq!(rect("#botao").h, 21.0);
    }

    /// O `border-spacing` de 2px (default de `table::mod::estilo`, não desta
    /// folha — ver o comentário em `ua.css`) desloca a PRIMEIRA linha da
    /// borda da tabela NA VERTICAL: `<tr>.y == 2`, medido no Chrome real
    /// (`claude-ua-th.esperado.json`).
    ///
    /// **`<tr>.x` fica em 0, não em 2 como o Chrome mede** — LACUNA
    /// conhecida e não fechada por este lote: a caixa do `<tr>` neste motor
    /// cobre a largura inteira da tabela (o recuo horizontal do
    /// `border-spacing` está nas CÉLULAS, que `o_border_spacing_existe_
    /// tambem_entre_a_borda_e_a_primeira_coluna` já pina certo), e o Chrome
    /// mede a caixa do `<tr>` encolhida ao redor das células em vez disso.
    /// É uma questão de layout de tabela (`table/`), fora do ficheiro deste
    /// lote (`style/ua.css`) — reportado, não corrigido aqui.
    #[test]
    fn primeira_linha_desloca_na_vertical_pelo_border_spacing_default() {
        let dom = crate::parse_html_to_dom(
            "<style>body{margin:0}</style><table id='tabela'><tr id='linha'><th>a</th><td>b</td></tr></table>",
        );
        let ctx = crate::layout::LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let list = crate::layout::layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#linha").unwrap()).unwrap();
        let r = list.geometry().rects[&idx];
        assert_eq!(r.y, 2.0);
    }

    /// A UA nunca cria surpresas de fora da cascade normal em cima da regra
    /// `!important` de UM autor — a inversão de origem só entraria em jogo
    /// se a PRÓPRIA folha de UA usasse `!important`, o que ela não faz (não
    /// há motivo: nenhum default de browser precisa de resistir ao autor).
    /// Este teste pina a ordem NORMAL, que é a única que a folha actual
    /// exercita: um `color` de autor sem `!important` já vence o `color`
    /// de UA do `<a>`.
    #[test]
    fn autor_normal_vence_cor_de_link_da_ua() {
        let mut sheet = Stylesheet::new();
        sheet.append_css("a { color: red; }");
        let computed = sheet.computed_for("a", None, &[]);
        assert_eq!(computed.normal.color, crate::style::color::parse_color("red"));
    }
}
