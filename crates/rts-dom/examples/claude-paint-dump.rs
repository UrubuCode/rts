//! O que é DESENHADO, uma linha JSON por item — a entrada da 4ª régua.
//!
//!   cargo run -q -p rts-dom --example claude-paint-dump -- pagina.html > out/rts-paint.jsonl
//!
//! # Porque existe, quando `page_paint.rs` já anda a mesma lista
//!
//! Porque aquele RESPONDE e este DESPEJA. O `page_paint` foi escrito para uma
//! pergunta de cada vez — quantos clips abrem, qual o primeiro, quantos itens
//! ficam lá dentro — e cada pergunta nova é um bloco novo em Rust. As três
//! réguas de paridade que já existem comparam a CAIXA de cada elemento e
//! nenhuma vê o desenho, e as duas classes de defeito que faltam apanhar
//! (conteúdo em falta, item no sítio errado) precisam da lista inteira do lado
//! de fora, onde um comparador em JS a pode cruzar com o dump do Chrome.
//!
//! Então a decisão que este ficheiro toma é NÃO TOMAR NENHUMA: não classifica
//! um marcador, não decide o que é um órfão, não normaliza texto. Emite o que a
//! `DisplayList` tem e deixa o critério na régua — que é onde ele pode ser
//! sabotado de propósito para se provar que o instrumento vê o que audita. Um
//! critério enterrado aqui seria um critério que a auto-conferência não alcança.
//!
//! # O MEDIDOR é o `ApproxMeasurer`, e isso não é um atalho
//!
//! É o mesmo que `Dom::bounding_components_many` usa (`dom.rs`), ou seja o
//! mesmo que produziu o `rts.jsonl` de geometria com que este dump vai ser
//! cruzado. Um medidor melhor aqui daria um dump mais bonito e não comparável:
//! as duas metades da régua passariam a medir layouts diferentes e a diferença
//! entre elas leria-se como defeito. O `page_paint` usa um medidor próprio
//! (`0.5 × size` por caractere) e é exatamente por isso que o dele não serve.
//!
//! # As linhas
//!
//! `__meta` primeiro, `__fim` com os totais por último — a regra do
//! `docs/ui/parity-chrome.md`: um dump cortado a meio parece um corpus mais
//! pequeno e lê-se como "o motor não desenhou metade da página". Sem o rodapé
//! não há como distinguir os dois casos, e são conclusões opostas.
//!
//! Dois tipos de linha, de propósito, no MESMO ficheiro:
//!
//! - `{"k":"el",…}` — a caixa de cada elemento, com o caminho `html[1]/body[1]/…`
//!   que as outras réguas já usam para emparelhar;
//! - `{"k":"text"|"rect"|"border"|…}` — um item de pintura, já posicionado em
//!   coordenadas absolutas de documento.
//!
//! Juntos num ficheiro porque a pergunta "este desenho pertence a alguma caixa?"
//! cruza os dois, e emparelhar dois ficheiros medidos em corridas diferentes
//! acrescenta uma forma de erro que não tem nada a ver com o que se quer medir.
//! Os itens de pintura NÃO carregam o nó que os originou — a `DisplayList` não
//! o guarda, salvo no `BeginClip` — e é isso que torna a pergunta dos órfãos uma
//! pergunta genuína de geometria em vez de uma consulta a um campo.

use rts_dom::layout::{self, DisplayItem, DisplayList};
use rts_dom::{Dom, NodeIdx, NodeKind};

