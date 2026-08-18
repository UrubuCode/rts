//! Os CENÁRIOS — cada um isola um caminho que um cronômetro sozinho mistura.
//!
//! A regra de um cenário aqui: ele responde a uma pergunta que os outros não
//! respondem. `layout frio` e `relayout parado` medem a mesma função e existem
//! separados porque a diferença entre os dois É a resposta (o que os caches
//! cobrem). Um cenário que só repete outro com números maiores não entra.

use crate::report::Run;
use rts_dom::layout::{ApproxMeasurer, LayoutCtx, TextMeasurer};
use rts_dom::metrics;
use rts_dom::{parse_html_to_dom, Dom, NodeId, NodeKind};
use std::time::Instant;

/// Medidor que CONTA as medições além de responder. `text_width` é o único
/// trabalho do layout que sai do crate (o backend real mede pela fonte), então
/// contá-lo separa "o layout está lento" de "o layout pede muita medição".
pub struct CountingMeasurer {
    inner: ApproxMeasurer,
    widths: std::cell::Cell<u64>,
    lines: std::cell::Cell<u64>,
}

impl CountingMeasurer {
    pub fn new() -> Self {
        Self { inner: ApproxMeasurer, widths: 0.into(), lines: 0.into() }
    }
    fn take(&self) -> (u64, u64) {
        let v = (self.widths.get(), self.lines.get());
        self.widths.set(0);
        self.lines.set(0);
        v
    }
}

impl TextMeasurer for CountingMeasurer {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool) -> f32 {
        self.widths.set(self.widths.get() + 1);
        self.inner.text_width(text, size, mono, bold)
    }
    fn line_height(&self, size: f32) -> f32 {
        self.lines.set(self.lines.get() + 1);
        self.inner.line_height(size)
    }
}

pub fn ctx<'a>(m: &'a CountingMeasurer, w: f32, h: f32) -> LayoutCtx<'a> {
    LayoutCtx { viewport_w: w, viewport_h: h, measurer: m }
}

/// Roda `f` `iters` vezes, guardando o tempo de CADA iteração (p95 e máximo só
/// existem porque as amostras são guardadas; uma média acumulada não os teria).
/// `audit_of` devolve a árvore final quando o cenário deixa uma — é o que
/// permite dizer, junto com o tempo, se ele terminou com a árvore consistente.
pub fn run<F: FnMut()>(
    name: &'static str,
    unit: &'static str,
    iters: u32,
    m: &CountingMeasurer,
    mut f: F,
) -> Run {
    m.take();
    let before = metrics::counters::snapshot();
    // Fases e amostras são ZERADAS por cenário em vez de subtraídas: `min`/`max`
    // de uma fase não são subtraíveis (o pico do cenário anterior sobreviveria ao
    // delta e apareceria como pico deste), e uma amostra repetida não voltaria a
    // ser coletada depois que o teto do cenário anterior fechou a lista.
    metrics::phases::reset();
    metrics::samples::reset();
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        f();
        samples.push(t0.elapsed());
    }
    Run {
        name,
        unit,
        samples,
        counters: metrics::counters::snapshot().delta(&before),
        phases: metrics::phase_snapshot(),
        notes: metrics::samples::snapshot(),
        text_measures: m.take(),
        audit: None,
        footprint: None,
    }
}

/// O elemento mais PROFUNDO — a folha cuja mutação tem o menor alcance
/// possível. Se até ela invalidar a página inteira, a invalidação é global de
/// fato, e não por subárvore como a API sugere.
pub fn deepest_element(dom: &Dom) -> Option<NodeId> {
    let mut best: Option<(usize, usize)> = None;
    let mut stack = vec![(0usize, 0usize)];
    while let Some((idx, depth)) = stack.pop() {
        if matches!(dom.node(idx).kind, NodeKind::Element { .. })
            && best.map(|(_, d)| depth > d).unwrap_or(true)
        {
            best = Some((idx, depth));
        }
        stack.extend(dom.node(idx).children.iter().map(|&c| (c, depth + 1)));
    }
    best.map(|(idx, _)| dom.id_of_idx(idx))
}

