//! A LARGURA das colunas — a parte do layout de tabela que ninguém acerta por
//! intuição, e por isso a que vive numa pasta só sua.
//!
//! Aqui mede-se: o que cada célula exige e como as células se juntam em colunas.
//! Como o espaço se reparte por elas está em [`reparticao`], separado porque
//! esta metade lê a árvore e aquela não lê nada.
//!
//! O algoritmo é o do CSS 2.1 §17.5.2 relido no LayoutNG: cada coluna tem uma
//! largura MÍNIMA (o que o conteúdo não consegue encolher mais — a palavra mais
//! larga) e uma MÁXIMA (o que o conteúdo quer sem quebrar), e a largura
//! disponível reparte-se entre as duas proporcionalmente à FOLGA de cada coluna.
//!
//! A tentação é repartir proporcionalmente ao máximo, e é o erro clássico: uma
//! coluna com uma frase longa e outra com um número receberiam larguras na razão
//! dos textos, e o número ficaria com 3px enquanto a frase sobra. Repartir a
//! folga (`max - min`) é o que dá a cada coluna o seu mínimo primeiro e só
//! depois divide o que sobra.

mod reparticao;
pub(crate) use reparticao::{resolve_colunas, resolve_fixo};

use crate::layout::LayoutCtx;
use crate::style::ResolveCtx;
use crate::{Dom, NodeIdx, NodeKind};

/// Uma coluna — ou a restrição que UMA célula lhe impõe: o par (mínimo, máximo)
/// e a CLASSE a que pertence.
///
/// A classe não é decoração. O Blink (`TableTypes::Column`) escolhe um REGIME de
/// repartição e faz crescer nele **uma classe de colunas só**, deixando as
/// outras congeladas; sem saber a classe não há como tratar uma coluna com
/// `width: 200px` de forma diferente de uma coluna de texto que calhou querer
/// 200px, e é essa a raiz da divergência que se mede contra o Chrome.
///
/// Os dois campos de classe são os que decidem tudo o resto:
/// - `percentagem` — `width: 30%` guardado como `30.0`. Não é resolúvel em
///   [`cell_min_max`], porque depende da largura da tabela, que só existe na
///   repartição; por isso viaja até lá em vez de ser deitado fora.
/// - `restringida` — a célula DECLARA uma largura (é o `is_constrained` do
///   Blink: "tem um `inline-size` que não é `auto`"). Percentagem também conta.
///
///
/// **Não há aqui um `percent_border_padding`, e a ausência foi medida.** O
/// raciocínio de que uma percentagem de CSS mede a caixa de CONTEÚDO e por isso
/// a moldura teria de somar-se está certo em geral e é FALSO numa tabela auto: o
/// Blink só usa esse termo quando a tabela é `fixed` — a linha na source di-lo
/// nessas palavras. O Chrome dá 180px a uma `width:30%` de uma tabela de 600,
/// com padding ou sem ele, em `content-box` ou em `border-box`. O campo chegou a
/// existir aqui, com o sinal a somar, e nenhum caso sintético o apanhava porque
/// todos tinham `padding: 0`.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Coluna {
    pub min: f32,
    pub max: f32,
    pub percentagem: Option<f32>,
    pub restringida: bool,
}

impl Coluna {
    /// Junta a restrição de mais uma célula (o `Encompass` do Blink): o mínimo e
    /// o máximo sobem, e as bandeiras de classe acumulam — basta UMA célula
    /// declarar largura para a coluna deixar de ser texto livre.
    ///
    /// A percentagem fica com a MAIOR das pedidas, e não com a primeira: duas
    /// células da mesma coluna a pedir 20% e 30% deixam a coluna com 30%, que é
    /// o que satisfaz as duas.
    fn absorve(&mut self, o: Coluna) {
        self.min = self.min.max(o.min);
        self.max = self.max.max(o.max);
        self.restringida |= o.restringida;
        self.percentagem = match (self.percentagem, o.percentagem) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }
}

