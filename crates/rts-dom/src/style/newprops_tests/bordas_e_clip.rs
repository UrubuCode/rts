//! Os shorthands de caixa da borda, `clip` e os aliases de fornecedor
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

// ── Os shorthands de caixa da borda (ver `style::borders`) ───────────────────

#[test]
fn border_width_de_quatro_valores_chega_aos_quatro_lados() {
    // O defeito: o braço fazia `parse_len(val)`, que lê UM comprimento — quatro
    // valores devolviam `None` e a declaração caía inteira, em silêncio.
    let s = parse_inline("border-style: solid; border-width: 1px 2px 3px 4px");
    assert_eq!(crate::style::borders::used_widths(&s), [1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn border_width_reparte_como_shorthand_de_caixa_e_nao_como_canto() {
    // 2 valores = vertical / horizontal; 3 = topo / horizontal / baixo. É a regra
    // da CAIXA — os cantos de `border-radius` copiam a DIAGONAL, e as duas formas
    // parecem-se o suficiente para se trocarem sem ninguém notar.
    let dois = parse_inline("border-style: solid; border-width: 5px 10px");
    assert_eq!(
        crate::style::borders::used_widths(&dois),
        [5.0, 10.0, 5.0, 10.0]
    );
    let tres = parse_inline("border-style: solid; border-width: 1px 2px 3px");
    assert_eq!(
        crate::style::borders::used_widths(&tres),
        [1.0, 2.0, 3.0, 2.0]
    );
    // e um valor só continua a escrever o campo UNIFORME, como sempre escreveu.
    let um = parse_inline("border-width: 7px");
    assert_eq!(um.border_width, Some(7.0));
}

#[test]
fn border_style_e_color_multivalor_tambem_chegam_aos_lados() {
    // Não é simetria: um triângulo com as larguras certas e sem estilo continua
    // invisível E sem ocupar espaço, porque `used_widths` zera o lado que não
    // pinta. Corrigir só a largura não teria movido nada.
    let s = parse_inline("border-width: 10px; border-style: solid none solid none");
    assert_eq!(
        crate::style::borders::used_widths(&s),
        [10.0, 0.0, 10.0, 0.0]
    );
    let c = parse_inline("border-color: #ff0000 #00ff00");
    assert_eq!(c.border_top_color, Some(0xFF0000FF));
    assert_eq!(c.border_right_color, Some(0x00FF00FF));
    assert_eq!(c.border_bottom_color, Some(0xFF0000FF));
    // uma cor com ESPAÇOS dentro é UM valor, não três lados.
    let rgb = parse_inline("border-color: rgb(1, 2, 3)");
    assert_eq!(rgb.border_color, Some(0x010203FF));
}

#[test]
fn triangulo_de_css_tem_o_tamanho_da_borda() {
    // A forma que motivou isto: conteúdo 0x0, três lados a zero e um enorme — a
    // caixa É a borda. É como a Wikipédia desenha um gráfico de setores, e a
    // declaração inteira era descartada: 24,9% de todo o erro de largura da
    // página em 36 elementos.
    let s = parse_inline("width:0;height:0;border-style:solid;border-width:100px 0 0 200px");
    assert_eq!(
        crate::style::borders::used_widths(&s),
        [100.0, 0.0, 0.0, 200.0]
    );
    // e o box model soma-as: a caixa mede 200x100 com conteúdo nenhum.
    let html = "<div style='background:#eee'>\
                <div style='width:0;height:0;border-style:solid;border-width:100px 0 0 200px'></div>\
                </div>";
    let pai = first_solid(&layout(html, 1280.0)).expect("o pai pinta").0;
    assert_eq!(pai.h, 100.0, "a altura do pai é a borda do triângulo");
}

#[test]
fn largura_zero_e_uma_largura_declarada_e_nao_uma_ausencia() {
    // Segundo defeito da mesma zona, encontrado a verificar o primeiro: o
    // `parse_px` filtra `> 0`, portanto `border-width: 0` devolvia `None` e a
    // declaração caía. O lado ficava por declarar e HERDAVA a borda uniforme —
    // dando largura a um lado que o autor mandou apagar.
    let s = parse_inline("border: 5px solid; border-top-width: 0");
    assert_eq!(
        crate::style::borders::used_widths(&s)[0],
        0.0,
        "o topo foi apagado"
    );
    assert_eq!(
        crate::style::borders::used_widths(&s)[2],
        5.0,
        "o resto fica"
    );
    // e no shorthand, que é onde a forma do triângulo o traz.
    let t = parse_inline("border: 5px solid; border-width: 0 200px 100px 0");
    assert_eq!(
        crate::style::borders::used_widths(&t),
        [0.0, 200.0, 100.0, 0.0]
    );
    // os keywords também: `parse_len` não os conhecia e caíam do mesmo modo.
    assert_eq!(parse_inline("border-width: thick").border_width, Some(5.0));
}

#[test]
fn cor_de_decoracao_nao_declarada_e_a_cor_do_elemento() {
    // `currentColor` é o inicial de `text-decoration-color`, e o Chrome responde
    // a cor já RESOLVIDA. O inicial não cabia na tabela de `style::initial`
    // porque não é uma constante: é o valor de outra propriedade deste nó.
    let s = parse_inline("color: #0000ff; text-decoration-line: underline");
    assert_eq!(
        s.computed_value("text-decoration-color", None),
        "rgb(0, 0, 255)"
    );
    // declarada, vence o declarado.
    let d = parse_inline("color: #0000ff; text-decoration-color: #ff0000");
    assert_eq!(
        d.computed_value("text-decoration-color", None),
        "rgb(255, 0, 0)"
    );
    // e o `el.style` continua vazio para o que o elemento não declarou.
    assert_eq!(s.get_property("text-decoration-color"), "");
}

// ── LOTE A do corpus alargado: `clip` e os aliases de fornecedor ─────────────

#[test]
fn clip_aceita_as_duas_sintaxes_de_rect_que_o_corpus_escreve() {
    // Não é purismo de spec: as duas estão no corpus e vêm de autores diferentes.
    // Com vírgulas é o que Bootstrap, Tailwind e Foundation emitem; sem vírgulas
    // é o que MediaWiki e WhatsApp emitem. Reconhecer só uma delas deixava
    // metade das 8 folhas por cobrir e a contagem diria o contrário.
    use crate::style::vocab::Clip;
    let virgulas = parse_inline("clip: rect(0, 0, 0, 0)");
    let espacos = parse_inline("clip: rect(0 0 0 0)");
    assert_eq!(virgulas.clip, espacos.clip, "a grafia não muda o valor");
    assert!(matches!(virgulas.clip, Some(Clip::Rect { .. })));
    // e o computed sai na forma do Chrome: vírgulas e unidade explícita.
    assert_eq!(virgulas.get_property("clip"), "rect(0px, 0px, 0px, 0px)");
}

#[test]
fn clip_guarda_auto_por_lado_e_comprimento_negativo() {
    // `auto` num lado só (`rect(auto, 0, 0, auto)`) é legal e não é o mesmo que
    // zero — quem vier a recortar precisa da diferença. E o retângulo pode
    // começar ACIMA da caixa, o que é o motivo de o parser ser `parse_inset`:
    // `parse_dimension` rejeita negativos e transformaria -5px num lado ausente.
    let s = parse_inline("clip: rect(auto, 0, 0, auto)");
    assert_eq!(s.get_property("clip"), "rect(auto, 0px, 0px, auto)");
    let neg = parse_inline("clip: rect(-5px 0 0 0)");
    assert_eq!(neg.get_property("clip"), "rect(-5px, 0px, 0px, 0px)");
}

#[test]
fn clip_nao_declarado_computa_auto_e_o_style_inline_fica_vazio() {
    // As duas semânticas opostas que `style::initial` documenta, nesta
    // propriedade: o computed cai no inicial, o `el.style` não.
    let s = parse_inline("color: red");
    assert_eq!(s.computed_value("clip", None), "auto");
    assert_eq!(
        s.get_property("clip"),
        "",
        "el.style só tem o que foi declarado"
    );
}

#[test]
fn sr_only_continua_escondido_sem_o_recorte_ser_aplicado() {
    // Esta é a condição que autorizou guardar `clip` sem recortar, e por isso é
    // um teste e não um comentário. Em TODAS as 8 folhas do corpus o
    // `clip: rect(...)` vem ao lado de uma caixa de 1px com `overflow:hidden` —
    // é a caixa que esconde, não o clip. Se um dia o layout deixar de honrar a
    // altura de 1px, este teste cai e diz que o recorte passou a ser preciso.
    let l = layout(
        "<div style='position:absolute;width:1px;height:1px;overflow:hidden;\
         clip:rect(0,0,0,0)'>texto para leitor de ecra</div>",
        800.0,
    );
    let maior = itens(&l)
        .iter()
        .filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(rect.w.max(rect.h)),
            _ => None,
        })
        .fold(0.0f32, f32::max);
    assert!(
        maior <= 1.0,
        "a caixa do .sr-only tem de continuar em 1px, e não {maior}"
    );
}

#[test]
fn text_decoration_prefixada_responde_o_mesmo_que_a_nua() {
    // 6 folhas escrevem `-webkit-text-decoration` ao lado da nua. O `match` do
    // `parse` casa por literal e não vê o prefixo, por isso a prefixada ia para
    // a lista de ignoradas. O que este teste fixa não é só "passou a ser
    // reconhecida" — é que as duas grafias respondem o MESMO, incluindo a cor do
    // shorthand, que era a metade fácil de esquecer numa segunda cópia do corpo.
    let nua = parse_inline("text-decoration: underline red");
    let webkit = parse_inline("-webkit-text-decoration: underline red");
    let moz = parse_inline("-moz-text-decoration: underline red");
    assert_eq!(nua.text_decoration, webkit.text_decoration);
    assert_eq!(nua.text_decoration_color, webkit.text_decoration_color);
    assert!(
        webkit.text_decoration_color.is_some(),
        "a cor do shorthand também"
    );
    assert_eq!(nua.text_decoration, moz.text_decoration);
}

#[test]
fn text_decoration_line_continua_a_nao_ler_cor() {
    // A distinção que a função partilhada tem de preservar: `-line` não aceita
    // cor. Partilhar o corpo sem o parâmetro fá-lo-ia passar a aceitar, o que é
    // uma regressão que nenhum teste anterior apanhava.
    let s = parse_inline("text-decoration-line: underline red");
    assert_eq!(s.text_decoration_color, None);
}

#[test]
fn text_size_adjust_e_recusada_com_motivo_e_nao_ignorada() {
    // Não tem campo de propósito: este motor não reflui por largura de ecrã, e
    // `none`/`100%`/`auto` computam todos para a mesma página. Reconhecê-la
    // faria a contagem subir sem um pixel mudar — a coluna das recusadas existe
    // exatamente para essa diferença.
    use crate::style::inert::is_inert;
    assert!(is_inert("text-size-adjust"));
    assert!(
        is_inert("-webkit-text-size-adjust"),
        "a forma que as folhas escrevem"
    );
    assert!(is_inert("-ms-text-size-adjust"));
}

#[test]
fn filter_e_clip_path_chegam_crus_ao_paint() {
    // Guardados CRUS a pedido do lado do paint: só um subconjunto das funções é
    // exprimível no backend, e essa decisão é do consumidor. O que este teste
    // fixa é que o valor CHEGA inteiro — incluindo os parênteses e os espaços,
    // que um parser a mais aqui em cima teria de reconstruir.
    let s = parse_inline("filter: blur(4px) brightness(0.8)");
    assert_eq!(s.filter.as_deref(), Some("blur(4px) brightness(0.8)"));
    let c = parse_inline("clip-path: polygon(50% 0%, 100% 100%, 0% 100%)");
    assert_eq!(
        c.clip_path.as_deref(),
        Some("polygon(50% 0%, 100% 100%, 0% 100%)")
    );
    // O prefixo `-webkit-` está ao lado do nome padrão: a folha real declara os
    // dois na mesma regra, e reconhecer só um deixava a metade escrita primeiro
    // a decidir o resultado.
    assert_eq!(
        parse_inline("-webkit-filter: invert(1)").filter.as_deref(),
        Some("invert(1)")
    );
    assert_eq!(
        parse_inline("-webkit-clip-path: inset(10px)")
            .clip_path
            .as_deref(),
        Some("inset(10px)")
    );
}