/// Escapar para JSON. O texto de um item de pintura é conteúdo do AUTOR — pode
/// ter aspas, barras, e na Wikipédia tem tudo — e uma linha partida não se lê
/// como um item mal escapado, lê-se como um item que o motor não desenhou.
fn jstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Um número com 2 casas: as coordenadas são `f32` e imprimi-las cruas gera
/// `123.45000000000001` em algumas e não noutras, o que faz um diff de dois
/// dumps acusar diferenças que não existem.
fn n(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// A travessia dos ELEMENTOS, com o caminho `html[1]/body[1]/div[3]/…`.
///
/// O índice conta irmãos DA MESMA TAG (1-based, como XPath), que é a regra que
/// `chrome_extract.mjs` e `examples/claude-parity-rts.ts` já usam. Tem de ser a
/// mesma ou os caminhos não casam — e caminhos que não casam aparecem como "só
/// num dos lados", que é a cara de um defeito de layout sem o ser.
///
/// Iterativa e não recursiva: a árvore da Wikipédia passa das 30 gerações e um
/// estouro de pilha aqui não daria um número pior, daria zero número.
fn elementos(dom: &Dom, raiz: NodeIdx) -> Vec<(NodeIdx, String)> {
    let mut saida = Vec::new();
    let mut pilha = vec![(raiz, String::from("html[1]"))];
    while let Some((idx, caminho)) = pilha.pop() {
        saida.push((idx, caminho.clone()));
        let mut vistos: Vec<(String, usize)> = Vec::new();
        let mut filhos = Vec::new();
        for &f in &dom.node(idx).children {
            let NodeKind::Element { tag } = &dom.node(f).kind else {
                continue;
            };
            let tag = tag.to_lowercase();
            let conta = match vistos.iter_mut().find(|(t, _)| *t == tag) {
                Some((_, c)) => {
                    *c += 1;
                    *c
                }
                None => {
                    vistos.push((tag.clone(), 1));
                    1
                }
            };
            filhos.push((f, format!("{caminho}/{tag}[{conta}]")));
        }
        // Ao contrário, para que a saída fique em ordem de documento.
        for f in filhos.into_iter().rev() {
            pilha.push(f);
        }
    }
    saida
}

/// A linha de um item de pintura, ou `None` para os que não têm geometria a
/// reportar. `dx`/`dy` são o deslocamento do fragmento em que o item vive — as
/// coordenadas saem absolutas, que é a única forma comparável com um
/// `getBoundingClientRect`.
fn item_linha(item: &DisplayItem, dx: f32, dy: f32, i: usize) -> Option<String> {
    let pre = format!("{{\"i\":{i}");
    Some(match item {
        DisplayItem::Text {
            x,
            y,
            text,
            color,
            size,
            mono,
            bold,
            italic,
            letter_spacing,
            decoration,
        } => format!(
            "{pre},\"k\":\"text\",\"x\":{},\"y\":{},\"t\":{},\"color\":{color},\
             \"size\":{},\"mono\":{mono},\"bold\":{bold},\"italic\":{italic},\
             \"ls\":{},\"deco\":{decoration}}}",
            n(x + dx),
            n(y + dy),
            jstr(text),
            n(*size),
            n(*letter_spacing)
        ),
        // O `radius` sai por canto: um marcador de lista é um quadrado com raio
        // igual a metade do lado, e é por essa forma que a régua o reconhece —
        // a `DisplayList` não diz que um `SolidRect` é um bullet.
        DisplayItem::SolidRect {
            rect,
            color,
            radius,
        } => format!(
            "{pre},\"k\":\"rect\",\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"color\":{color},\
             \"r\":[{},{},{},{}]}}",
            n(rect.x + dx),
            n(rect.y + dy),
            n(rect.w),
            n(rect.h),
            n(radius.tl),
            n(radius.tr),
            n(radius.br),
            n(radius.bl)
        ),
        DisplayItem::Border {
            rect,
            width,
            color,
            radius,
        } => format!(
            "{pre},\"k\":\"border\",\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"color\":{color},\
             \"bw\":{},\"r\":{}}}",
            n(rect.x + dx),
            n(rect.y + dy),
            n(rect.w),
            n(rect.h),
            n(*width),
            n(*radius)
        ),
        DisplayItem::GradientRect { rect, .. } => format!(
            "{pre},\"k\":\"gradient\",\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
            n(rect.x + dx),
            n(rect.y + dy),
            n(rect.w),
            n(rect.h)
        ),
        DisplayItem::Shadow { rect, .. } => format!(
            "{pre},\"k\":\"shadow\",\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
            n(rect.x + dx),
            n(rect.y + dy),
            n(rect.w),
            n(rect.h)
        ),
        DisplayItem::Quad { pts, .. } => {
            let x0 = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
            let y0 = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let x1 = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
            let y1 = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
            format!(
                "{pre},\"k\":\"quad\",\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
                n(x0 + dx),
                n(y0 + dy),
                n(x1 - x0),
                n(y1 - y0)
            )
        }
        DisplayItem::Image { rect, .. } | DisplayItem::Pixels { rect, .. } => format!(
            "{pre},\"k\":\"image\",\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
            n(rect.x + dx),
            n(rect.y + dy),
            n(rect.w),
            n(rect.h)
        ),
        // Os clips vão no dump e não são ruído: um item pintado dentro de um
        // clip de 1×1 não aparece no ecrã por muito que a sua posição esteja
        // certa, e sem eles a régua de conteúdo diria "desenhámos isto" sobre
        // algo que ninguém vê. É o mesmo mecanismo que já deixou a página toda
        // em branco uma vez (a regra de acessibilidade do MediaWiki).
        DisplayItem::BeginClip {
            rect,
            node,
            offset_x,
            offset_y,
            ..
        } => format!(
            "{pre},\"k\":\"clip+\",\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"node\":{node},\
             \"ox\":{},\"oy\":{}}}",
            n(rect.x + dx),
            n(rect.y + dy),
            n(rect.w),
            n(rect.h),
            n(*offset_x),
            n(*offset_y)
        ),
        DisplayItem::EndClip { .. } => format!("{pre},\"k\":\"clip-\"}}"),
        // A matriz de `transform` — mesma razão do clip: um item pintado sob
        // uma rotação/skew não fica onde o `rect` cru diz, e um dump de
        // conteúdo que ignorasse `PushTransform` reportaria a posição ERRADA
        // de tudo o que vem depois dele.
        DisplayItem::PushTransform { mat } => format!(
            "{pre},\"k\":\"transform+\",\"a\":{},\"b\":{},\"c\":{},\"d\":{},\"e\":{},\"f\":{}}}",
            n(mat.a),
            n(mat.b),
            n(mat.c),
            n(mat.d),
            n(mat.e + dx),
            n(mat.f + dy)
        ),
        DisplayItem::PopTransform => format!("{pre},\"k\":\"transform-\"}}"),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(caminho) = args.get(1) else {
        eprintln!("uso: claude-paint-dump <pagina.html>");
        std::process::exit(2);
    };
    let html = match std::fs::read_to_string(caminho) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("não li {caminho}: {e}");
            std::process::exit(2);
        }
    };
    let dom = rts_dom::parse_html_to_dom(&html);

    // 1280x800 é o default do `Dom` e o que o lado do Chrome é forçado a usar
    // (`Emulation.setDeviceMetricsOverride`). Dito aqui em vez de assumido.
    let ctx = layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &layout::ApproxMeasurer,
    };
    let list: DisplayList = layout::layout_document(&dom, &ctx);

    let mut linhas: Vec<String> = Vec::new();

    // A raiz do percurso é o `<html>`, e não a raiz sintética `#document`: é o
    // que faz o primeiro caminho ser `html[1]` dos dois lados.
    let raiz = dom.node(dom.root).children.iter().copied().find(|&f| {
        matches!(&dom.node(f).kind, NodeKind::Element { tag } if tag.eq_ignore_ascii_case("html"))
    });
    let Some(raiz) = raiz else {
        println!("{{\"__erro\":\"sem elemento <html>\"}}");
        std::process::exit(1);
    };

    // Elementos SEM caixa saem com `"sem_caixa":true` em vez de não saírem: um
    // elemento que o layout não posicionou é informação, e descontá-lo em
    // silêncio é encolher o denominador — a falha mais cara que uma régua tem.
    let els = elementos(&dom, raiz);
    let mut sem_caixa = 0usize;
    for (idx, caminho) in &els {
        let tag = match &dom.node(*idx).kind {
            NodeKind::Element { tag } => tag.to_lowercase(),
            _ => "?".to_string(),
        };
        match list.rect_of(*idx) {
            Some(r) => linhas.push(format!(
                "{{\"k\":\"el\",\"p\":{},\"tag\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
                jstr(caminho),
                jstr(&tag),
                n(r.x),
                n(r.y),
                n(r.w),
                n(r.h)
            )),
            None => {
                sem_caixa += 1;
                linhas.push(format!(
                    "{{\"k\":\"el\",\"p\":{},\"tag\":{},\"sem_caixa\":true}}",
                    jstr(caminho),
                    jstr(&tag)
                ));
            }
        }
    }

    let mut itens = 0usize;
    let mut i = 0usize;
    list.walk(|item, dx, dy| {
        if let Some(l) = item_linha(item, dx, dy, i) {
            linhas.push(l);
            itens += 1;
        }
        i += 1;
    });

    let meta = format!(
        "{{\"__meta\":1,\"lado\":\"rts-paint\",\"ficheiro\":{},\"bytes\":{},\
         \"viewport\":[1280,800],\"medidor\":\"ApproxMeasurer\",\
         \"canvas_bg\":{},\"content_height\":{}}}",
        jstr(caminho),
        html.len(),
        list.canvas_background,
        n(list.content_height)
    );
    let fim = format!(
        "{{\"__fim\":1,\"elementos\":{},\"sem_caixa\":{sem_caixa},\"itens\":{itens},\
         \"emitidos\":{}}}",
        els.len(),
        linhas.len()
    );

    let mut saida = String::with_capacity(linhas.len() * 96);
    saida.push_str(&meta);
    saida.push('\n');
    for l in &linhas {
        saida.push_str(l);
        saida.push('\n');
    }
    saida.push_str(&fim);
    saida.push('\n');
    print!("{saida}");
    eprintln!(
        "rts-paint: {} elementos ({sem_caixa} sem caixa), {itens} itens de pintura",
        els.len()
    );
}