/// O elemento com MAIS filhos — o container que um `innerHTML` ou uma remoção
/// em massa realmente atinge numa página real.
fn biggest_container(dom: &Dom) -> Option<NodeId> {
    let mut best: Option<(usize, usize)> = None;
    for idx in 0..dom.nodes.len() {
        if matches!(dom.node(idx).kind, NodeKind::Element { .. })
            && best.map(|(_, n)| dom.node(idx).children.len() > n).unwrap_or(true)
        {
            best = Some((idx, dom.node(idx).children.len()));
        }
    }
    best.map(|(idx, _)| dom.id_of_idx(idx))
}

/// Todos os cenários de uma página. `iters` é por cenário.
pub fn page(html: &str, vw: f32, vh: f32, iters: u32, m: &CountingMeasurer) -> Vec<Run> {
    let mut runs = Vec::new();

    // 1. PARSE puro — tokenizer + árvore + índices + o CSS dos `<style>`.
    runs.push(run("parse", "parse", iters, m, || {
        std::hint::black_box(parse_html_to_dom(html));
    }));

    // 2. LAYOUT FRIO — árvore recém-parseada, todo cache vazio: o que a página
    //    paga ao abrir, e o único cenário em que a cascade roda de verdade.
    runs.push(run("layout frio", "página", iters, m, || {
        let d = parse_html_to_dom(html);
        std::hint::black_box(rts_dom::layout::layout_document(&d, &ctx(m, vw, vh)));
    }));

    // 3. RELAYOUT SEM MUTAÇÃO — o teto dos caches: nada mudou, então o que
    //    sobra é trabalho que nenhum memo cobre (o custo de um frame parado).
    let dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh));
    runs.push(run("relayout parado", "frame", iters, m, || {
        std::hint::black_box(rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh)));
    }));
    drop(dom);

    // 3b. FRAME PARADO COM CACHE — o caminho que um app real percorre: nada
    //     mudou, então o layout inteiro é reusado. A diferença para o cenário
    //     acima é o preço de NÃO ter o cache, e é ele que diz quanto um
    //     `getBoundingClientRect` custava no caminho headless.
    let dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh));
    runs.push(run("frame parado (cache)", "frame", iters, m, || {
        std::hint::black_box(rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh)));
    }));
    drop(dom);

    // 3c. MUTAÇÃO + frame com cache — o cache é invalidado por revisão, então
    //     aqui ele NÃO ajuda: é o custo real de um frame que muda algo.
    let mut dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh));
    let leaf0 = deepest_element(&dom);
    let mut t0 = 0u32;
    runs.push(run("mutação + frame (cache)", "frame", iters, m, || {
        t0 += 1;
        if let Some(t) = leaf0 {
            dom.set_text(t, &format!("t {t0}"));
        }
        std::hint::black_box(rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh)));
    }));
    drop(dom);

    // 3d. CLASSE INERTE + frame com cache — `classList.toggle('x')` com uma
    //     classe que nenhuma regra cita. Um browser resolve isto em microssegundos
    //     porque não invalida nada; é o cenário que mede se nós também.
    let mut dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh));
    let leaf1 = deepest_element(&dom);
    let mut flip0 = false;
    runs.push(run("classe inerte + frame", "frame", iters, m, || {
        flip0 = !flip0;
        if let Some(t) = leaf1 {
            dom.set_attr(t, "class", if flip0 { "rts-classe-inexistente" } else { "" });
        }
        std::hint::black_box(rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh)));
    }));
    drop(dom);

    // 3f. CLASSE QUE SÓ MUDA COR + frame — o toggle mais comum de um app
    //     (`.active`, `.selected`, `:hover` traduzido para classe): muda o que
    //     se PINTA sem mudar o que se MEDE. É o caso que a costura de container
    //     existe para servir, e o que um browser resolve em microssegundos.
    let mut dom = parse_html_to_dom(html);
    dom.add_stylesheet(".rts-realce{color:#ff0000}");
    let _ = rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh));
    let leaf3 = deepest_element(&dom);
    let mut flip3 = false;
    runs.push(run("classe de cor + frame", "frame", iters, m, || {
        flip3 = !flip3;
        if let Some(t) = leaf3 {
            dom.set_attr(t, "class", if flip3 { "rts-realce" } else { "" });
        }
        std::hint::black_box(rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh)));
    }));
    drop(dom);

    // 3e. CLASSE QUE CASA + frame com cache — o `classList.toggle` que de fato
    //     muda estilo, pelo caminho que um app percorre (o layout é PEDIDO, não
    //     forçado). É o número comparável ao do browser; o cenário `classe +
    //     relayout` abaixo força o layout completo e mede outra coisa.
    let mut dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh));
    let leaf2 = deepest_element(&dom);
    // uma classe que o stylesheet da página realmente cita
    let citada = (0..dom.nodes.len())
        .filter_map(|i| dom.node(i).attr("class").map(str::to_string))
        .flat_map(|c| c.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .find(|c| dom.stylesheet().mentions_class(c))
        .unwrap_or_default();
    let mut flip2 = false;
    runs.push(run("classe que casa + frame", "frame", iters, m, || {
        flip2 = !flip2;
        if let Some(t) = leaf2 {
            dom.set_attr(t, "class", if flip2 { &citada } else { "" });
        }
        std::hint::black_box(rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh)));
    }));
    drop(dom);

    // 4. TEXTO de uma folha + relayout — o contador, o relógio, o campo que
    //    digita. Mede o que a invalidação PRESERVA.
    let mut dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh));
    let leaf = deepest_element(&dom);
    let mut tick = 0u32;
    // A pegada ANTES do cenário: o Δ contra a do fim é o que mostra o estado
    // derivado crescendo sem a árvore crescer.
    let antes = metrics::footprint(&dom);
    let mut r = run("texto + relayout", "frame", iters, m, || {
        tick += 1;
        if let Some(t) = leaf {
            dom.set_text(t, &format!("tick {tick}"));
        }
        std::hint::black_box(rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh)));
    });
    r.audit = Some(metrics::audit(&dom));
    r.footprint = Some((antes, metrics::footprint(&dom)));
    runs.push(r);
    drop(dom);

    // 5. CLASSE de uma folha + relayout — o caminho de todo estado visual
    //    (`.active`, `.open`): muda o ESTILO, não só o conteúdo, então re-roda
    //    a cascade que o cenário 4 preserva. A diferença entre os dois é o
    //    preço de uma classe.
    let mut dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh));
    let leaf = deepest_element(&dom);
    let mut flip = false;
    // A pegada ANTES do cenário: o Δ contra a do fim é o que mostra o estado
    // derivado crescendo sem a árvore crescer.
    let antes = metrics::footprint(&dom);
    let mut r = run("classe + relayout", "frame", iters, m, || {
        flip = !flip;
        if let Some(t) = leaf {
            dom.set_attr(t, "class", if flip { "active" } else { "" });
        }
        std::hint::black_box(rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh)));
    });
    r.audit = Some(metrics::audit(&dom));
    r.footprint = Some((antes, metrics::footprint(&dom)));
    runs.push(r);
    drop(dom);

    // 6. HOVER — o backend informa o nó sob o cursor a cada frame. Deve custar
    //    ZERO numa página sem regra `:hover` (há uma guarda para isso); este
    //    cenário é o que prova que a guarda continua valendo.
    let mut dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh));
    let mut n = 0usize;
    runs.push(run("hover + relayout", "frame", iters, m, || {
        n = (n + 7) % dom.nodes.len().max(1);
        dom.set_hovered(Some(n));
        std::hint::black_box(rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh)));
    }));
    drop(dom);

    // 7-8. CONSULTAS: a genérica (lista de seletores) e a por `#id`, que tem
    //    índice. As duas juntas dizem se o índice está sendo usado.
    let dom = parse_html_to_dom(html);
    runs.push(run("querySelectorAll", "consulta", iters, m, || {
        std::hint::black_box(dom.query_all(".btn, div, a[href]"));
    }));
    // A consulta por CLASSE é a que um app faz o tempo todo, e a única forma
    // que os índices podem servir — a de cima tem `div` (tag) e cai na varredura.
    let classe = (0..dom.nodes.len())
        .find_map(|i| {
            dom.node(i).attr("class").and_then(|c| c.split_whitespace().next().map(str::to_string))
        })
        .unwrap_or_else(|| "nao-existe".into());
    let sel_classe = format!(".{classe}");
    runs.push(run("querySelectorAll .classe", "consulta", iters, m, || {
        std::hint::black_box(dom.query_all(&sel_classe));
    }));
    let first_id = (0..dom.nodes.len())
        .find_map(|i| dom.node(i).attr("id").map(str::to_string))
        .unwrap_or_else(|| "nao-existe".into());
    let sel = format!("#{first_id}");
    runs.push(run("querySelector #id", "consulta", iters, m, || {
        std::hint::black_box(dom.query(&sel));
    }));
    drop(dom);

    // 9. EVENTO com bubbling — o caminho de todo clique: sobe a árvore
    //    coletando callbacks. Custa em profundidade, não em tamanho de página.
    let mut dom = parse_html_to_dom(html);
    let leaf = deepest_element(&dom);
    if let Some(t) = leaf {
        dom.add_event_listener_cb(t, "click", 1);
    }
    runs.push(run("clique + bubbling", "evento", iters, m, || {
        if let Some(t) = leaf {
            std::hint::black_box(dom.dispatch_event_collect(t, "click", true));
        }
    }));
    drop(dom);

    // 10. innerHTML num container real — re-parse de subárvore, o padrão de
    //     toda renderização de lista feita por script.
    let mut dom = parse_html_to_dom(html);
    let _ = rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh));
    let host = biggest_container(&dom);
    let mut i = 0u32;
    // A pegada ANTES do cenário: o Δ contra a do fim é o que mostra o estado
    // derivado crescendo sem a árvore crescer.
    let antes = metrics::footprint(&dom);
    let mut r = run("innerHTML + relayout", "frame", iters, m, || {
        i += 1;
        if let Some(h) = host {
            dom.set_inner_html(h, &format!("<p class=\"row\">linha {i}</p><p>outra</p>"));
        }
        std::hint::black_box(rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh)));
    });
    r.audit = Some(metrics::audit(&dom));
    r.footprint = Some((antes, metrics::footprint(&dom)));
    runs.push(r);

    runs
}

