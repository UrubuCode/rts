//! SEGMENTOS de uma linha, e a elipse: acumular o que já cabe, colapsar o
//! espaço em branco, e cortar com `…` quando `text-overflow` o pede.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
/// Um segmento de texto colorido/pesado posicionado numa linha (após o wrap).
/// `atomic: Some((idx, kind))` = uma caixa de `ww × wh` (pintada pela emissão),
/// ou um marcador de largura zero que só existe para receber a sua geometria.
pub(in crate::layout) struct Segment {
    pub(in crate::layout) text: String,
    pub(in crate::layout) text_width: f32,
    pub(in crate::layout) color: u32,
    pub(in crate::layout) bold: bool,
    pub(in crate::layout) italic: bool,
    pub(in crate::layout) deco: u8,
    pub(in crate::layout) owners: Vec<NodeIdx>,
    pub(in crate::layout) atomic: Option<(NodeIdx, AtomicKind)>,
    pub(in crate::layout) ww: f32,
    pub(in crate::layout) wh: f32,
    /// A largura do espaço que precede este segmento e NÃO lhe pertence: o que
    /// veio do run ANTERIOR, em `antes <a>alvo</a>`.
    ///
    /// É um vão antes do segmento, nunca parte do `text`/`text_width`/`ww` — e é
    /// por isso que existe em vez de se somar à largura. O espaço ocupa lugar na
    /// linha (o `<a>` começa depois dele) mas é conteúdo do texto anónimo que vem
    /// antes, portanto a CAIXA do `<a>` não o contém: o Chrome responde `x=48,
    /// w=32` onde somá-lo dava `x=40, w=40`. Quando o segmento é FUNDIDO no
    /// anterior o vão passa a ser interior e vive dentro do `text` — é o mesmo
    /// espaço no mesmo sítio, visto do lado de dentro.
    pub(in crate::layout) lead_w: f32,
}

/// Quebra uma sequência de RUNS coloridos em LINHAS por palavra (word-wrap), juntando
/// runs adjacentes na mesma linha. Cada linha é um vetor de [`Segment`] (pedaços
/// coloridos contíguos). Uma palavra que não cabe começa nova linha. FIEL AOS
/// ESPAÇOS do fonte: um espaço só entra entre duas palavras quando o texto
/// ORIGINAL tinha whitespace ali (colapsado p/ 1) — inclusive ATRAVÉS de runs
/// (`<a>Bootstrap</a>, by` NÃO ganha espaço antes da vírgula; antes toda palavra
/// ganhava espaço e a pontuação descolava).
/// Colapsa o whitespace de um run como o fluxo inline faz: sequências viram um
/// espaço só, o do fim some (o separador seguinte o recria) e o do início só
/// entra quando havia palavra antes na linha. É a normalização que o scanner
/// palavra-a-palavra produz implicitamente — o fast path precisa dela explícita
/// para que os dois caminhos gerem o MESMO texto.
///
/// `leading_space` é a resposta JÁ TOMADA pelo chamador à pergunta "havia
/// whitespace desde a última palavra?", e é a única coisa que decide o espaço da
/// frente. Perguntá-la outra vez aqui — exigindo além disso que o texto DESTE
/// run comece por whitespace — apagava o espaço em toda fronteira de elemento
/// inline: em `antes <a>alvo</a>`, o espaço está no fim do run anterior e o run
/// do `<a>` começa por 'a', portanto nenhum dos dois o emitia. A página saía
/// pintada com `antesalvo`, e cada fronteira encurtava a linha em um espaço, o
/// que mudava o ponto de quebra. O scanner palavra-a-palavra decide por
/// `pending_space && !at_line_start` e só por isso — é a mesma pergunta e passa
/// a ter a mesma resposta.
pub(in crate::layout) fn collapse_ws(text: &str, leading_space: bool) -> std::borrow::Cow<'_, str> {
    // O caso comum é o texto JÁ normalizado (uma palavra, ou palavras separadas
    // por um espaço só, sem borda) — devolver emprestado evita uma alocação por
    // run, e um relayout de página grande são milhares deles.
    let needs_work = leading_space
        || text.starts_with(e_espaco_css)
        || text.ends_with(e_espaco_css)
        || text.contains("  ")
        || text.chars().any(|c| e_espaco_css(c) && c != ' ');
    if !needs_work {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 1);
    if leading_space {
        out.push(' ');
    }
    let mut first = true;
    for word in crate::inline_box::palavras_css(text) {
        if !first {
            out.push(' ');
        }
        out.push_str(word);
        first = false;
    }
    std::borrow::Cow::Owned(out)
}