/// As larguras intrínsecas de UMA célula, já com o frame (padding+borda) dentro:
/// a coluna é dimensionada em caixa de BORDA, porque é isso que ocupa espaço na
/// linha.
pub(crate) fn cell_min_max(dom: &Dom, id: NodeIdx, parent_font: f32, ctx: &LayoutCtx) -> Coluna {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = crate::layout::font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: ctx.viewport_w,
        node_font_size: font,
        root_font_size: crate::layout::DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let border_box = css.border_box.unwrap_or(false);
    let frame = css.padding.resolve_h(&resolve) + 2.0 * css.border_width.unwrap_or(0.0);
    let mono = css
        .font_family
        .as_deref()
        .map(crate::style::is_mono_family)
        .unwrap_or(false);

    // A CLASSE, decidida antes das larguras porque não depende delas: uma célula
    // que declara `width` — em pixels ou em percentagem — restringe a coluna,
    // mesmo quando o valor declarado acaba por ser levantado pelo conteúdo.
    let percentagem = percentagem_declarada(dom, id, css.width);
    let restringida = percentagem.is_some()
        || largura_absoluta(css.width, &resolve).is_some()
        || largura_de_atributo(dom, id).is_some();

    // Um `width` explícito na célula (ou o atributo HTML `width`, que páginas
    // reais ainda usam) FIXA a coluna: entra como mínimo E como máximo, senão a
    // repartição por folga ignorá-lo-ia — a folga de uma coluna fixa é zero.
    let fixo = largura_absoluta(css.width, &resolve)
        .map(|w| if border_box { w } else { w + frame })
        .or_else(|| largura_de_atributo(dom, id));
    if let Some(w) = fixo {
        // Ainda assim não pode ficar ABAIXO do mínimo do conteúdo: uma largura
        // que não cabe é ignorada pelo browser, não respeitada com o texto a
        // transbordar.
        let piso = min_content(dom, id, font, ctx, false, mono, true);
        return Coluna {
            min: w.max(piso),
            max: w.max(piso),
            percentagem,
            restringida,
        };
    }
    // Sem somar `frame`: o `min_content` de um elemento já inclui a moldura DELE,
    // e somá-la aqui contava o padding da célula duas vezes. Ficou invisível
    // enquanto uma célula com `width` declarado devolvia a largura e voltava
    // atrás — o caminho que somava duas vezes só era percorrido pelas outras.
    let min = min_content(dom, id, font, ctx, false, mono, true);
    Coluna {
        min,
        percentagem,
        restringida,
        // O MÁXIMO vem da medição intrínseca que o resto do motor já usa, em vez
        // de uma segunda escrita da mesma travessia. Herda dela uma limitação
        // conhecida: um DESCENDENTE com `width:%` resolve contra a viewport e
        // devolve um máximo inflado, o que enviesa a repartição da folga a favor
        // dessa coluna. Não enviesa o MÍNIMO (ver `largura_absoluta`), que é o
        // que decide se a tabela transborda, e corrigi-lo em
        // `intrinsic_outer_width` mudaria o shrink-to-fit de flex e inline-block
        // na página inteira — uma mudança que precisa da sua própria medição.
        //
        // O `.max(min)` no fim não é defensivo, é uma correção: aquela medição
        // toma o MAIOR filho inline onde a linha os SOMA, por isso responde
        // abaixo do mínimo a uma célula com dois `<a>` lado a lado. Uma folga
        // negativa não é uma folga — `resolve_colunas` divide por ela e a coluna
        // sai mais estreita do que o seu próprio mínimo (64px de conteúdo em
        // 56px de coluna, num teste). Corrigir a medição do layout arrastaria o
        // shrink-to-fit da página inteira, que é medição à parte; aqui a
        // resposta fica coerente consigo mesma.
        max: crate::layout::intrinsic_outer_width(dom, id, parent_font, ctx).max(min),
    }
}