/// POR QUE a página desenha o que desenha: quais elementos receberam geometria,
/// quais não, e o que os que não receberam têm em comum.
///
/// Uma página que abre em branco não diz onde falhou — o layout não reclama,
/// ele só não emite. Esta visão é o que transforma "ficou branco" numa lista de
/// causas: `display:none`, fora do fluxo, tamanho zero, ou tag que o motor pula.
pub fn explicar_pagina(html: &str, vw: f32, vh: f32, m: &CountingMeasurer) {
    use std::collections::BTreeMap;
    let dom = parse_html_to_dom(html);
    let lista = rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh));
    let geo = lista.geometry();

    let mut com_caixa = 0usize;
    let mut sem_caixa = 0usize;
    let mut zerados = 0usize;
    let mut por_motivo: BTreeMap<String, usize> = BTreeMap::new();
    let mut exemplos: Vec<String> = Vec::new();

    for idx in 0..dom.nodes.len() {
        let rts_dom::NodeKind::Element { tag } = &dom.node(idx).kind else { continue };
        let css = dom.computed_style_idx(idx);
        let display = css.as_ref().and_then(|c| c.effective_display());
        match geo.rects.get(&idx) {
            Some(r) if r.w > 0.5 && r.h > 0.5 => com_caixa += 1,
            Some(_) => {
                zerados += 1;
                *por_motivo.entry(format!("caixa 0×0 ({tag}, display {display:?})")).or_default() += 1;
            }
            None => {
                sem_caixa += 1;
                let motivo = match display {
                    Some(rts_dom::style::DisplayKind::None) => "display:none".to_string(),
                    _ => {
                        let pos = css.as_ref().and_then(|c| c.position);
                        match pos {
                            Some(p) if p.out_of_flow() => format!("fora do fluxo ({p:?})"),
                            _ => format!("sem geometria ({tag}, display {display:?})"),
                        }
                    }
                };
                if exemplos.len() < 8 && motivo.starts_with("sem geometria") {
                    exemplos.push(format!("{tag}#{idx} — {motivo}"));
                }
                *por_motivo.entry(motivo).or_default() += 1;
            }
        }
    }

    println!("    por que a página desenha o que desenha");
    println!("      elementos com caixa visível        {com_caixa}");
    println!("      elementos com caixa 0×0            {zerados}");
    println!("      elementos SEM geometria            {sem_caixa}");
    let mut motivos: Vec<(&String, &usize)> = por_motivo.iter().collect();
    motivos.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (motivo, n) in motivos.into_iter().take(8) {
        println!("        {n:>5}× {motivo}");
    }
    for e in exemplos {
        println!("        · {e}");
    }

    // O que de fato vai para a tela: uma página pode ter caixas e ainda assim
    // abrir em branco se tudo o que ela emite for da cor do fundo.
    use rts_dom::layout::DisplayItem as D;
    let mut retangulos = 0usize;
    let mut bordas = 0usize;
    let mut textos: Vec<String> = Vec::new();
    let mut cores: std::collections::BTreeMap<u32, usize> = Default::default();
    lista.walk(|item, _, _| match item {
        D::SolidRect { color, rect, .. } => {
            retangulos += 1;
            if rect.w > 1.0 && rect.h > 1.0 {
                *cores.entry(*color).or_default() += 1;
            }
        }
        D::Border { .. } => bordas += 1,
        D::Text { text, x, y, .. } => {
            if textos.len() < 10 && !text.trim().is_empty() {
                textos.push(format!("({x:.0},{y:.0}) {:?}", text.chars().take(40).collect::<String>()));
            }
        }
        _ => {}
    });
    println!("      itens pintados: {retangulos} retângulos, {bordas} bordas, {} textos", textos.len());
    // ORDEM de pintura: um fundo que venha DEPOIS do texto apaga o texto.
    println!("      ordem (primeiros 22):");
    let mut n = 0usize;
    lista.walk(|item, dx, dy| {
        if n >= 22 {
            return;
        }
        n += 1;
        let descricao = match item {
            D::SolidRect { rect, color, .. } => format!(
                "rect ({:.0},{:.0}) {:.0}x{:.0} #{color:08X}",
                rect.x + dx, rect.y + dy, rect.w, rect.h
            ),
            D::Border { rect, color, .. } => {
                format!("borda ({:.0},{:.0}) #{color:08X}", rect.x + dx, rect.y + dy)
            }
            D::Text { x, y, text, color, .. } => format!(
                "texto ({:.0},{:.0}) #{color:08X} {:?}",
                x + dx,
                y + dy,
                text.chars().take(24).collect::<String>()
            ),
            D::BeginClip { rect, .. } => format!("clip ({:.0},{:.0}) {:.0}x{:.0}", rect.x + dx, rect.y + dy, rect.w, rect.h),
            D::EndClip => "fim-clip".to_string(),
            outro => format!("{outro:?}").chars().take(40).collect(),
        };
        println!("        {n:>3}. {descricao}");
    });
    let mut cores: Vec<(u32, usize)> = cores.into_iter().collect();
    cores.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (cor, n) in cores.into_iter().take(5) {
        println!("        {n:>4}× cor #{cor:08X}");
    }
    for t in textos {
        println!("        texto {t}");
    }
}

