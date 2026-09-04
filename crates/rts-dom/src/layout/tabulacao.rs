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
        super::segmento::aplicar_elipse_forcada(ultima, content_w, font_size, mono, m, true);
    lines.extend(cortada);
    lines
}
