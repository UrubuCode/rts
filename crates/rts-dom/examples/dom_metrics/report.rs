//! O RESULTADO de um cenário e como ele é impresso, serializado e comparado.
//!
//! Três formas da mesma medição, e cada uma existe por um motivo diferente: a
//! tabela é para ler agora, o JSON é para comparar depois, e o diff contra um
//! baseline é o único que responde "isto piorou?" — que é a pergunta que uma
//! tabela sozinha nunca responde, porque ninguém lembra o número da semana
//! passada.

use rts_dom::metrics::{AuditReport, DomMetrics, Footprint, Phases, Samples};
use std::time::Duration;

/// Uma execução medida: N amostras de tempo mais o que aconteceu por dentro.
pub struct Run {
    pub name: &'static str,
    /// O que UMA iteração representa ("página", "frame", "consulta").
    pub unit: &'static str,
    /// Uma duração POR iteração — guardadas todas, porque a média sozinha
    /// esconde o pico, e é o pico que faz uma página parecer travada.
    pub samples: Vec<Duration>,
    pub counters: DomMetrics,
    pub phases: Phases,
    /// QUAIS foram os casos por trás dos contadores de descarte (seletores
    /// recusados, propriedades ignoradas): a lista de trabalho que um número
    /// sozinho não dá.
    pub notes: Samples,
    /// `(text_width, line_height)` — o único trabalho do layout que sai do crate.
    pub text_measures: (u64, u64),
    /// A árvore ao FIM do cenário. `None` quando o cenário não deixa uma (o
    /// `parse` descarta cada árvore que cria).
    pub audit: Option<AuditReport>,
    /// A PEGADA ao fim do cenário, quando há árvore. Comparada com a do início,
    /// é o que mostra o estado derivado crescendo sem a árvore crescer.
    pub footprint: Option<(Footprint, Footprint)>,
}

/// Estatísticas de tempo de uma execução, em milissegundos.
pub struct Timing {
    pub calls: usize,
    pub min: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    pub avg: f64,
    pub total: f64,
}

impl Run {
    pub fn timing(&self) -> Timing {
        let mut ms: Vec<f64> = self
            .samples
            .iter()
            .map(|d| d.as_secs_f64() * 1000.0)
            .collect();
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = ms.len();
        let at = |q: f64| -> f64 {
            if n == 0 {
                return 0.0;
            }
            ms[(((n - 1) as f64) * q).round() as usize]
        };
        Timing {
            calls: n,
            min: ms.first().copied().unwrap_or(0.0),
            p50: at(0.5),
            p95: at(0.95),
            max: ms.last().copied().unwrap_or(0.0),
            avg: if n == 0 {
                0.0
            } else {
                ms.iter().sum::<f64>() / n as f64
            },
            total: ms.iter().sum(),
        }
    }

    pub fn print(&self) {
        let t = self.timing();
        println!(
            "\n▸ {:<26} {:.3} ms/{}  (p50 {:.3} · p95 {:.3} · máx {:.3} · {} iter)",
            self.name, t.avg, self.unit, t.p50, t.p95, t.max, t.calls
        );
        let report = self.counters.report();
        if report.is_empty() {
            println!("    (sem contadores — compile com --features metrics)");
        } else {
            print!("{report}");
        }
        if !self.phases.is_empty() {
            println!("  fases (% sobre o tempo do cenário; fases se aninham)");
            print!("{}", self.phases.report_over(t.total));
        }
        if !self.notes.is_empty() {
            println!("  amostras");
            print!("{}", self.notes.report());
        }
        if self.text_measures.0 + self.text_measures.1 > 0 {
            println!(
                "  medição de texto\n    {:<38} {:>13}\n    {:<38} {:>13}",
                "text_width", self.text_measures.0, "line_height", self.text_measures.1
            );
        }
        self.print_ratios();
        if let Some((antes, depois)) = &self.footprint {
            println!("  memória (fim do cenário)");
            print!("{}", depois.report());
            let (a, d) = (antes.total(), depois.total());
            if d != a {
                println!(
                    "      Δ desde o início do cenário: {:+.1} KiB ({:+.1}%)",
                    (d as f64 - a as f64) / 1024.0,
                    (d as f64 - a as f64) * 100.0 / a.max(1) as f64
                );
            }
        }
        if let Some(a) = &self.audit {
            if a.bugs() > 0 || a.leaks() > 0 || a.page_issues() > 0 {
                println!("  consistência ao FIM do cenário");
                print!("{}", a.report());
            }
        }
    }