/// Confere, numa página REAL, que o layout com reuso de fragmentos é o mesmo
/// que o layout calculado do zero — depois de uma sequência de mutações que
/// exercita os caminhos de invalidação.
///
/// Devolve quantos itens foram conferidos, ou a primeira divergência. A
/// tolerância é a mesma do teste unitário e pela mesma razão: reusar deslocando
/// é somar, e somar não dá bit a bit o mesmo que calcular do zero.
pub fn verificar_equivalencia(
    html: &str,
    vw: f32,
    vh: f32,
    m: &CountingMeasurer,
) -> Result<usize, String> {
    const TOL: f32 = 0.05;
    let mut dom = parse_html_to_dom(html);
    let alvo = deepest_element(&dom);
    let mut conferidos = 0usize;
    for passo in 0..4 {
        match passo {
            1 => {
                if let Some(t) = alvo {
                    dom.set_text(t, "texto trocado pelo verificador");
                }
            }
            2 => {
                if let Some(t) = alvo {
                    dom.set_attr(t, "class", "verificador-classe");
                }
            }
            3 => {
                if let Some(t) = alvo {
                    dom.remove_node(t);
                }
            }
            _ => {}
        }
        // PLANAS: com a saída em ÁRVORE, os itens de uma subárvore reusada não
        // estão no buffer próprio — comparar `items` compararia o que sobrou.
        let reusado = rts_dom::layout::layout_cached(&dom, &ctx(m, vw, vh)).materialized();
        dom.clear_fragment_cache();
        let zero = rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh)).materialized();
        if reusado.len() != zero.len() {
            return Err(format!(
                "passo {passo}: {} itens com reuso, {} sem",
                reusado.len(),
                zero.len()
            ));
        }
        for (i, (a, b)) in reusado.iter().zip(&zero).enumerate() {
            if !item_equivalente(a, b, TOL) {
                return Err(format!("passo {passo}, item {i}:
      reuso: {a:?}
      zero:  {b:?}"));
            }
            conferidos += 1;
        }
    }
    Ok(conferidos)
}