/// O `width` desta caixa quando ele conta para uma medição intrínseca — um
/// comprimento absoluto. A percentagem responde `None` e o porquê está em
/// [`crate::style::dimensao_absoluta`], que é onde a regra vive desde que o
/// mesmo defeito apareceu uma segunda vez, no dimensionamento do flex.
fn largura_absoluta(d: Option<crate::style::Dimension>, resolve: &ResolveCtx) -> Option<f32> {
    crate::style::dimensao_absoluta(d?, resolve)
}

/// A PERCENTAGEM que esta célula declara — `width: 30%` ou `width="30%"` —
/// guardada como `30.0` e não como `0.3`, que é a forma do Blink e a que
/// aparece no CSS.
///
/// Vem em duas fontes e o CSS ganha, como em toda a folha. O atributo é lido
/// porque páginas reais o escrevem (a Wikipédia inteira) e porque
/// [`largura_de_atributo`] o descarta de propósito: ali a percentagem não é
/// resolúvel, aqui não é preciso resolvê-la — é preciso lembrá-la.
///
/// Um `calc()` com componente percentual responde `None`: a percentagem dele não
/// é separável do resto sem resolver o `calc()` inteiro, e resolvê-lo exigiria
/// a largura da tabela.
fn percentagem_declarada(
    dom: &Dom,
    id: NodeIdx,
    css_width: Option<crate::style::Dimension>,
) -> Option<f32> {
    match css_width {
        Some(crate::style::Dimension::Percent(p)) => return Some(p),
        // O CSS declarou OUTRA coisa: o atributo já perdeu e não se lê. Cair para
        // ele aqui daria 30% a uma célula com `width: 200px` e `width="30%"`.
        Some(d) if d != crate::style::Dimension::Auto => return None,
        _ => {}
    }
    let v = dom.node(id).attr("width")?.trim();
    v.strip_suffix('%')?.parse::<f32>().ok().filter(|p| *p > 0.0)
}

/// O atributo HTML `width` de uma célula/coluna (`<td width="120">`, ainda vivo
/// em páginas reais e em toda a Wikipédia). Só a forma em PIXELS: a forma em
/// percentagem precisa da largura da tabela, que ainda não está decidida quando
/// isto corre, e tratá-la como pixels daria uma coluna de 50px onde se pediu
/// metade da tabela.
fn largura_de_atributo(dom: &Dom, id: NodeIdx) -> Option<f32> {
    let v = dom.node(id).attr("width")?.trim();
    if v.ends_with('%') {
        return None;
    }
    v.trim_end_matches("px")
        .parse::<f32>()
        .ok()
        .filter(|w| *w > 0.0)
}

