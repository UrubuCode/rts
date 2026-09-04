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

// ── A moldura da percentagem: o termo que NÃO existe ─────────────────────────
// Os três casos abaixo medem a mesma coluna `width:30%` de uma tabela de 600px
// com padding a variar, e o Chrome responde 180 aos três. Existem porque o campo
// que somava a moldura chegou a estar escrito, com a regra geral certa por trás
// («uma percentagem de CSS mede a caixa de conteúdo») e o resultado errado: numa
// tabela auto o Blink não conta a moldura. Nenhum outro caso deste ficheiro o
// apanhava, porque todos têm `padding: 0`.

#[test]
fn a_percentagem_nao_soma_o_padding_em_content_box() {
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:30%;padding:0 20px"><b></b></td><td><b></b></td></tr></table>"#, 2),
        [180.0, 420.0]
    );
}

#[test]
fn a_percentagem_nao_muda_com_box_sizing() {
    // Se a moldura entrasse na conta, `border-box` e `content-box` teriam de dar
    // números diferentes. Dão o mesmo — é o par que prova que o termo não existe,
    // em vez de existir com o sinal a anular-se.
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:30%;padding:0 20px;box-sizing:border-box"><b></b></td><td><b></b></td></tr></table>"#, 2),
        [180.0, 420.0]
    );
}