/// Acrescenta texto à linha, juntando ao último segmento quando o estilo é o
/// mesmo (é o que evita um segmento por palavra na hora de pintar).
/// `lead` é a largura do espaço com que `text` começa quando esse espaço veio do
/// run ANTERIOR (zero se não há espaço ou se ele é deste run). Ao abrir segmento
/// novo o espaço sai do texto e vira vão, para ficar de fora da caixa dos donos;
/// ao FUNDIR fica onde está, porque aí é interior ao segmento que o recebe.
pub(in crate::layout) fn push_segment(cur: &mut Vec<Segment>, run: &InlineRun, text: &str, width: f32, lead: f32) {
    if let Some(last) = cur.last_mut() {
        if last.atomic.is_none()
            && last.color == run.color
            && last.bold == run.bold
            && last.italic == run.italic
            && last.deco == run.deco
            && last.owners == run.owners
        {
            last.text.push_str(text);
            last.text_width += width;
            return;
        }
    }
    // Só sai do texto se sobrar texto: um segmento que fosse SÓ o vão não tem
    // dono a quem servir e ainda perderia o espaço.
    let separa = lead > 0.0 && text.starts_with(' ') && text.len() > 1;
    let (text, width, lead) = match separa {
        true => (&text[1..], width - lead, lead),
        false => (text, width, 0.0),
    };
    cur.push(Segment {
        text: text.to_string(),
        text_width: width,
        color: run.color,
        bold: run.bold,
        italic: run.italic,
        deco: run.deco,
        owners: run.owners.clone(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
        lead_w: lead,
    });
}

/// As TRÊS condições de `text-overflow: ellipsis` — e são três porque com
/// qualquer uma em falta o Chrome não põe reticências nenhumas.
///
/// 1. a propriedade pedida; 2. o transbordo ESCONDIDO (`visible` deixa o texto
/// sair da caixa e não há nada a cortar); 3. a linha a NÃO quebrar — com quebra,
/// o texto desce em vez de transbordar e a elipse nunca chega a ser devida.
///
/// ⚠️ CORTE declarado: o Chrome aplica-a também no eixo do bloco e em conteúdo
/// que transborda por outras razões. Aqui é só a linha única horizontal, que é
/// o que as 29 declarações `ellipsis` do corpus escrevem — todas num container
/// com `overflow:hidden` e `white-space:nowrap`.
pub(in crate::layout) fn elipse_pedida(css: &ComputedStyle, nowrap: bool) -> bool {
    css.text_overflow == Some(crate::style::vocab::TextOverflow::Ellipsis)
        && matches!(
            css.overflow_x,
            Some(crate::scrollbar::Overflow::Hidden | crate::scrollbar::Overflow::Auto)
                | Some(crate::scrollbar::Overflow::Scroll)
        )
        && nowrap
}

/// Corta cada linha que transborda `content_w` e acrescenta-lhe `…`.
///
/// O orçamento é `content_w` MENOS a largura da própria elipse: o browser
/// garante que as reticências ficam DENTRO da caixa, e cortar em `content_w` e
/// depois somar o `…` punha-as de fora — o mesmo transbordo que isto existe
/// para esconder, um carácter mais estreito.
///
/// Uma caixa atómica no ponto de corte é DESCARTADA em vez de encolhida: um
/// `<img>` não tem prefixo, e escalá-lo para caber inventaria uma geometria que
/// o Chrome não produz.
pub(in crate::layout) fn aplicar_elipse(
    lines: Vec<Vec<Segment>>,
    content_w: f32,
    font_size: f32,
    mono: bool,
    m: &dyn TextMeasurer,
) -> Vec<Vec<Segment>> {
    aplicar_elipse_forcada(lines, content_w, font_size, mono, m, false)
}

/// O mesmo corte de [`aplicar_elipse`], mas com um `forcar` que salta a saída
/// antecipada "já cabe, não corta nada" — `-webkit-line-clamp`
/// (`layout::tabulacao::aplicar_line_clamp`) chama a última linha mantida
/// SEMPRE com reticências, mesmo quando essa linha por si só caberia na
/// largura: o que a propriedade limita são LINHAS, não largura, e uma frase
/// curta que sobrou como 3ª de 3 continua a precisar do "…" que diz que havia
/// mais texto por trás.
pub(in crate::layout) fn aplicar_elipse_forcada(
    lines: Vec<Vec<Segment>>,
    content_w: f32,
    font_size: f32,
    mono: bool,
    m: &dyn TextMeasurer,
    forcar: bool,
) -> Vec<Vec<Segment>> {
    const ELIPSE: &str = "…";
    lines
        .into_iter()
        .map(|line| {
            let total: f32 = line
                .iter()
                .map(|s| s.lead_w + if s.atomic.is_some() { s.ww } else { s.text_width })
                .sum();
            if total <= content_w && !forcar {
                return line;
            }
            if total <= content_w && forcar {
                // cabe, mas a elipse é devida na mesma (linha cortada por
                // `line-clamp`, não por transbordo): só acrescenta o "…".
                let w_elipse = m.text_width(ELIPSE, font_size, mono, false, false);
                let mut line = line;
                match line.last_mut() {
                    Some(last) if last.atomic.is_none() => {
                        last.text.push_str(ELIPSE);
                        last.text_width += w_elipse;
                    }
                    _ => line.push(Segment {
                        text: ELIPSE.to_string(),
                        text_width: w_elipse,
                        color: 0,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners: Vec::new(),
                        atomic: None,
                        ww: 0.0,
                        wh: 0.0,
                        lead_w: 0.0,
                    }),
                }
                return line;
            }
            let w_elipse = m.text_width(ELIPSE, font_size, mono, false, false);
            let orcamento = content_w - w_elipse;
            let mut out: Vec<Segment> = Vec::with_capacity(line.len());
            let mut acc = 0.0f32;
            for mut seg in line {
                let largura = if seg.atomic.is_some() {
                    seg.ww
                } else {
                    seg.text_width
                };
                if acc + seg.lead_w + largura <= orcamento {
                    acc += seg.lead_w + largura;
                    out.push(seg);
                    continue;
                }
                if seg.atomic.is_none() {
                    let disp = orcamento - acc - seg.lead_w;
                    let (n, w) = crate::inline_box::prefixo_que_cabe(
                        &seg.text,
                        disp,
                        font_size,
                        mono,
                        seg.bold,
                        seg.italic,
                        m,
                    );
                    seg.text.truncate(n);
                    seg.text.push_str(ELIPSE);
                    seg.text_width = w + w_elipse;
                    out.push(seg);
                    return out;
                }
                // atómica a transbordar: cai fora, e a elipse vai para o texto
                // que ficou — ou abre segmento próprio se a linha começa por ela.
                break;
            }
            match out.last_mut() {
                Some(last) if last.atomic.is_none() => {
                    last.text.push_str(ELIPSE);
                    last.text_width += w_elipse;
                }
                _ => out.push(Segment {
                    text: ELIPSE.to_string(),
                    text_width: w_elipse,
                    color: 0,
                    bold: false,
                    italic: false,
                    deco: 0,
                    owners: Vec::new(),
                    atomic: None,
                    ww: 0.0,
                    wh: 0.0,
                    lead_w: 0.0,
                }),
            }
            out
        })
        .collect()
}
