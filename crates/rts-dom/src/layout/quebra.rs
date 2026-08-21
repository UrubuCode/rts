//! QUEBRA DE LINHA: decidir onde os runs passam para a linha seguinte.
//!
//! **455 linhas.** O `wrap_runs` tem 408 e não é partido por dentro: partir uma
//! função deixa de ser um movimento de código. Tem dois `macro_rules!` no corpo
//! (`fechar_cluster`, `juntar`) que fecham com `    }` a quatro espaços — quem
//! cortar este ficheiro por blocos em vez de por item de topo fecha blocos
//! falsos no meio da função, e ali isso não dá erro de compilação.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
pub(in crate::layout) fn wrap_runs(
    runs: &[InlineRun],
    // A largura disponível DA LINHA `i` — não uma largura só para todas. Um
    // float encurta uma linha e deixa a seguinte inteira, e a diferença entre as
    // duas é o que faz o texto contornar a figura em vez de descer abaixo dela.
    max_w: &mut dyn FnMut(usize) -> f32,
    font_size: f32,
    mono: bool,
    // Pode partir-se DENTRO de um aglomerado? Vem do elemento que possui o
    // fluxo, e não de cada run: `word-break`/`overflow-wrap` são herdadas e o
    // corpus real escreve-as sempre no container (13 folhas, zero excepções).
    // Guardá-las por run era a alternativa e custava um campo em cada `InlineRun`
    // para responder o mesmo valor em todos eles.
    quebra: crate::inline_box::QuebraDentro,
    m: &dyn TextMeasurer,
) -> Vec<Vec<Segment>> {
    let _phase = crate::metrics::phases::scope("wrap-runs");
    // A largura do espaço só interessa ao caminho palavra-a-palavra. Medida
    // sempre, era metade de todas as medições de texto de um relayout — uma por
    // chamada, mesmo quando o fast path respondia sozinho.
    let mut space_w_memo: Option<f32> = None;
    let mut space_w = |m: &dyn TextMeasurer| -> f32 {
        *space_w_memo.get_or_insert_with(|| m.text_width(" ", font_size, mono, false, false))
    };
    let mut lines: Vec<Vec<Segment>> = Vec::new();
    let mut cur: Vec<Segment> = Vec::new();
    let mut cur_w = 0.0f32;
    let mut at_line_start = true;
    // havia whitespace no ORIGINAL desde a última palavra? (carrega entre runs)
    let mut pending_space = false;

    // -- O CLUSTER: a unidade que a linha move.
    //
    // Uma linha so pode quebrar numa OPORTUNIDADE DE QUEBRA, e no texto essa
    // oportunidade e o whitespace. Entre dois runs colados -- `<span>[</span>`
    // seguido de `<span>135</span>`, a marcacao de referencia do MediaWiki --
    // nao existe nenhuma, e o Chrome desce o `[135]` inteiro para a linha
    // seguinte. Decidir peca a peca partia-o ao meio: medido na Wikipedia, um
    // `<a>` com fragmentos de 8px no canto direito da linha e 24px no inicio da
    // seguinte, e a caixa dele passava a ser a uniao dos dois -- 752 de largura
    // onde o Chrome da 21.
    //
    // Por isso as pecas sem oportunidade entre elas sao acumuladas aqui e a
    // pergunta "cabe?" e feita ao conjunto, uma vez. Nao e uma regra nova: e a
    // regra do CSS aplicada a unidade certa. Uma peca sozinha, que e o caso
    // esmagador, comporta-se exatamente como antes.
    struct Peca {
        run: usize,
        texto: String,
        largura: f32,
        atomico: Option<(NodeIdx, AtomicKind, f32, f32)>,
    }
    let mut cluster: Vec<Peca> = Vec::new();
    let mut cluster_w = 0.0f32;
    // havia whitespace ANTES do cluster? e esse whitespace veio de FORA do run
    // que abre o cluster? (a segunda pergunta decide de quem e o vao -- ver o
    // `lead_w` do `Segment`.)
    let mut cluster_espaco = false;
    let mut cluster_de_fora = false;
    // o whitespace pendente veio de um run ANTERIOR (e nao de dentro deste)?
    let mut espaco_de_fora = false;

    macro_rules! fechar_cluster {
        () => {
            if !cluster.is_empty() {
                let sep = cluster_espaco && !at_line_start;
                let need = if sep {
                    space_w(m) + cluster_w
                } else {
                    cluster_w
                };
                // `break-all` ENCHE a linha corrente antes de descer, e por isso
                // salta a quebra prévia: descer primeiro e partir depois deixava
                // à direita um vazio do tamanho da palavra, que é exatamente o
                // que `break-all` existe para não deixar. Só vale para um
                // aglomerado todo de texto — uma caixa atómica (um `<img>`, um
                // widget) é inquebrável e continua a descer inteira.
                let so_texto = cluster.iter().all(|p| p.atomico.is_none());
                let enche_a_linha =
                    quebra == crate::inline_box::QuebraDentro::Sempre && so_texto;
                if !at_line_start && !enche_a_linha && cur_w + need > max_w(lines.len()) {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0.0;
                    at_line_start = true;
                }
                // recalculado DEPOIS da quebra: um cluster que abre linha nao
                // leva o espaco com ele.
                let sep = cluster_espaco && !at_line_start;
                let mut primeiro = true;
                for peca in cluster.drain(..) {
                    let run = &runs[peca.run];
                    let com_espaco = primeiro && sep;
                    let espaco = if com_espaco { space_w(m) } else { 0.0 };
                    match peca.atomico {
                        Some((a_idx, kind, ww, wh)) => {
                            cur.push(Segment {
                                text: String::new(),
                                text_width: 0.0,
                                color: run.color,
                                bold: false,
                                italic: false,
                                deco: 0,
                                owners: run.owners.clone(),
                                atomic: Some((a_idx, kind)),
                                ww,
                                wh,
                                lead_w: espaco,
                            });
                            cur_w += ww + espaco;
                        }
                        None => {
                            let vao = if com_espaco && cluster_de_fora {
                                espaco
                            } else {
                                0.0
                            };
                            let mut texto = String::with_capacity(peca.texto.len() + 1);
                            if com_espaco {
                                texto.push(' ');
                            }
                            texto.push_str(&peca.texto);
                            let largura = peca.largura + espaco;
                            // PARTIR DENTRO DA PALAVRA — o que `overflow-wrap` e
                            // `word-break` ligam. A pergunta faz-se aqui, na
                            // emissão de uma peça, porque é aqui que já se sabe
                            // quanto resta da linha; fazê-la antes, sobre o
                            // aglomerado inteiro, obrigava a uma segunda regra de
                            // quebra ao lado da que já existe.
                            let disponivel = max_w(lines.len());
                            let partir = match quebra {
                                crate::inline_box::QuebraDentro::Nao => false,
                                // `break-word`: só quando a palavra não cabe NEM
                                // numa linha vazia. Se cabe, ela já desceu inteira
                                // na quebra prévia e parti-la seria errado.
                                crate::inline_box::QuebraDentro::SePreciso => {
                                    peca.largura > disponivel
                                }
                                crate::inline_box::QuebraDentro::Sempre => {
                                    cur_w + largura > disponivel
                                }
                            };
                            if partir {
                                let mut resto = texto.as_str();
                                let mut lead = vao;
                                while !resto.is_empty() {
                                    let disp = max_w(lines.len()) - cur_w;
                                    let (mut n, mut w) = crate::inline_box::prefixo_que_cabe(
                                        resto,
                                        disp,
                                        font_size,
                                        mono,
                                        run.bold,
                                        run.italic,
                                        m,
                                    );
                                    if n == 0 && at_line_start {
                                        // Numa caixa mais estreita que um glifo,
                                        // nada cabe e descer de linha não muda
                                        // isso: sem um carácter forçado o laço
                                        // não termina. Transbordar um carácter é
                                        // o que o browser também faz.
                                        n = resto.chars().next().map_or(0, char::len_utf8);
                                        w = m.text_width(
                                            &resto[..n],
                                            font_size,
                                            mono,
                                            run.bold,
                                            run.italic,
                                        );
                                    }
                                    if n == 0 {
                                        lines.push(std::mem::take(&mut cur));
                                        cur_w = 0.0;
                                        at_line_start = true;
                                        continue;
                                    }
                                    push_segment(&mut cur, run, &resto[..n], w, lead);
                                    lead = 0.0;
                                    cur_w += w;
                                    at_line_start = false;
                                    resto = &resto[n..];
                                    if !resto.is_empty() {
                                        lines.push(std::mem::take(&mut cur));
                                        cur_w = 0.0;
                                        at_line_start = true;
                                    }
                                }
                            } else {
                                push_segment(&mut cur, run, &texto, largura, vao);
                                cur_w += largura;
                            }
                        }
                    }
                    primeiro = false;
                    at_line_start = false;
                }
                cluster_w = 0.0;
                cluster_espaco = false;
                cluster_de_fora = false;
            }
        };
    }
    // acrescenta uma peca ao cluster corrente, abrindo-o se estiver vazio.
    macro_rules! juntar {
        ($peca:expr, $w:expr) => {{
            if cluster.is_empty() {
                cluster_espaco = pending_space;
                cluster_de_fora = espaco_de_fora;
            }
            cluster.push($peca);
            cluster_w += $w;
            pending_space = false;
            espaco_de_fora = false;
        }};
    }

    for (i, run) in runs.iter().enumerate() {
        // WIDGET: uma "palavra" inquebravel de run.ww pontos, segmento proprio.
        if let Some((a_idx, kind)) = run.atomic {
            // BREAK: entra na linha (para receber a sua caixa) e FECHA-A.
            if kind == AtomicKind::Break {
                fechar_cluster!();
                cur.push(Segment {
                    text: String::new(),
                    text_width: 0.0,
                    color: run.color,
                    bold: false,
                    italic: false,
                    deco: 0,
                    owners: run.owners.clone(),
                    atomic: Some((a_idx, AtomicKind::Break)),
                    ww: 0.0,
                    wh: 0.0,
                    lead_w: 0.0,
                });
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
                pending_space = false;
                espaco_de_fora = false;
                continue;
            }
            // MARKER: largura zero, nao quebra a linha, nao consome o espaco
            // pendente -- so marca uma posicao para quem lhe quiser a caixa.
            if kind == AtomicKind::Marker {
                fechar_cluster!();
                cur.push(Segment {
                    text: String::new(),
                    text_width: 0.0,
                    color: run.color,
                    bold: false,
                    italic: false,
                    deco: 0,
                    owners: run.owners.clone(),
                    atomic: Some((a_idx, AtomicKind::Marker)),
                    ww: 0.0,
                    wh: 0.0,
                    lead_w: 0.0,
                });
                continue;
            }
            juntar!(
                Peca {
                    run: i,
                    texto: String::new(),
                    largura: run.ww,
                    atomico: Some((a_idx, kind, run.ww, run.wh)),
                },
                run.ww
            );
            continue;
        }
        // so whitespace: vira separador pendente e nao abre peca. Decidido ANTES
        // de normalizar, porque um separador pendente faz a normalizacao
        // devolver " " -- nao-vazio -- e o run deixaria de ser reconhecido como
        // o separador que e.
        if !run.text.is_empty() && so_espaco_css(&run.text) {
            fechar_cluster!();
            pending_space = true;
            espaco_de_fora = true;
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        // O espaco da frente e devido quando havia whitespace desde a ultima
        // palavra, esteja ele no fim do run ANTERIOR ou no inicio deste.
        if run.text.starts_with(e_espaco_css) {
            fechar_cluster!();
            pending_space = true;
            // NAO e vao: este espaco esta no texto DESTE run, logo pertence aos
            // donos dele e vive dentro do segmento. So o espaco que vem de um
            // run ANTERIOR e um vao. E a diferenca entre `<a> alvo</a>` e
            // `antes <a>alvo</a>` -- o `::after` com `content:" (…)"` e o
            // primeiro caso, e o espaco tem de sobreviver no texto.
            espaco_de_fora = false;
        }
        // FAST PATH: o run inteiro e UMA peca quando nao tem whitespace dentro.
        //
        // Medir a string inteira e o que um browser faz, e e o que evita uma
        // medicao por palavra: `wrap-runs` era 38% de um relayout de pagina
        // grande, com 11 000 `text_width` por frame.
        let miolo = apara_css(&run.text);
        if !miolo.contains(e_espaco_css) {
            let w = m.text_width(miolo, font_size, mono, run.bold, run.italic);
            let terminava_em_espaco = run.text.ends_with(e_espaco_css);
            juntar!(
                Peca {
                    run: i,
                    texto: miolo.to_string(),
                    largura: w,
                    atomico: None
                },
                w
            );
            if terminava_em_espaco {
                fechar_cluster!();
                pending_space = true;
                espaco_de_fora = true;
            }
            continue;
        }
        // FAST PATH 2 — o run INTEIRO cabe na linha corrente.
        //
        // Medir palavra a palavra custa uma medicao por palavra, e medir texto e
        // a unica coisa que o layout pede ao backend: `wrap-runs` era 38% de um
        // relayout de pagina grande, com 11 000 `text_width` por frame. Quando o
        // run cabe todo, uma medicao responde por todas.
        //
        // So e seguro sob duas condicoes, e as duas sao sobre CLUSTERS: o run
        // tem de ABRIR um (senao a sua primeira palavra pertence ao aglomerado
        // que vem de tras e nao pode ser commitada sozinha) e tem de FECHAR um
        // (senao a sua ultima palavra pode ainda vir a ter de descer com o run
        // seguinte). Sem as duas, o caminho lento e o que responde certo.
        let abre_cluster = cluster.is_empty();
        let fecha_cluster = run.text.ends_with(e_espaco_css);
        if abre_cluster && fecha_cluster {
            let normalizado = collapse_ws(&run.text, pending_space && !at_line_start);
            if !normalizado.is_empty() {
                let w = m.text_width(&normalizado, font_size, mono, run.bold, run.italic);
                if !at_line_start && cur_w + w <= max_w(lines.len()) {
                    let vao = if pending_space && espaco_de_fora {
                        space_w(m)
                    } else {
                        0.0
                    };
                    push_segment(&mut cur, run, &normalizado, w, vao);
                    cur_w += w;
                    at_line_start = false;
                    pending_space = true;
                    espaco_de_fora = true;
                    continue;
                }
            }
        }
        // scanner ws/palavra: cada whitespace FECHA o cluster (e uma
        // oportunidade de quebra) e cada palavra abre o seguinte.
        let mut rest = run.text.as_str();
        while !rest.is_empty() {
            if rest.starts_with(e_espaco_css) {
                fechar_cluster!();
                pending_space = true;
                espaco_de_fora = false;
                rest = rest.trim_start_matches(e_espaco_css);
                continue;
            }
            let end = rest.find(e_espaco_css).unwrap_or(rest.len());
            let word = &rest[..end];
            rest = &rest[end..];
            let ww = m.text_width(word, font_size, mono, run.bold, run.italic);
            juntar!(
                Peca {
                    run: i,
                    texto: word.to_string(),
                    largura: ww,
                    atomico: None
                },
                ww
            );
        }
        if run.text.ends_with(e_espaco_css) {
            fechar_cluster!();
            pending_space = true;
            espaco_de_fora = true;
        }
    }
    fechar_cluster!();
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(vec![Segment {
            text: String::new(),
            text_width: 0.0,
            color: 0,
            bold: false,
            italic: false,
            deco: 0,
            owners: Vec::new(),
            atomic: None,
            ww: 0.0,
            wh: 0.0,
            lead_w: 0.0,
        }]);
    }
    lines
}

/// Quebra `text` em LINHAS que cabem em `max_w` (word-wrap do CSS `white-space:
/// normal`): acumula palavras separadas por espaço; quando a próxima não cabe,
/// fecha a linha e começa outra. Uma palavra maior que `max_w` fica sozinha na
/// linha (não quebra no meio da palavra — `overflow-wrap:normal`).
fn wrap_text(
    text: &str,
    max_w: f32,
    font_size: f32,
    mono: bool,
    m: &dyn TextMeasurer,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0.0f32;
    let space_w = m.text_width(" ", font_size, mono, false, false);
    for word in text.split_whitespace() {
        let word_w = m.text_width(word, font_size, mono, false, false);
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else if current_w + space_w + word_w <= max_w {
            current.push(' ');
            current.push_str(word);
            current_w += space_w + word_w;
        } else {
            // não cabe: fecha a linha atual e começa nova com a palavra.
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_w = word_w;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