#[test]
fn o_padding_de_uma_coluna_auto_nao_entra_na_percentagem_da_vizinha() {
    iguais!(
        larguras(r#"<table style="width:600px"><tr><td style="width:30%"><b></b></td><td style="padding:0 25px"><b></b></td></tr></table>"#, 2),
        [180.0, 420.0]
    );
}

// ── Tabelas dentro de tabelas ────────────────────────────────────────────────
// O cluster que a régua da página real apontou depois da escada entrar: os seis
// elementos perdidos em `w` e as três piores células eram todos desta forma, e o
// amplificador eram os `<li>` lá dentro — `w=72` no Chrome contra 578 nossos,
// com a altura a passar de 16 para 37 porque a linha partiu em duas.

/// A largura das `<table>`, na ordem do documento — a de fora primeiro.
fn tabelas(html: &str, n: usize) -> Vec<f32> {
    let doc = format!("{BASE}{html}");
    let (dom, list) = geometria(&doc, 900.0);
    (0..n).map(|i| rect(&dom, &list, "table", i).w).collect()
}

#[test]
fn uma_tabela_dentro_de_uma_celula_encolhe_ao_conteudo() {
    // A de dentro cabe em 200 e a célula dá-lhe 600: uma `<table>` sem `width` é
    // shrink-to-fit e não ocupa o que lhe dão, que é a diferença mais visível
    // entre uma tabela e um `<div>`.
    iguais!(
        tabelas(r#"<table style="width:600px"><tr><td><table><tr><td><b></b></td><td><b></b></td></tr></table></td></tr></table>"#, 2),
        [600.0, 200.0]
    );
}

#[test]
fn a_tabela_de_dentro_com_width_cem_por_cento_ocupa_a_celula() {
    // O contraste do teste anterior: com `width` declarado ela ocupa mesmo, e as
    // colunas dela repartem os 600.
    iguais!(
        tabelas(r#"<table style="width:600px"><tr><td><table style="width:100%"><tr><td><b></b></td><td><b></b></td></tr></table></td></tr></table>"#, 2),
        [600.0, 600.0]
    );
}

#[test]
fn o_maximo_de_uma_celula_que_contem_tabela_e_o_maximo_da_tabela() {
    // A célula da esquerda vale o que a tabela de dentro quer (200) e a da
    // direita 100: acima do máximo, as duas crescem em proporção ao seu máximo.
    //
    // O selector é `td.fora` e não `td`: em ordem de documento a segunda `<td>`
    // da página é a primeira célula da tabela de DENTRO, e um teste com o nome
    // deste a medir aquela célula passaria a perseguir um número que nunca era o
    // que o nome diz.
    let doc = format!(
        "{BASE}{}",
        r#"<table style="width:600px"><tr><td class="fora"><table><tr><td><b></b></td><td><b></b></td></tr></table></td><td class="fora"><b></b></td></tr></table>"#
    );
    let (dom, list) = geometria(&doc, 900.0);
    let w: Vec<f32> = (0..2).map(|i| rect(&dom, &list, "td.fora", i).w).collect();
    iguais!(w, [400.0, 200.0]);
}

#[test]
fn a_tabela_de_dentro_com_folga_encolhe_a_soma_dos_maximos() {
    // Colunas com folga (50..100) na de dentro: o alvo dela é a soma dos máximos
    // e não o espaço da célula, portanto 200 e não 600.
    iguais!(
        tabelas(r#"<table style="width:600px"><tr><td><table><tr><td><i></i><i></i></td><td><i></i><i></i></td></tr></table></td></tr></table>"#, 2),
        [600.0, 200.0]
    );
}

// ── A forma REAL: o navbox do MediaWiki ──────────────────────────────────────
// Reduzida da página que a régua mede. Traz duas coisas que a escada acabou de
// tocar: uma célula `colspan=2` com `width:100%` e, dentro da tabela aninhada,
// um rótulo com `width:1%` — a `.navbox-group`, que já custou 231px onde o
// Chrome dá 123.

#[test]
fn um_rotulo_a_um_por_cento_fica_no_seu_minimo_e_nao_no_seu_um_por_cento() {
    // 1% de 600 são 6px e o rótulo precisa de 100: a percentagem é levantada
    // até ao mínimo, não respeitada com o conteúdo a transbordar. `th` tem
    // `padding:1px` na folha de UA (lote I) — 2px a mais no rótulo tiram 2px
    // à coluna medida aqui (600 total, mesma soma): 500 → 498.
    iguais!(
        larguras(r#"<table style="width:600px;border-spacing:0"><tr><th style="width:1%"><b></b></th><td><b></b><b></b></td></tr></table>"#, 1),
        [498.0]
    );
}

#[test]
fn uma_tabela_com_coluna_em_percentagem_nao_encolhe_ao_conteudo() {
    // A de dentro tem max-content 300 e fica com 600. Uma coluna a 1% que
    // precisa de 100px implica uma tabela de 10 000 para a satisfazer, e é esse
    // o máximo intrínseco — não a soma dos máximos das colunas. É por isso que
    // ela ocupa a célula toda em vez de encolher.
    iguais!(
        tabelas(r#"<table style="width:600px;border-spacing:0"><tr><th colspan="2"><b></b></th></tr><tr><td colspan="2" style="width:100%"><table style="border-spacing:0"><tr><th style="width:1%"><b></b></th><td><b></b><b></b></td></tr></table></td></tr></table>"#, 2),
        [600.0, 600.0]
    );
}

#[test]
fn uma_celula_com_colspan_e_percentagem_nao_desequilibra_as_colunas() {
    // A célula que atravessa pede 100%; as duas colunas por baixo repartem em
    // partes iguais porque nenhuma delas declara nada.
    iguais!(
        larguras(r#"<table style="width:600px;border-spacing:0"><tr><td colspan="2" style="width:100%"><b></b></td></tr><tr><td><b></b></td><td><i></i><i></i></td></tr></table>"#, 3),
        [600.0, 300.0, 300.0]
    );
}

// ── A raiz do cluster da página real: uma coluna com uma IMAGEM ──────────────
// Emparelhando o dump do Chrome com o nosso na `pagina.combinada.html`, a cadeia
// inteira de 21 ancestrais bate a ZERO até um sítio só: uma coluna de navbox com
// uma imagem, que recebe 5px onde o Chrome dá 154. A `<img>` é medida bem — 152px
// nos dois motores — mas a COLUNA não a vê, colapsa, e as duas vizinhas absorvem
// o que ela devia ocupar. Era isto o cluster, e não o aninhamento.

#[test]
fn uma_coluna_com_imagem_vale_a_largura_da_imagem() {
    iguais!(
        larguras(r#"<table style="width:600px;border-spacing:0"><tr><td><b></b></td><td><img width="152" height="117" alt=""></td></tr></table>"#, 2),
        [238.09, 361.91]
    );
}

#[test]
fn a_imagem_conta_mesmo_dentro_de_um_span_e_de_um_a() {
    // A forma da página real: a miniatura vem embrulhada em `<span><a>`, e o
    // embrulho não pode esconder a largura de quem está lá dentro.
    iguais!(
        larguras(r#"<table style="width:600px;border-spacing:0"><tr><td><b></b></td><td><span><a><img width="152" height="117" alt=""></a></span></td></tr></table>"#, 2),
        [238.09, 361.91]
    );
}

#[test]
fn uma_largura_declarada_de_um_pixel_e_levantada_ate_a_imagem() {
    // A forma EXATA da página: `.navbox-image` do MediaWiki escreve
    // `width:1px;padding:0 0 0 2px` na célula da miniatura. Um pixel não cabe, e
    // uma largura declarada que não cabe é levantada até ao mínimo do conteúdo —
    // que numa imagem é a imagem. O Chrome dá 154 = 152 + 2 de padding, que é o
    // número que a página real mostra ao pixel.
    iguais!(
        larguras(r#"<table style="width:600px;border-spacing:0"><tr><td><b></b></td><td style="width:1px;padding:0 0 0 2px"><img width="152" height="117" alt=""></td></tr></table>"#, 2),
        [446.0, 154.0]
    );
}

// ── O predicado que decide se uma caixa fecha a corrida ──────────────────────
// Estes dois existem para pôr o `em_linha` à prova em vez de o assumir. A
// pergunta que ele responde não é "esta caixa é inline?" mas "esta caixa
// partilha a linha com as irmãs?" — e um `inline-block` responde SIM às duas,
// por razões diferentes. O par abaixo separa-as: se o predicado errasse, o
// primeiro caso mediria 50 em vez de 100.

#[test]
fn numa_celula_nowrap_os_inline_block_somam_se() {
    // Sem quebra possível, os dois ficam na mesma linha: o mínimo é 50+50.
    iguais!(
        larguras(r#"<table style="width:60px;border-spacing:0"><tr><td style="white-space:nowrap"><i></i><i></i></td><td><b></b></td></tr></table>"#, 2),
        [100.0, 100.0]
    );
}

#[test]
fn um_bloco_no_meio_quebra_a_linha_mesmo_com_nowrap() {
    // O contraste que dá sentido ao teste anterior: um `display:block` fecha a
    // corrida, e o mínimo volta a ser o MAIOR dos dois e não a soma. O `nowrap`
    // não junta o que o fluxo de bloco separa.
    iguais!(
        larguras(r#"<table style="width:60px;border-spacing:0"><tr><td style="white-space:nowrap"><i></i><div style="display:block"><i></i></div></td><td><b></b></td></tr></table>"#, 2),
        [50.0, 100.0]
    );
}
