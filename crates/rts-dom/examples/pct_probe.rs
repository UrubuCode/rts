//! Mede ISOLADAMENTE o efeito da regra "uma percentagem não conta para o
//! tamanho intrínseco" sobre a página real.
//!
//! Duas corridas na mesma árvore, com a regra ligada e desligada pelo
//! `RTS_PCT_INTRINSECO`, e a comparação de duas grandezas do eixo VERTICAL — a
//! altura do documento e o `y` do último texto pintado. É esse eixo que
//! regrediu na volta em que a regra entrou, e é ele que decide se a regressão é
//! desta mudança ou de outra que veio na mesma janela.
//!
//!   cargo run -q -p rts-dom --example pct_probe -- scripts/parity/pagina.combinada.html
//!   RTS_PCT_INTRINSECO=1 cargo run -q -p rts-dom --example pct_probe -- <o mesmo>

use rts_dom::layout::{self, DisplayItem};

/// O mesmo medidor aproximado das outras sondas. Não é o do backend, e não
/// precisa de ser: as duas corridas usam o MESMO, portanto a diferença entre
/// elas é da regra e não da fonte.
struct Medidor;

impl layout::TextMeasurer for Medidor {
    fn text_width(&self, text: &str, size: f32, _mono: bool, _bold: bool) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }
    /// A constante da altura de linha é REGULÁVEL (`RTS_LH`) por uma razão de
    /// método: ela é do INSTRUMENTO e não do motor, e comparar a nossa altura
    /// com a do Chrome sem a poder variar é comparar o motor mais um número
    /// escolhido aqui. Variá-la separa "o motor produz altura a mais" de "esta
    /// sonda mede linhas altas de mais".
    fn line_height(&self, size: f32) -> f32 {
        static F: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
        size * *F.get_or_init(|| {
            std::env::var("RTS_LH").ok().and_then(|v| v.parse().ok()).unwrap_or(1.3)
        })
    }
}

fn main() {
    let caminho = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "scripts/parity/pagina.combinada.html".to_string());
    let fonte = std::fs::read_to_string(&caminho).expect("ler a página");
    let dom = rts_dom::parse_html_to_dom(&fonte);
    let ctx = layout::LayoutCtx { viewport_w: 1280.0, viewport_h: 800.0, measurer: &Medidor };
    let lista = layout::layout_document(&dom, &ctx);

    let mut ultimo_texto_y = 0.0f32;
    let mut textos = 0usize;
    let mut maior_y = 0.0f32;
    lista.walk(|item, _dx, dy| {
        if let DisplayItem::Text { y, .. } = item {
            textos += 1;
            ultimo_texto_y = *y + dy;
            maior_y = maior_y.max(*y + dy);
        }
    });

    let regra = if std::env::var("RTS_PCT_INTRINSECO").as_deref() == Ok("1") {
        "DESLIGADA (percentagem conta, como antes)"
    } else {
        "LIGADA (percentagem não conta)"
    };
    println!("regra .................. {regra}");
    println!("altura do documento .... {:.1}px", lista.content_height);
    println!("y do último texto ...... {ultimo_texto_y:.1}px");
    println!("y máximo de um texto ... {maior_y:.1}px");
    println!("textos pintados ........ {textos}");

    // E o dump por elemento, para se poder perguntar a coisa que os agregados
    // não respondem: as LARGURAS aproximaram-se do Chrome enquanto as alturas se
    // afastaram? Um número de altura sozinho não distingue "piorou" de
    // "destapou".
    if let Some(destino) = std::env::args().nth(2) {
        let geo = lista.geometry();
        let mut out = String::new();
        dump_caminhos(&dom, &geo, dom.root, "html[1]".to_string(), &mut out);
        std::fs::write(&destino, out).expect("escrever o dump");
        println!("dump escrito em ........ {destino}");
    }
}

/// Percorre a árvore emitindo `caminho	rect`, com a MESMA convenção de caminho
/// do extrator de paridade (índice a contar irmãos da mesma tag) — sem essa
/// igualdade os ficheiros não casam elemento a elemento e a comparação mede a
/// diferença dos percursos em vez da do layout.
fn dump_caminhos(
    dom: &rts_dom::Dom,
    geo: &rts_dom::layout::Geometry,
    idx: rts_dom::NodeIdx,
    caminho: String,
    out: &mut String,
) {
    use rts_dom::NodeKind;
    if let Some(r) = geo.rects.get(&idx) {
        out.push_str(&format!("{caminho}	{:.2}	{:.2}	{:.2}	{:.2}
", r.x, r.y, r.w, r.h));
    }
    let mut contas: std::collections::BTreeMap<String, usize> = Default::default();
    for &f in &dom.node(idx).children {
        let NodeKind::Element { tag } = &dom.node(f).kind else { continue };
        let n = contas.entry(tag.clone()).or_insert(0);
        *n += 1;
        dump_caminhos(dom, geo, f, format!("{caminho}/{tag}[{n}]"), out);
    }
}