/// Igualdade de item com tolerância só na GEOMETRIA — texto, cor e tipo têm de
/// bater exatamente.
///
/// TODAS as variantes com retângulo entram: a primeira versão só tratava texto,
/// fundo e borda, e o resto caía em igualdade ESTRITA — o que acusou uma
/// divergência falsa num `BeginClip` de página real (17.599998 contra
/// 17.599976, dois centésimos de milésimo de pixel). Um verificador que dá
/// alarme falso é pior que nenhum: ensina a ignorá-lo.
fn item_equivalente(a: &rts_dom::layout::DisplayItem, b: &rts_dom::layout::DisplayItem, tol: f32) -> bool {
    use rts_dom::layout::DisplayItem as D;
    let perto = |x: f32, y: f32| (x - y).abs() < tol;
    let rects = |ra: &rts_dom::layout::Rect, rb: &rts_dom::layout::Rect| {
        perto(ra.x, rb.x) && perto(ra.y, rb.y) && perto(ra.w, rb.w) && perto(ra.h, rb.h)
    };
    match (a, b) {
        (D::Text { x: xa, y: ya, text: ta, color: ca, .. }, D::Text { x: xb, y: yb, text: tb, color: cb, .. }) => {
            perto(*xa, *xb) && perto(*ya, *yb) && ta == tb && ca == cb
        }
        (D::SolidRect { rect: ra, color: ca, .. }, D::SolidRect { rect: rb, color: cb, .. })
        | (D::Border { rect: ra, color: ca, .. }, D::Border { rect: rb, color: cb, .. })
        | (D::Shadow { rect: ra, color: ca, .. }, D::Shadow { rect: rb, color: cb, .. }) => {
            rects(ra, rb) && ca == cb
        }
        (D::GradientRect { rect: ra, c0: a0, c1: a1, .. }, D::GradientRect { rect: rb, c0: b0, c1: b1, .. }) => {
            rects(ra, rb) && a0 == b0 && a1 == b1
        }
        (D::Image { rect: ra, pixels_handle: ha, .. }, D::Image { rect: rb, pixels_handle: hb, .. }) => {
            rects(ra, rb) && ha == hb
        }
        (D::BeginClip { rect: ra, node: na, .. }, D::BeginClip { rect: rb, node: nb, .. }) => {
            rects(ra, rb) && na == nb
        }
        (D::EndClip, D::EndClip) => true,
        _ => false,
    }
}

