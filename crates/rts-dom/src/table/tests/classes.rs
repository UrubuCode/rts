//! O que a CLASSE de uma coluna decide — declarada, percentagem, auto, e a que
//! nenhuma célula toca.
//!
//! Todos os números aqui foram **medidos num Chrome real** por
//! `scripts/parity/chrome_extract.mjs` (viewport 1280x800, JS da página
//! desligado), cada caso numa caixa fechada de 900px para que um não contamine o
//! seguinte. Não são derivados da spec: a spec é vaga neste ponto e toda a gente
//! implementa o que o Blink implementa.
//!
//! **Nenhuma célula contém texto.** O conteúdo é sempre `inline-block` de
//! largura fixa — um de 100px, ou dois de 50px quando a coluna precisa de FOLGA
//! (mínimo 50, máximo 100). A alternativa, escrever texto, mediria a métrica da
//! fonte: o Chrome usa a fonte real e os testes usam o `ApproxMeasurer`
//! (`n * tamanho * 0.5`), por isso qualquer divergência viria da medição do
//! texto e não do critério de repartição, que é o que estes testes fixam.

use super::{geometria, rect};

/// O CSS comum a todos os casos, igual ao da página que o Chrome mediu.
const BASE: &str = "<style>table{border-collapse:collapse}td{padding:0;border:0}\
i{display:inline-block;width:50px;height:10px}b{display:inline-block;width:100px;height:10px}</style>";

/// As larguras das `n` primeiras células, na ordem do documento.
fn larguras(html: &str, n: usize) -> Vec<f32> {
    let doc = format!("{BASE}{html}");
    let (dom, list) = geometria(&doc, 900.0);
    (0..n).map(|i| rect(&dom, &list, "td", i).w).collect()
}

fn perto(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.51
}

macro_rules! iguais {
    ($got:expr, $esperado:expr) => {{
        let g: Vec<f32> = $got;
        let e: Vec<f32> = $esperado.to_vec();
        assert!(
            g.len() == e.len() && g.iter().zip(&e).all(|(a, b)| perto(*a, *b)),
            "chrome {e:?}, nós {g:?}"
        );
    }};
}

#[test]
fn acima_do_maximo_so_as_auto_crescem() {
    // Declarada 200 (min=max) + auto 50..100. Alvo 600 já passa o kMaxGuess
    // (300): o excedente é TODO da coluna auto, e a declarada fica nos 200.
    // Nós repartimos proporcionalmente ao máximo → 400/200 em vez de 200/400.
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:200px"><b></b></td><td><i></i><i></i></td></tr></table>"#, 2),
        [200.0, 400.0]
    );
}

#[test]
fn entre_o_specified_e_o_max_guess_so_a_auto_cresce() {
    // Mesmo par, alvo 280: entre o kSpecifiedGuess (250) e o kMaxGuess (300).
    // A declarada continua congelada nos 200 e a auto leva os 30 da folga.
    iguais!(
        larguras(r#"<table style="width:280px"><tr><td style="width:200px"><b></b></td><td><i></i><i></i></td></tr></table>"#, 2),
        [200.0, 80.0]
    );
}

#[test]
fn duas_auto_repartem_entre_si_e_a_declarada_nao_participa() {
    // Declarada 100 + duas auto 50..100, alvo 260. A declarada não recebe nada
    // do excedente: 100 + 80 + 80.
    iguais!(
        larguras(r#"<table style="width:260px"><tr><td style="width:100px"><b></b></td><td><i></i><i></i></td><td><i></i><i></i></td></tr></table>"#, 3),
        [100.0, 80.0, 80.0]
    );
}

#[test]
fn uma_coluna_em_percentagem_leva_a_sua_percentagem_da_tabela() {
    // 30% de 600 = 180, e não o que a repartição por folga daria. As duas auto
    // dividem o resto. Nós não temos classe percent de todo: a coluna reparte
    // como se fosse texto livre.
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:30%"><b></b></td><td><i></i><i></i></td><td><i></i><i></i></td></tr></table>"#, 3),
        [180.0, 210.0, 210.0]
    );
}

#[test]
fn percentagens_acima_de_cem_por_cento_sao_normalizadas() {
    // 60% + 60% = 120%: o Blink normaliza para a soma dar a tabela inteira,
    // mantendo a razão — 360 e 240 e não 360 e 360.
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:60%"><i></i></td><td style="width:60%"><i></i></td></tr></table>"#, 2),
        [360.0, 240.0]
    );
}