/// A largura que uma célula DECLARA — pelo CSS ou pelo atributo `width` — ou
/// `None` quando não declara nenhuma.
///
/// É a pergunta do `table-layout: fixed`, e é diferente da que [`cell_min_max`]
/// responde: aquela devolve sempre um número (medindo o conteúdo quando não há
/// declaração), e usar esse número como se fosse declarado faz o algoritmo fixo
/// tratar TODAS as colunas como fixas — nenhuma sobra para repartir o resto, e
/// as larguras acabam escaladas em vez de respeitadas. Foi o que um teste
/// apanhou: uma coluna de 50px a sair com 258px.
pub(crate) fn largura_declarada(
    dom: &Dom,
    id: NodeIdx,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> Option<f32> {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = crate::layout::font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: ctx.viewport_w,
        node_font_size: font,
        root_font_size: crate::layout::DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let frame = css.padding.resolve_h(&resolve) + 2.0 * css.border_width.unwrap_or(0.0);
    largura_absoluta(css.width, &resolve)
        .map(|w| {
            if css.border_box.unwrap_or(false) {
                w
            } else {
                w + frame
            }
        })
        .or_else(|| largura_de_atributo(dom, id))
}

/// A largura MÍNIMA do conteúdo de um nó: o ponto em que quebrar mais linhas já
/// não estreita nada — a PALAVRA mais larga.
///
/// Escrito aqui em vez de reusar o `intrinsic_content_width` do layout porque
/// aquele responde a outra pergunta (o máximo, sem quebra nenhuma) e não há como
/// derivar um do outro. A alternativa considerada foi medir o bloco com largura
/// disponível zero e ler o resultado; não serve, porque um bloco sem `width`
/// ocupa o que lhe derem e responderia zero.
///
/// `sem_quebra` é o `white-space` do PAI a chegar ao texto: uma célula
/// `white-space:nowrap` não tem palavra mais larga, tem a frase inteira, e por
/// isso o seu mínimo iguala o máximo — folga zero. Ignorar isto era o defeito
/// que a página real expunha: as `th` das navboxes do MediaWiki (`.navbox-group`
/// é `nowrap`) declaravam uma folga que não existe, recebiam parte do espaço a
/// repartir e ficavam com 231px onde o Chrome dá 123 — com a coluna do lado a
/// pagar a diferença.
pub(in crate::table) fn min_content(
    dom: &Dom,
    id: NodeIdx,
    font: f32,
    ctx: &LayoutCtx,
    sem_quebra: bool,
    // `true` quando a família herdada/declarada é MONOESPAÇADA — decide o
    // avanço por carácter no `TextMeasurer` (`MONO_ADVANCE` vs `PROP_ADVANCE`,
    // `style/text_metrics.rs`). Fixo em `false` era o bug que fazia o piso do
    // `flex-shrink` divergir do `wrap_runs` real em toda fixture `monospace`
    // — `claude-flex-shrink-min-content.html` mediu 294.4px onde o Chrome dá
    // 351.88 (40 carateres × 16px × 0.5498). Recalculado a cada Elemento
    // (abaixo) porque `font-family` é herdada e já vem resolvida no
    // `ComputedStyle` — não precisa de olhar para o pai outra vez.
    mono: bool,
    // `true` (o comportamento de sempre, para a tabela): um `width` DECLARADO
    // é um PISO — a célula nunca fica abaixo da largura que o autor pediu.
    // `false` (para quem quer o min-content PURO, como o piso do
    // `flex-shrink`, spec flexbox §9.7): a spec de flex NÃO trata `width`
    // como mínimo — é só a `flex-basis` de fallback, e o item PODE encolher
    // abaixo dela. Reusar esta função sem o parâmetro (a 1ª versão desta
    // mudança) recusava encolher qualquer item com `width` declarado, que é
    // quase todos — `o_shrink_e_ponderado_pela_base_e_nao_so_pelo_peso` e
    // `flex_shrink_encolhe_em_overflow` apanharam-no.
    floor_width: bool,
) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Text(t) => {
            if sem_quebra {
                // Espaços colapsados mas nenhuma quebra: mede-se o texto todo.
                let junto = t.split_whitespace().collect::<Vec<_>>().join(" ");
                return ctx.measurer.text_width(&junto, font, mono, false, false);
            }
            t.split_whitespace()
                .map(|p| ctx.measurer.text_width(p, font, mono, false, false))
                .fold(0.0f32, f32::max)
        }
        NodeKind::Element { tag } => {
            if crate::layout::is_non_rendered_tag(tag) {
                return 0.0;
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            if css.effective_display() == Some(crate::style::DisplayKind::None) {
                return 0.0;
            }
            let f = crate::layout::font_px(&css, font);
            let mono = css
                .font_family
                .as_deref()
                .map(crate::style::is_mono_family)
                .unwrap_or(mono);
            let resolve = ResolveCtx {
                parent_content_w: ctx.viewport_w,
                node_font_size: f,
                root_font_size: crate::layout::DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let frame = css.padding.resolve_h(&resolve)
                + css.margin.resolve_h(&resolve)
                + 2.0 * css.border_width.unwrap_or(0.0);
            // Uma caixa REPLACED não encolhe abaixo da sua largura — e esta linha
            // dizia-o num comentário enquanto o código só o cumpria quando a
            // largura vinha do CSS. Uma `<img>` com o tamanho nos ATRIBUTOS, ou
            // vindo dos pixels decodificados, respondia ZERO.
            //
            // O custo estava na página real e não num caso de canto: a miniatura
            // de uma navbox do MediaWiki vive numa célula com
            // `width:1px;padding:0 0 0 2px`, e o `.max(piso)` que existe para
            // levantar uma largura declarada que não cabe **não levantava nada,
            // porque o piso era zero**. A coluna ficava com 5px onde o Chrome dá
            // 154, e as duas vizinhas absorviam os 149 — uma larga demais e outra
            // estreita demais, no mesmo par.
            //
            // Pergunta-se ao ÚNICO sítio onde o tamanho de um replaced se decide,
            // que é o mesmo que o `intrinsic_content_width` já consulta para o
            // MÁXIMO. Escrever a regra outra vez aqui criava o segundo sítio que
            // aquele ficheiro recusa criar, e as duas cópias divergiriam no dia
            // em que uma delas aprendesse `<picture>`.
            //
            // A largura disponível é INFINITA porque um replaced não tem
            // min-content diferente do max-content: ele não quebra. Passar a
            // largura da viewport devolveria o que coubesse nela, que é outra
            // pergunta.
            if let Some((w, _)) =
                crate::inline_box::replaced_inline_size(dom, id, &css, f32::INFINITY, ctx)
            {
                return w + frame;
            }
            // Um `width` explícito é um PISO e não um teto, e a diferença é o
            // defeito inteiro. Isto respondia a largura declarada e **voltava
            // atrás sem visitar os filhos** — uma célula com `width:1px`
            // respondia 1px por muito que lá dentro estivesse, e quem a chamava
            // fazia `declarada.max(piso)` contra um piso que era a própria
            // declaração. O mínimo do conteúdo nunca entrava na conta.
            //
            // Guarda-se a declaração e continua-se a medir; o máximo dos dois é a
            // resposta, mais abaixo. É também o que o browser faz: uma largura
            // que não cabe é ignorada, não respeitada com o conteúdo a
            // transbordar.
            let declarada = largura_absoluta(css.width, &resolve).map(|w| {
                w + if css.border_box.unwrap_or(false) {
                    0.0
                } else {
                    frame
                }
            });
            // O `white-space` é herdado, por isso lê-se o do próprio nó em vez de
            // se propagar o do pai pela recursão: um descendente que volte a
            // `normal` PODE quebrar, e arrastar a bandeira para baixo negar-lhe-ia
            // isso.
            let sem_quebra = matches!(
                css.white_space,
                Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre)
            );
            // Sem quebra, os filhos de nível inline ficam todos na MESMA linha: o
            // mínimo é a soma deles, e não o maior. Os de bloco continuam a
            // empilhar-se, logo entram pelo máximo.
            let mut m = 0.0f32;
            let mut linha = 0.0f32;
            for &c in &dom.node(id).children {
                if crate::layout::is_out_of_flow(dom, c) {
                    continue;
                }
                let w = min_content(dom, c, f, ctx, sem_quebra, mono, floor_width);
                if sem_quebra && em_linha(dom, c) {
                    linha += w;
                } else {
                    m = m.max(w);
                }
            }
            if floor_width {
                (m.max(linha) + frame).max(declarada.unwrap_or(0.0))
            } else {
                m.max(linha) + frame
            }
        }
        _ => 0.0,
    }
}

