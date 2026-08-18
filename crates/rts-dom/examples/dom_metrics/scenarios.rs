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