#[test]
fn sem_width_a_tabela_fica_na_soma_dos_maximos() {
    // O caso exato: o alvo bate na soma dos máximos e cada coluna recebe
    // literalmente o seu máximo, sem passar pela matemática de distribuição.
    // É o caso mais comum de uma página inteira.
    iguais!(
        larguras(r#"<table><tr><td style="width:200px"><b></b></td><td><i></i><i></i></td></tr></table>"#, 2),
        [200.0, 100.0]
    );
}

#[test]
fn coluna_que_nenhuma_celula_toca_fica_a_zero() {
    // Três `<col>` e só duas células: a terceira coluna é `mergeable` e é
    // SALTADA — as duas reais dividem os 600 inteiros. Nós damos-lhe uma fatia
    // da sobra, o que estreita as duas outras.
    iguais!(
        larguras(r#"<table style="width:600px"><colgroup><col><col><col></colgroup><tr><td><i></i><i></i></td><td><i></i><i></i></td></tr></table>"#, 2),
        [300.0, 300.0]
    );
}

#[test]
fn sem_colunas_auto_as_declaradas_escalam_para_a_largura_da_tabela() {
    // 200 e 300 numa tabela de 800: crescem em proporção ao que declararam,
    // porque o alvo é a largura da tabela.
    iguais!(
        larguras(r#"<table style="width:800px"><tr><td style="width:200px"><b></b></td><td style="width:300px"><b></b></td></tr></table>"#, 2),
        [320.0, 480.0]
    );
}

#[test]
fn abaixo_do_minimo_cada_coluna_fica_no_seu_minimo() {
    // Tabela pedida a 60px contra dois mínimos de 50: a tabela transborda para
    // 100 e cada coluna fica nos 50.
    iguais!(
        larguras(r#"<table style="width:60px"><tr><td><i></i><i></i></td><td><i></i><i></i></td></tr></table>"#, 2),
        [50.0, 50.0]
    );
}

#[test]
fn a_declarada_fica_parada_mesmo_sem_folga_nenhuma_na_auto() {
    // A mesma pergunta do primeiro teste, com a coluna auto a ter UM só
    // inline-block: min == max == 100, nenhuma folga. Existe para separar as
    // duas causas — aqui a medição intrínseca das células está certa nos dois
    // motores, portanto tudo o que restar de divergência é CLASSE.
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:200px"><b></b></td><td><b></b></td></tr></table>"#, 2),
        [200.0, 400.0]
    );
}

#[test]
fn acima_do_maximo_as_auto_crescem_em_proporcao_ao_seu_maximo() {
    // Duas auto iguais e uma declarada: os 200 que sobram dividem-se pelas duas
    // auto, e a declarada não vê nada deles.
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:200px"><b></b></td><td><b></b></td><td><b></b></td></tr></table>"#, 3),
        [200.0, 200.0, 200.0]
    );
}

#[test]
fn colspan_reparte_o_excedente_pelo_criterio_de_classe() {
    // Um cabeçalho com `colspan=2` que quer 400 sobre uma coluna declarada de
    // 100 e uma auto: o excedente vai todo para a auto. Nós espalhamos por
    // igual, o que empurra a declarada acima dos 100 que ela declarou.
    let w = larguras(
        r#"<table style="width:600px"><tr><td colspan="2"><b></b><b></b><b></b><b></b></td></tr><tr><td style="width:100px"><b></b></td><td><i></i><i></i></td></tr></table>"#,
        3,
    );
    iguais!(vec![w[1], w[2]], [100.0, 500.0]);
}

/// A CLASSE de cada célula, na ordem do documento. Vai buscá-la ao ponto onde
/// ela é decidida — nenhuma geometria aqui, porque a classificação acontece
/// antes de haver larguras e é essa a coisa a fixar.
fn classes(html: &str) -> Vec<(Option<f32>, bool)> {
    let dom = crate::parse_html_to_dom(&format!("{BASE}{html}"));
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 900.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    dom.query_all("td")
        .iter()
        .map(|id| {
            let idx = dom.resolve(*id).expect("nó vivo");
            let c = crate::table::widths::cell_min_max(&dom, idx, 16.0, &ctx);
            (c.percentagem, c.restringida)
        })
        .collect()
}

#[test]
fn a_percentagem_declarada_chega_a_reparticao_em_vez_de_ser_descartada() {
    // Três células: uma em percentagem, uma em pixels, uma livre. A percentagem
    // não é resolúvel onde a célula é medida — depende da largura da tabela — e
    // por isso viajava para lado nenhum. Agora viaja.
    assert_eq!(
        classes(r#"<table><tr><td style="width:30%"><b></b></td><td style="width:200px"><b></b></td><td><i></i></td></tr></table>"#),
        vec![(Some(30.0), true), (None, true), (None, false)]
    );
}

#[test]
fn o_atributo_width_classifica_e_o_css_ganha_lhe() {
    // `width="40%"` vale, porque páginas reais escrevem-no e o CSS nada diz. Na
    // segunda célula o CSS declara pixels: o atributo já perdeu a decisão da
    // largura, e ler-lhe a percentagem daria a uma coluna de 200px a classe de
    // uma coluna de 40%.
    assert_eq!(
        classes(r#"<table><tr><td width="40%"><b></b></td><td width="40%" style="width:200px"><b></b></td></tr></table>"#),
        vec![(Some(40.0), true), (None, true)]
    );
}
