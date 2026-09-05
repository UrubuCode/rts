//! `tab-size` e `-webkit-line-clamp` — os dois cortam o fluxo já quebrado, não
//! a medida de texto, por isso vivem à parte de `quebra.rs`/`segmento.rs`.
//!
//! Movido para módulo próprio (e não acrescentado a `quebra.rs`, que já está
//! perto do teto de 500) na entrada do lote de texto — ver `PLAN.md` §5.S.

use super::Segment;

/// Expande cada `\t` de `texto` para o número de espaços que o leva ao PRÓXIMO
/// tab-stop de `tab_size` colunas — só chamado sob `white-space: pre`/`pre-wrap`,
/// onde o tab é preservado em vez de colapsar como um espaço qualquer. Devolve o
/// texto expandido e a coluna final (para o próximo run continuar a contagem).
///
/// A COLUNA é contada em CARACTERES desde o último `\n` (ou o início do fluxo),
/// que é o que `tab-size: <número>` pede — não é uma medida em pixels: um tab
/// vale `tab_size` vezes o avanço de um espaço, e um avanço de espaço é um
/// caractere no medidor deste motor (`ApproxMeasurer`/`EguiMeasurer` medem uma
/// string de N espaços como N avanços). Um medidor de largura de coluna FIXA
/// (px) existe no CSS mas não é o que o corpus escreve (`parse_tab_size`
/// recusa-o — ver `style/painting.rs`), por isso não há aqui uma segunda forma
/// a suportar.
///
/// ⚠️ CORTE DECLARADO: a coluna reinicia por CHAMADA (por `InlineRun`), não
/// pela posição real na linha pintada — um tab a meio de um `<span>` dentro de
/// texto sem quebra ainda conta a partir do fragmento em que caiu, e não da
/// margem esquerda do bloco. É a mesma aproximação que o resto do fluxo inline
/// já faz por fronteira de run (`fecha_a_corrida`, `intrinsic_width`); o
/// chamador (`layout_inline_flow`) faz o melhor que pode encadeando a coluna
/// entre runs consecutivos do MESMO fluxo, que cobre o caso comum — um `<pre>`
/// de texto simples, sem elementos inline a meio de uma linha.
pub(in crate::layout) fn expandir_tabs(texto: &str, tab_size: usize, coluna_inicial: usize) -> (String, usize) {
    if !texto.contains('\t') {
        return (texto.to_string(), coluna_desde(texto, coluna_inicial));
    }
    let tab_size = tab_size.max(1);
    let mut out = String::with_capacity(texto.len());
    let mut coluna = coluna_inicial;
    for ch in texto.chars() {
        match ch {
            '\t' => {
                let avanco = tab_size - (coluna % tab_size);
                out.extend(std::iter::repeat(' ').take(avanco));
                coluna += avanco;
            }
            '\n' => {
                out.push('\n');
                coluna = 0;
            }
            c => {
                out.push(c);
                coluna += 1;
            }
        }
    }
    (out, coluna)
}

/// A coluna corrente depois de `texto`, sem tabs dentro (fast path do chamador
/// — não vale a pena percorrer char a char um texto que não tem `\t` nenhum).
fn coluna_desde(texto: &str, coluna_inicial: usize) -> usize {
    match texto.rfind('\n') {
        Some(i) => texto[i + 1..].chars().count(),
        None => coluna_inicial + texto.chars().count(),
    }
}

/// Aplica `tab-size` e `word-spacing` a um texto ANTES de ele ser medido para a
/// largura INTRÍNSECA (`medida::intrinsic_content_width`) — a mesma dupla soma
/// que `wrap_runs`/`layout_inline_flow` fazem para a largura de LINHA, e pela
/// mesma razão: a caixa de um `inline-block`/item flex sem `width` é decidida
/// por ESTA largura, medida ANTES de o fluxo de linha correr — e não pela do
/// `wrap_runs`. As duas funções lerem o mesmo par de propriedades e não
/// concordarem é a classe de defeito que este lote encontrou ao vivo: um
/// `inline-block` com `word-spacing`/`tab-size` media a MESMA largura com ou
/// sem a propriedade, porque só o `wrap_runs` (que corre DEPOIS da caixa
/// decidida) a conhecia.
///
/// Devolve o texto (com os tabs expandidos, se havia) e a largura EXTRA de
/// `word-spacing` a somar (não embutida no texto — é um número, como
/// `letter-spacing` já é no chamador).
pub(in crate::layout) fn ajustar_texto_intrinsico(
    texto: String,
    css: Option<&crate::style::ComputedStyle>,
) -> (String, f32) {
    let preserva_tabs = css
        .and_then(|c| c.white_space)
        .map(|w| w.preserves_spaces())
        .unwrap_or(false);
    let texto = if preserva_tabs && texto.contains('\t') {
        let tab_size = css
            .and_then(|c| c.tab_size)
            .unwrap_or(8.0)
            .round()
            .max(1.0) as usize;
        expandir_tabs(&texto, tab_size, 0).0
    } else {
        texto
    };
    let ws = css.and_then(|c| c.word_spacing).unwrap_or(0.0);
    // nº de separadores de palavra que o texto colapsado terá — a mesma
    // contagem que `wrap_runs`/`collapse_ws` produzem (uma corrida de
    // whitespace é UM separador, não um por carácter).
    let n_espacos = crate::inline_box::palavras_css(&texto)
        .count()
        .saturating_sub(1);
    (texto, n_espacos as f32 * ws)
}

/// `-webkit-line-clamp: N` — mantém só as primeiras `n` linhas já quebradas e
/// fecha a última com reticências quando havia mais. Corta DEPOIS do
/// `wrap_runs` pela mesma razão que `text-overflow` corta depois: o que se
/// limita é a LINHA já formada, não a medida de uma palavra.
///
/// A altura da caixa não é tocada aqui — e não precisa de ser: um bloco de
/// altura `auto` mede-se pelo `cy` que `layout_inline_flow` devolve, e esse
/// `cy` só avança pelas linhas que sobram desta lista. Cortar a lista ANTES da
/// emissão é o mesmo que cortar a altura, sem um segundo cálculo.
pub(in crate::layout) fn aplicar_line_clamp(
    mut lines: Vec<Vec<Segment>>,
    n: usize,
    content_w: f32,
    font_size: f32,
    mono: bool,
    ahem: bool,
    m: &dyn crate::layout::TextMeasurer,
) -> Vec<Vec<Segment>> {
    if n == 0 || lines.len() <= n {
        return lines;
    }
    lines.truncate(n);
    // a última linha mantida ganha "…" pelo MESMO cortador que `text-overflow`
    // usa — envolvê-la numa lista de uma linha só reaproveita
    // `aplicar_elipse` sem uma segunda função "corta e junta reticências".
    let ultima = vec![lines.pop().expect("n > 0 e lines.len() > n")];
    let cortada =
        super::segmento::aplicar_elipse_forcada(ultima, content_w, font_size, mono, ahem, m, true);
    lines.extend(cortada);
    lines
}