/// CONSTRUÇÃO programática — `createElement` + `appendChild` × N, o caminho de
/// qualquer script que monta uma lista. Independe de arquivo: é sobre a
/// estrutura, não sobre uma página.
pub fn build(n: u32, vw: f32, vh: f32, m: &CountingMeasurer) -> Vec<Run> {
    let mut runs = Vec::new();
    let host_html = "<html><body><ul id=\"host\"></ul></body></html>";

    let mut dom = parse_html_to_dom(host_html);
    let host = dom.query("#host").expect("host");
    // A pegada ANTES do cenário: o Δ contra a do fim é o que mostra o estado
    // derivado crescendo sem a árvore crescer.
    let antes = metrics::footprint(&dom);
    let mut r = run("append × N", "árvore", 1, m, || {
        for i in 0..n {
            let li = dom.create_element("li");
            dom.set_attr(li, "class", "item");
            let txt = dom.create_text_node(&format!("item {i}"));
            dom.append_child(li, txt);
            dom.append_child(host, li);
        }
    });
    r.audit = Some(metrics::audit(&dom));
    r.footprint = Some((antes, metrics::footprint(&dom)));
    runs.push(r);

    // O MESMO trabalho, lendo o layout a cada 100 inserções — o padrão real de
    // um script que mede enquanto constrói. Separa o custo da CONSTRUÇÃO do
    // custo da INVALIDAÇÃO que ela dispara: se a segunda linha não for
    // proporcional à primeira, a invalidação é quadrática.
    let mut dom = parse_html_to_dom(host_html);
    let host = dom.query("#host").expect("host");
    // A pegada ANTES do cenário: o Δ contra a do fim é o que mostra o estado
    // derivado crescendo sem a árvore crescer.
    let antes = metrics::footprint(&dom);
    let mut r = run("append + layout /100", "árvore", 1, m, || {
        for i in 0..n {
            let li = dom.create_element("li");
            dom.set_attr(li, "class", "item");
            let txt = dom.create_text_node(&format!("item {i}"));
            dom.append_child(li, txt);
            dom.append_child(host, li);
            if i % 100 == 99 {
                std::hint::black_box(rts_dom::layout::layout_document(&dom, &ctx(m, vw, vh)));
            }
        }
    });
    r.audit = Some(metrics::audit(&dom));
    r.footprint = Some((antes, metrics::footprint(&dom)));
    runs.push(r);

    // REMOÇÃO em massa — o inverso, e o cenário que revela o estado derivado
    // que sobrevive ao nó (índices, listeners): a auditoria ao fim é a métrica,
    // não o tempo.
    let mut dom = parse_html_to_dom(host_html);
    let host = dom.query("#host").expect("host");
    let mut criados: Vec<NodeId> = Vec::new();
    for i in 0..n {
        let li = dom.create_element("li");
        dom.set_attr(li, "id", &format!("i{i}"));
        dom.set_attr(li, "class", "item");
        dom.append_child(host, li);
        criados.push(li);
    }
    let mut it = criados.into_iter();
    // A pegada ANTES do cenário: o Δ contra a do fim é o que mostra o estado
    // derivado crescendo sem a árvore crescer.
    let antes = metrics::footprint(&dom);
    let mut r = run("remove × N", "árvore", 1, m, || {
        for id in it.by_ref() {
            dom.remove_node(id);
        }
    });
    r.audit = Some(metrics::audit(&dom));
    r.footprint = Some((antes, metrics::footprint(&dom)));
    runs.push(r);

    runs
}