    /// As RAZÕES: um número absoluto de hits não diz se o cache está
    /// funcionando; a fração diz. E a razão por ELEMENTO é o que permite
    /// comparar páginas de tamanhos diferentes.
    fn print_ratios(&self) {
        let c = &self.counters;
        let mut lines: Vec<String> = Vec::new();
        if c.computed_calls > 0 {
            lines.push(format!(
                "estilo servido pelo memo: {:.1}%  ({} cascades completas)",
                pct(c.computed_memo_hits, c.computed_calls),
                c.cascade_runs
            ));
        }
        if c.measure_calls > 0 {
            lines.push(format!(
                "measure_block do cache: {:.1}%",
                pct(c.measure_hits, c.measure_calls)
            ));
        }
        if c.intrinsic_calls > 0 {
            lines.push(format!(
                "largura intrínseca do cache: {:.1}%",
                pct(c.intrinsic_hits, c.intrinsic_calls)
            ));
        }
        if c.cascade_runs > 0 && c.rules_considered > 0 {
            lines.push(format!(
                "regras testadas por cascade: {:.1} (casaram {:.1})",
                c.rules_considered as f64 / c.cascade_runs as f64,
                c.rules_matched as f64 / c.cascade_runs as f64,
            ));
        }
        if c.query_calls > 0 {
            lines.push(format!(
                "nós visitados por consulta: {:.0}",
                c.query_nodes_visited as f64 / c.query_calls as f64
            ));
        }
        if c.touch_global > 0 && c.memo_cleared_entries > 0 {
            lines.push(format!(
                "memos jogados fora por touch() global: {:.0} em média",
                c.memo_cleared_entries as f64 / c.touch_global as f64
            ));
        }
        if !lines.is_empty() {
            println!("  razões");
            for l in lines {
                println!("    {l}");
            }
        }
    }

    /// Um objeto JSON com tempo, contadores e fases deste cenário.
    pub fn to_json(&self) -> String {
        let t = self.timing();
        format!(
            "{{\n  \"scenario\": \"{}\",\n  \"unit\": \"{}\",\n  \"iters\": {},\n  \
             \"ms\": {{ \"avg\": {:.6}, \"p50\": {:.6}, \"p95\": {:.6}, \"min\": {:.6}, \"max\": {:.6} }},\n  \
             \"counters\": {},\n  \"phases\": {}\n}}",
            self.name,
            self.unit,
            t.calls,
            t.avg,
            t.p50,
            t.p95,
            t.min,
            t.max,
            self.counters.to_json(),
            self.phases.to_json(),
        )
    }
}

pub fn pct(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}

/// Compara os CONTADORES de um cenário com os de um baseline, POR ITERAÇÃO, e
/// imprime só o que mudou. Duas decisões aqui, e as duas são sobre não gerar
/// alarme falso:
///
/// - **contadores, não tempos**: um contador é determinístico, então uma
///   diferença nele é sempre mudança de COMPORTAMENTO; uma diferença de tempo
///   pode ser a máquina.
/// - **por iteração**: um baseline gravado com `--iters 8` comparado a uma
///   execução com `--iters 4` acusaria −50% em TODAS as linhas. A taxa por
///   iteração é o que é comparável entre execuções.
pub fn diff_counters(
    now: &DomMetrics,
    now_iters: usize,
    base: &DomMetrics,
    base_iters: usize,
    tolerance_pct: f64,
) -> String {
    let mut out = String::new();
    let (ni, bi) = (now_iters.max(1) as f64, base_iters.max(1) as f64);
    let base_rows = base.rows();
    for (i, (_, label, key, value)) in now.rows().iter().enumerate() {
        let Some((_, _, base_key, before)) = base_rows.get(i) else {
            continue;
        };
        debug_assert_eq!(base_key, key);
        let (a, b) = (*before as f64 / bi, *value as f64 / ni);
        if (a - b).abs() < 1e-9 {
            continue;
        }
        let change = if a == 0.0 { 100.0 } else { (b - a) * 100.0 / a };
        if change.abs() < tolerance_pct {
            continue;
        }
        out.push_str(&format!(
            "    {:<38} {:>12.1} → {:<12.1} {:+.1}%  (por iteração)
",
            label, a, b, change
        ));
    }
    out
}
