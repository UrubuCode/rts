//! HARNESS de métricas do DOM: roda cenários fixos sobre páginas HTML reais e
//! diz, por cenário, quanto tempo levou, o que aconteceu por dentro e se a
//! árvore terminou CONSISTENTE.
//!
//! ```bash
//! # o que aconteceu por dentro (contadores e fases valem; o tempo desta execução não)
//! cargo run --release -q -p rts-dom --features metrics --example dom_metrics -- pagina.html
//!
//! # quanto custou (tempo vale; contadores saem zerados, e o harness diz isso)
//! cargo run --release -q -p rts-dom --example dom_metrics -- pagina.html
//!
//! # gravar um baseline e comparar depois — é o diff de CONTADORES que acusa
//! # mudança de comportamento; tempo varia com a máquina, contador não.
//! … --features metrics … -- pagina.html --json base.json
//! … --features metrics … -- pagina.html --baseline base.json
//! ```
//!
//! Opções: `--viewport 1280x800`, `--iters 20`, `--build 4000`, `--json <arq>`,
//! `--baseline <arq>`, `--tolerance 5` (% de variação ignorada no diff).
//!
//! Nunca em build de debug: um número de debug não é um número.

mod report;
mod scenarios;

use report::Run;
use rts_dom::metrics;
use rts_dom::parse_html_to_dom;
use scenarios::CountingMeasurer;

struct Options {
    files: Vec<String>,
    vw: f32,
    vh: f32,
    iters: u32,
    build_n: u32,
    json_out: Option<String>,
    baseline: Option<String>,
    tolerance: f64,
}

fn parse_args() -> Options {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut o = Options {
        files: Vec::new(),
        vw: 1280.0,
        vh: 800.0,
        iters: 20,
        build_n: 2000,
        json_out: None,
        baseline: None,
        tolerance: 5.0,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--viewport" => {
                if let Some((w, h)) = it.next().and_then(|v| v.split_once('x').map(|(a, b)| (a.to_string(), b.to_string()))) {
                    o.vw = w.parse().unwrap_or(o.vw);
                    o.vh = h.parse().unwrap_or(o.vh);
                }
            }
            "--iters" => o.iters = it.next().and_then(|v| v.parse().ok()).unwrap_or(o.iters),
            "--build" => o.build_n = it.next().and_then(|v| v.parse().ok()).unwrap_or(o.build_n),
            "--json" => o.json_out = it.next().cloned(),
            "--baseline" => o.baseline = it.next().cloned(),
            "--tolerance" => o.tolerance = it.next().and_then(|v| v.parse().ok()).unwrap_or(o.tolerance),
            other => o.files.push(other.to_string()),
        }
    }
    o
}

fn main() {
    let o = parse_args();
    println!("rts-dom · métricas");
    if metrics::enabled() {
        println!(
            "instrumentação LIGADA — contadores e fases valem; o TEMPO desta execução\n\
             não é comparável a um build sem `--features metrics`."
        );
    } else {
        println!(
            "instrumentação DESLIGADA — os tempos valem; contadores e fases saem zerados.\n\
             Rode com `--features metrics` para ver o que aconteceu por dentro."
        );
    }
    if o.files.is_empty() {
        println!(
            "\nuso: dom_metrics <arquivo.html>… [--viewport 1280x800] [--iters 20]\n\
             \x20            [--build 2000] [--json saida.json] [--baseline base.json]"
        );
    }

    let baseline = o.baseline.as_ref().and_then(|p| match std::fs::read_to_string(p) {
        Ok(text) => Some(text),
        Err(e) => {
            eprintln!("!! baseline {p}: {e}");
            None
        }
    });

    let mut json_blocks: Vec<String> = Vec::new();
    for f in &o.files {
        if let Some(block) = bench_file(f, &o, baseline.as_deref()) {
            json_blocks.push(block);
        }
    }
    let m = CountingMeasurer::new();
    println!("\n═══ construção programática ({} elementos)", o.build_n);
    let runs = scenarios::build(o.build_n, o.vw, o.vh, &m);
    for r in &runs {
        r.print();
    }
    json_blocks.push(json_group("build", &runs));

    if let Some(path) = &o.json_out {
        let text = format!("[\n{}\n]\n", json_blocks.join(",\n"));
        match std::fs::write(path, text) {
            Ok(()) => println!("\nJSON gravado em {path}"),
            Err(e) => eprintln!("!! não gravou {path}: {e}"),
        }
    }
}