/// `true` se este nó partilha linha com os irmãos — texto, ou um elemento cujo
/// `display` é de nível inline. Um elemento sem `display` nenhum (um `<a>`, um
/// `<span>`) também flui na linha, que é o default do browser para as tags que
/// não estão registadas como bloco.
fn em_linha(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Text(_) => true,
        NodeKind::Element { .. } => match dom
            .computed_style_idx(id)
            .and_then(|c| c.effective_display())
        {
            Some(d) => d.is_inline_level(),
            None => true,
        },
        _ => false,
    }
}

/// Junta as células numa lista de colunas: cada coluna fica com o maior mínimo e
/// o maior máximo das células que a ocupam SOZINHAS (colspan 1).
///
/// As células que atravessam colunas entram depois, e só para LEVANTAR o que já
/// existe — nunca para o baixar. É a regra que evita o efeito mais visível de um
/// algoritmo ingénuo: um cabeçalho com `colspan=3` a ditar a largura de três
/// colunas que o corpo da tabela já tinha dimensionado pelos seus dados.
pub(crate) fn colunas_min_max(
    cells: &[(usize, usize, Coluna)], // (coluna inicial, colspan, restrição da célula)
    cols: usize,
    spacing: f32,
) -> Vec<Coluna> {
    let mut out = vec![Coluna::default(); cols];
    for &(col, _span, mm) in cells.iter().filter(|c| c.1 <= 1) {
        if col < cols {
            out[col].absorve(mm);
        }
    }
    // Segunda passada: as que atravessam. O espaço que a célula precisa é o dela
    // MENOS o `border-spacing` que já existe entre as colunas atravessadas — esse
    // espaço é dela também.
    //
    // A CLASSE de uma célula que atravessa não passa para as colunas, e é de
    // propósito: no Blink a percentagem de uma célula com `colspan` é repartida
    // pelas colunas não-percentuais em proporção ao máximo delas, o que é uma
    // regra à parte e não uma absorção. Marcar as colunas como restringidas aqui
    // seria mais barato e daria a classe errada a todas.
    for &(col, span, mm) in cells.iter().filter(|c| c.1 > 1) {
        let fim = (col + span).min(cols);
        if col >= cols {
            continue;
        }
        let n = fim - col;
        let vaos = (n.saturating_sub(1)) as f32 * spacing;
        let atual_min: f32 = out[col..fim].iter().map(|c| c.min).sum::<f32>() + vaos;
        let atual_max: f32 = out[col..fim].iter().map(|c| c.max).sum::<f32>() + vaos;
        distribui_excedente(&mut out[col..fim], mm.min - atual_min, |c| &mut c.min);
        distribui_excedente(&mut out[col..fim], mm.max - atual_max, |c| &mut c.max);
        // O máximo nunca fica abaixo do mínimo depois de levantado.
        for c in &mut out[col..fim] {
            c.max = c.max.max(c.min);
        }
    }
    out
}

