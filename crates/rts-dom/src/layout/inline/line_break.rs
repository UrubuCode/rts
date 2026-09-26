//! QUEBRA DE LINHA: decidir onde os runs passam para a linha seguinte.
//!
//! **No teto de 500.** O `wrap_runs` é a maior parte disto e não é partido
//! por dentro: tem três `macro_rules!` no corpo (`fechar_cluster`, `juntar`,
//! `glue_space`) que capturam uma dúzia de locais cada — mover UM deles para
//! outro ficheiro obriga a mover TODOS os locais que captura, e o que sobra
//! deixa de ser um movimento de código. O que não toca nos macros já saiu:
//! o hífen suave em `hyphen.rs`, e a partição de peça/o atalho de run inteiro
//! em `line_break_partition.rs`.

use super::*;
use super::preserved_spaces::{trim_hanging, tokens, atomic_segment, Token};
pub(in crate::layout) fn wrap_runs(
    runs: &[InlineRun],
    // A largura disponível DA LINHA `i` — não uma largura só para todas. Um
    // float encurta uma linha e deixa a seguinte inteira, e a diferença entre as
    // duas é o que faz o texto contornar a figura em vez de descer abaixo dela.
    max_w: &mut dyn FnMut(usize) -> f32,
    line_offset: &mut dyn FnMut(usize) -> f32, // line `i`'s start vs. content edge — `Spaces::tab`
    font_size: f32,
    mono: bool,
    // Pode partir-se DENTRO de um aglomerado? Vem do elemento que possui o
    // fluxo, e não de cada run: `word-break`/`overflow-wrap` são herdadas e o
    // corpus real escreve-as sempre no container (13 folhas, zero excepções).
    // Guardá-las por run era a alternativa e custava um campo em cada `InlineRun`
    // para responder o mesmo valor em todos eles.
    quebra: crate::inline_box::QuebraDentro,
    // `white-space`/`tab-size` of EACH RUN (unlike `quebra` above): whether a
    // `\n` forces a break, and whether spaces are content (`preserved_spaces.rs`).
    spaces: super::preserved_spaces::Spaces,
    // `word-spacing` (px, pode ser negativo) — soma-se à largura de CADA espaço
    // entre palavras. Entra aqui e não só na pintura porque é o mesmo número
    // que decide ONDE a linha quebra: medir sem ele e pintar com ele (ou
    // vice-versa) seriam as "duas verdades" que `letter-spacing` já pagou.
    word_spacing: f32,
    // `hyphens` do container: `manual`/`auto` deixam o U+00AD ser oportunidade
    // de quebra (`hyphen.rs`); `none` apaga-o antes de medir.
    hifen_manual: bool,
    // The font of each run — the container's, or its innermost inline's where
    // that differs (`run_font.rs`). See `medir`.
    fontes: &super::run_font::Fontes,
    m: &dyn TextMeasurer,
) -> Vec<Vec<Segment>> {
    let _phase = crate::metrics::phases::scope("wrap-runs");
    let ahem = fontes.base_ahem();
    // `i` is the run the text belongs to; `usize::MAX` is the container's own.
    let medir = |m: &dyn TextMeasurer, i: usize, t: &str, bold: bool, italic: bool| -> f32 { fontes.largura(m, i, t, bold, italic) };
    // A largura do espaço só interessa ao caminho palavra-a-palavra. Medida
    // sempre, era metade de todas as medições de texto de um relayout — uma por
    // chamada, mesmo quando o fast path respondia sozinho.
    // A space is as wide as the font of the run it is IN: `i` inside run `i`,
    // `i − 1` when it came from the run before (0 wraps to the container's).
    let space_w = |m: &dyn TextMeasurer, i: usize| -> f32 { medir(m, i, " ", false, false) + word_spacing };
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
        atomico: Option<(NodeIdx, crate::boxes::BoxId, AtomicKind, f32, f32)>,
    }
    let mut cluster: Vec<Peca> = Vec::new();
    let mut cluster_w = 0.0f32;
    // The preserved spaces/tabs that END the cluster under `pre-wrap`/`pre`:
    // they hang, so they do not count when asking whether it fits. The last
    // PLACED cluster's (width, space width) is what a soft wrap trims.
    let (mut cluster_hang, mut posted_hang) = (0.0f32, (0.0f32, 0.0f32));
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
                let de = if cluster_de_fora { cluster[0].run.wrapping_sub(1) } else { cluster[0].run };
                let need = if sep {
                    space_w(m, de) + cluster_w
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
                // HÍFEN SUAVE (`hyphen.rs`): a palavra que não cabe deixa na linha
                // o prefixo com "-" e o aglomerado esvazia — o laço abaixo não emite.
                if hifen_manual
                    && !enche_a_linha
                    && cluster.len() == 1
                    && cluster[0].atomico.is_none()
                    && cluster[0].texto.contains(hyphen::SHY)
                    && cur_w + need > max_w(lines.len())
                {
                    let (sep_w, vao, peca) =
                        (if sep { space_w(m, de) } else { 0.0 }, sep && cluster_de_fora, &cluster[0]);
                    if hyphen::emitir_com_hifen(
                        &mut cur, &mut lines, &mut cur_w, &mut at_line_start,
                        &runs[peca.run], &peca.texto, peca.largura, sep_w, vao,
                        max_w, font_size, mono, ahem, m,
                    ) {
                        cluster.clear();
                    }
                }
                if !cluster.is_empty()
                    && !at_line_start
                    && !enche_a_linha
                    && cur_w + need - cluster_hang > max_w(lines.len())
                {
                    lines.push(trim_hanging(std::mem::take(&mut cur), posted_hang));
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
                    let espaco = if com_espaco { space_w(m, de) } else { 0.0 };
                    match peca.atomico {
                        Some((a_idx, caixa, kind, ww, wh)) => {
                            cur.push(atomic_segment(run, (a_idx, caixa, kind), ww, wh, espaco));
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
                            texto.push_str(&hyphen::sem_shy(&peca.texto));
                            let largura = peca.largura + espaco;
                            // PARTIR DENTRO DA PALAVRA — o que `overflow-wrap` e
                            // `word-break` ligam. A pergunta faz-se aqui, na
                            // emissão de uma peça, porque é aqui que já se sabe
                            // quanto resta da linha; fazê-la antes, sobre o
                            // aglomerado inteiro, obrigava a uma segunda regra de
                            // quebra ao lado da que já existe.
                            let disponivel = max_w(lines.len());
                            // A hanging sequence (`pre-wrap`) is never split; a
                            // `break-spaces` space may move down alone.
                            let partir = (spaces.of(peca.run).breaks_after_each() || !so_espaco_css(&peca.texto)) && match quebra {
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
                                // Moved to `line_break_partition.rs` (teto de 500).
                                super::line_break_partition::dividir_peca_que_nao_cabe(
                                    &mut cur, &mut lines, &mut cur_w, &mut at_line_start,
                                    &mut *max_w, run, peca.run, &texto, vao,
                                    font_size, mono, ahem, fontes, m,
                                );
                            } else {
                                push_segment(&mut cur, run, &texto, largura, vao);
                                cur_w += largura;
                            }
                        }
                    }
                    primeiro = false;
                    at_line_start = false;
                }
                (cluster_w, posted_hang) = (0.0, (cluster_hang, space_w(m, de)));
                cluster_hang = 0.0;
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
    // A collapsible space at a boundary whose run FORBIDS automatic wrapping
    // (`nowrap`, `pre` — `WhiteSpaceRegime::wraps`): closing the cluster here
    // like an ordinary space would DRAIN it into `cur` early, and a LATER
    // close — by a run that DOES allow wrapping, such as the plain text after
    // a `<span style="white-space:nowrap">` — would then see only the tail of
    // the nowrap span still open and could break the LINE in the middle of
    // it (`nowrap-span-glues-only-its-own-spaces`: "bb cc dd" landed as "bb
    // cc" on one line and "dd" alone on the next). So instead of closing, the
    // space becomes an ordinary PIECE of the same open cluster — exactly how
    // the PRESERVED branch already represents a space (`Token::Space`,
    // above) — so the whole nowrap run stays ONE cluster and is measured as
    // one indivisible unit whenever a real wrap opportunity eventually closes
    // it.
    // §4.1.3 phase II (see `preserved_spaces::collapses_at_line_start`): a
    // glued space still collapses away at a line start. And
    // `glued_space_absorbs_pending`: a `pending_space` already due when this
    // run's OWN whitespace is glued is the SAME collapsible run split across
    // the regime boundary — consumed here so `juntar!` does not also turn it
    // into a second, cluster-level separator on top of the piece.
    macro_rules! glue_space {
        ($i:expr) => {{
            if !super::preserved_spaces::collapses_at_line_start(cluster.is_empty(), at_line_start) {
                if super::preserved_spaces::glued_space_absorbs_pending(cluster.is_empty(), pending_space) {
                    pending_space = false;
                    espaco_de_fora = false;
                }
                let w = space_w(m, $i);
                juntar!(Peca { run: $i, texto: " ".to_string(), largura: w, atomico: None }, w);
            }
        }};
    }

    for (i, run) in runs.iter().enumerate() {
        // WIDGET: uma "palavra" inquebravel de run.ww pontos, segmento proprio.
        if let Some((a_idx, caixa, kind)) = run.atomic {
            // BREAK: entra na linha (para receber a sua caixa) e FECHA-A.
            if kind == AtomicKind::Break {
                fechar_cluster!();
                cur.push(atomic_segment(run, (a_idx, caixa, AtomicKind::Break), 0.0, 0.0, 0.0));
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
                pending_space = false;
                espaco_de_fora = false;
                continue;
            }
            // MARKER: largura zero, nao quebra a linha, nao consome o espaco
            // pendente -- so marca uma posicao para quem lhe quiser a caixa.
            // A float's ANCHOR is the same: it only says which line the float
            // appeared on; its width enters through the exclusions, not the line.
            if matches!(kind, AtomicKind::Marker | AtomicKind::Float | AtomicKind::Estatica) {
                fechar_cluster!();
                cur.push(atomic_segment(run, (a_idx, caixa, kind), 0.0, 0.0, 0.0));
                continue;
            }
            juntar!(
                Peca {
                    run: i,
                    texto: String::new(),
                    largura: run.ww,
                    atomico: Some((a_idx, caixa, kind, run.ww, run.wh)),
                },
                run.ww
            );
            continue;
        }
        // PRESERVED white space (`preserved_spaces.rs`): every space and tab is a
        // piece of the cluster — no opportunity BEFORE it (UAX #14) — and the
        // cluster closes after each one (`break-spaces`) or after the whole
        // sequence, which then hangs (`pre-wrap`, `pre`). Nothing is pending.
        let regime = spaces.of(i);
        if regime.preserves() {
            let mut toks = tokens(&run.text).peekable();
            while let Some(f) = toks.next() {
                let (texto, w) = match f {
                    Token::Break => {
                        fechar_cluster!();
                        lines.push(std::mem::take(&mut cur));
                        (cur_w, at_line_start) = (0.0, true);
                        continue;
                    }
                    Token::Word(p) => (hyphen::texto_da_peca(p, hifen_manual), medir(m, i, &hyphen::sem_shy(p), run.bold, run.italic)),
                    Token::Space => (" ".to_string(), space_w(m, i)),
                    Token::Tab => regime.tab(cur_w + cluster_w + line_offset(lines.len()), space_w(m, i)),
                };
                juntar!(Peca { run: i, texto, largura: w, atomico: None }, w);
                let each = regime.breaks_after_each();
                if f.is_white() && !each {
                    cluster_hang += w;
                }
                // Un-drained (see `glue_space!`) when this run forbids
                // wrapping: the space is already a PIECE in the cluster
                // (`juntar!` above), so nothing is lost by not closing —
                // closing early is what would let a later, wrap-allowed
                // boundary break in the middle of this run's content.
                if f.is_white() && (each || !toks.peek().is_some_and(Token::is_white)) && regime.wraps() {
                    fechar_cluster!();
                }
            }
            continue;
        }
        // so whitespace: vira separador pendente e nao abre peca. Decidido ANTES
        // de normalizar, porque um separador pendente faz a normalizacao
        // devolver " " -- nao-vazio -- e o run deixaria de ser reconhecido como
        // o separador que e.
        if !run.text.is_empty() && so_espaco_css(&run.text) {
            // Run TODO whitespace com um `\n` dentro (`<div
            // style="white-space:pre">\n</div>` sem mais texto) — o caso
            // degenerado do scanner abaixo, sem palavra que o alcance.
            if regime.preserves_newlines() && run.text.contains('\n') {
                fechar_cluster!();
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
                pending_space = false;
                espaco_de_fora = false;
                continue;
            }
            if regime.wraps() {
                fechar_cluster!();
                pending_space = true;
                espaco_de_fora = true;
            } else {
                glue_space!(i);
            }
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        // O espaco da frente e devido quando havia whitespace desde a ultima
        // palavra, esteja ele no fim do run ANTERIOR ou no inicio deste.
        if run.text.starts_with(e_espaco_css) {
            if regime.wraps() {
                fechar_cluster!();
                pending_space = true;
                // NAO e vao: este espaco esta no texto DESTE run, logo pertence aos
                // donos dele e vive dentro do segmento. So o espaco que vem de um
                // run ANTERIOR e um vao. E a diferenca entre `<a> alvo</a>` e
                // `antes <a>alvo</a>` -- o `::after` com `content:" (…)"` e o
                // primeiro caso, e o espaco tem de sobreviver no texto.
                espaco_de_fora = false;
            } else {
                glue_space!(i);
            }
        }
        // As FAST PATHS abaixo julgam pelo texto APARADO ou por `ends_with`, e
        // um run "tres\n" apara para "tres" (sem whitespace interno) — tomaria
        // o caminho rápido e perderia a quebra que estava na borda apagada.
        let tem_quebra_forcada = regime.preserves_newlines() && run.text.contains('\n');
        // FAST PATH: o run inteiro e UMA peca quando nao tem whitespace dentro.
        //
        // Medir a string inteira e o que um browser faz, e e o que evita uma
        // medicao por palavra: `wrap-runs` era 38% de um relayout de pagina
        // grande, com 11 000 `text_width` por frame.
        let miolo = apara_css(&run.text);
        if !miolo.contains(e_espaco_css) && !tem_quebra_forcada {
            let w = medir(m, i, &hyphen::sem_shy(miolo), run.bold, run.italic);
            let terminava_em_espaco = run.text.ends_with(e_espaco_css);
            juntar!(
                Peca {
                    run: i,
                    texto: hyphen::texto_da_peca(miolo, hifen_manual),
                    largura: w,
                    atomico: None
                },
                w
            );
            if terminava_em_espaco {
                if regime.wraps() {
                    fechar_cluster!();
                    pending_space = true;
                    espaco_de_fora = true;
                } else {
                    glue_space!(i);
                }
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
        // `word_spacing != 0`: este caminho mede o run INTEIRO (várias palavras)
        // como UMA string, e o `word-spacing` tem de somar UMA vez por espaço
        // ENTRE palavras — o scanner abaixo já faz isso por peça, este atalho
        // não. Desviar para o scanner é mais lento e correto; inventar um fator
        // aqui seria a mesma "segunda verdade" que `letter-spacing` já pagou.
        if abre_cluster
            && fecha_cluster
            && !tem_quebra_forcada
            && word_spacing == 0.0
            && !run.text.contains(hyphen::SHY)
        {
            let normalizado = collapse_ws(&run.text, pending_space && !at_line_start);
            // Moved to `line_break_partition.rs` (teto de 500).
            if super::line_break_partition::run_inteiro_cabe(
                &mut cur, &mut cur_w, &mut at_line_start, &mut pending_space, &mut espaco_de_fora,
                &mut *max_w, lines.len(), run, i, &normalizado, fontes, m,
            ) {
                continue;
            }
        }
        // scanner ws/palavra: cada whitespace FECHA o cluster (e uma
        // oportunidade de quebra) e cada palavra abre o seguinte.
        let mut rest = run.text.as_str();
        while !rest.is_empty() {
            if rest.starts_with(e_espaco_css) {
                // Um `\n` na corrida de whitespace fecha a linha corrente em
                // vez de virar separador pendente (`quebra_forcada_em`,
                // `inline_box.rs`) — só o PRIMEIRO conta; o resto da corrida,
                // se sobrar, passa por este braço de novo na iteração seguinte.
                if regime.preserves_newlines() {
                    if let Some(apos_nl) = crate::inline_box::quebra_forcada_em(rest) {
                        fechar_cluster!();
                        lines.push(std::mem::take(&mut cur));
                        cur_w = 0.0;
                        at_line_start = true;
                        pending_space = false;
                        espaco_de_fora = false;
                        rest = &rest[apos_nl..];
                        continue;
                    }
                }
                if regime.wraps() {
                    fechar_cluster!();
                    pending_space = true;
                    espaco_de_fora = false;
                } else {
                    glue_space!(i);
                }
                rest = rest.trim_start_matches(e_espaco_css);
                continue;
            }
            let end = rest.find(e_espaco_css).unwrap_or(rest.len());
            let word = &rest[..end];
            rest = &rest[end..];
            let ww = medir(m, i, &hyphen::sem_shy(word), run.bold, run.italic);
            juntar!(
                Peca {
                    run: i,
                    texto: hyphen::texto_da_peca(word, hifen_manual),
                    largura: ww,
                    atomico: None
                },
                ww
            );
        }
        if run.text.ends_with(e_espaco_css) {
            if regime.wraps() {
                fechar_cluster!();
                pending_space = true;
                espaco_de_fora = true;
            } else {
                glue_space!(i);
            }
        }
    }
    fechar_cluster!();
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(vec![super::line_break_partition::linha_vazia()]);
    }
    lines
}