/// Um grupo (arquivo ou `build`) como um objeto JSON com seus cenários dentro.
fn json_group(name: &str, runs: &[Run]) -> String {
    let inner: Vec<String> = runs.iter().map(|r| indent(&r.to_json())).collect();
    format!("  {{\n    \"group\": \"{name}\",\n    \"runs\": [\n{}\n    ]\n  }}", inner.join(",\n"))
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("      {l}")).collect::<Vec<_>>().join("\n")
}

fn bench_file(path: &str, o: &Options, baseline: Option<&str>) -> Option<String> {
    let html = match std::fs::read_to_string(path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("!! {path}: {e}");
            return None;
        }
    };
    let m = CountingMeasurer::new();

    // Uma árvore só para DESCREVER a página: a forma é o denominador de tudo o
    // mais (4000 cascades é muito ou pouco depende de quantos elementos há).
    let probe = parse_html_to_dom(&html);
    let audit = metrics::audit(&probe);
    println!("\n═══ {path}");
    println!("    {} bytes de HTML · viewport {}×{}", html.len(), o.vw as u32, o.vh as u32);
    print!("{}", audit.report());
    drop(probe);

    let runs = scenarios::page(&html, o.vw, o.vh, o.iters, &m);
    for r in &runs {
        r.print();
    }

    // Normaliza pelo tamanho da árvore — é o que permite comparar páginas.
    if let Some(cold) = runs.iter().find(|r| r.name == "layout frio") {
        let t = cold.timing();
        println!(
            "\n  layout frio: {:.2} µs por nó · {:.2} µs por elemento",
            t.avg * 1000.0 / audit.shape.nodes.max(1) as f64,
            t.avg * 1000.0 / audit.shape.elements.max(1) as f64,
        );
    }

    if let Some(text) = baseline {
        print_baseline_diff(text, path, &runs, o.tolerance);
    }
    Some(json_group(path, &runs))
}

/// Diff contra o baseline, cenário a cenário. Compara CONTADORES: são
/// determinísticos, então uma diferença é sempre mudança de comportamento — ao
/// contrário do tempo, que pode ser a máquina.
fn print_baseline_diff(baseline: &str, group: &str, runs: &[Run], tolerance: f64) {
    println!("\n  diff contra o baseline (contadores, |Δ| ≥ {tolerance:.0}%)");
    let Some(group_block) = find_block(baseline, &format!("\"group\": \"{group}\"")) else {
        println!("    (o baseline não tem este arquivo)");
        return;
    };
    let mut any = false;
    for r in runs {
        let Some(block) = find_block(group_block, &format!("\"scenario\": \"{}\"", r.name)) else {
            continue;
        };
        let before = metrics::DomMetrics::from_json(block);
        let before_iters = read_int(block, "\"iters\"").unwrap_or(1);
        let d = report::diff_counters(
            &r.counters,
            r.samples.len(),
            &before,
            before_iters,
            tolerance,
        );
        if !d.is_empty() {
            any = true;
            println!("  ▸ {}", r.name);
            print!("{d}");
        }
    }
    if !any {
        println!("    nada mudou além da tolerância");
    }
}

/// Lê um inteiro `"chave": N` de um bloco. Mesma razão do `find_block`: reler o
/// que este harness grava não justifica uma dependência de JSON num crate que
/// não tem nenhuma.
fn read_int(text: &str, key: &str) -> Option<usize> {
    let at = text.find(key)?;
    let rest = &text[at + key.len()..];
    let colon = rest.find(':')?;
    let digits: String =
        rest[colon + 1..].trim_start().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Recorta o pedaço de texto que começa numa marca e vai até a próxima
/// ocorrência da MESMA chave — bom o bastante para reler o que este harness
/// grava, e sem trazer uma dependência de JSON a um crate que não tem nenhuma.
fn find_block<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let start = text.find(marker)?;
    let rest = &text[start..];
    let key = marker.split(':').next().unwrap_or(marker);
    let end = rest[marker.len()..].find(key).map(|e| e + marker.len()).unwrap_or(rest.len());
    Some(&rest[..end])
}