/// Espalha `extra` (se positivo) igualmente pelas colunas. Igualmente e não em
/// proporção porque, no ponto em que isto corre, a proporção seria contra
/// larguras que podem ser todas zero (uma linha inteira de células vazias sob um
/// cabeçalho com colspan), e uma proporção contra zero não distribui nada.
fn distribui_excedente(cols: &mut [Coluna], extra: f32, campo: impl Fn(&mut Coluna) -> &mut f32) {
    if extra <= 0.0 || cols.is_empty() {
        return;
    }
    // Uma célula com colspan não pode elevar uma coluna que já tem uma largura
    // declarada: essa coluna já foi classificada como restringida pelo conteúdo
    // da própria coluna e deve ficar congelada neste degrau. O excedente pertence
    // às colunas automáticas do intervalo. Quando todas são declaradas, não há
    // uma classe livre; nesse caso mantemos o fallback igualitário para preservar
    // o comportamento de uma tabela composta só por restrições.
    let livres: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (!c.restringida).then_some(i))
        .collect();
    let alvos: Vec<usize> = if livres.is_empty() {
        (0..cols.len()).collect()
    } else {
        livres
    };
    let quota = extra / alvos.len() as f32;
    for i in alvos {
        *campo(&mut cols[i]) += quota;
    }
}

