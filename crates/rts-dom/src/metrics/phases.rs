//! FASES cronometradas — quanto tempo cada etapa nomeada levou, e quantas vezes.
//!
//! Um contador diz quantas vezes a cascade rodou; uma fase diz quanto do frame
//! ela levou. As duas juntas respondem a pergunta que nenhuma responde sozinha:
//! *4000 cascades são caras?* — só se `cascade` for uma fatia grande do frame.
//!
//! ## Por que o registro de fases mora AQUI e não em quem mede
//!
//! As fases interessantes atravessam crates: `load-html` e `cascade` são deste
//! crate, `layout` é deste mas quem o chama por frame é o `rts-egui`, e `paint`,
//! `frame` e o tempo de um callback de página são de fora. Um registro por crate
//! daria três relatórios que não somam — e a pergunta "o que come o frame" só
//! existe se todas as fatias estiverem na MESMA conta. O `rts-dom` é o crate
//! comum a todos eles (e o único sem dependências), então o registro é aqui e
//! os de fora chamam [`scope`].
//!
//! Nomes são `&'static str` de propósito: uma fase é um ponto do CÓDIGO, não um
//! dado. Um nome montado em tempo de execução criaria uma linha nova por
//! iteração e o relatório viraria um log.
//!
//! ## Reentrância
//!
//! `layout` chamado de dentro de `frame` acumula nos DOIS — as fases se aninham
//! e a soma delas passa de 100% do tempo de parede. É intencional: é o que
//! permite ler "`layout` é 80% de `frame`". [`PhaseStats::depth_max`] registra o
//! aninhamento máximo visto, que é como se percebe uma recursão inesperada.

/// O acumulado de UMA fase.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhaseStats {
    /// Quantas vezes a fase começou e terminou.
    pub calls: u64,
    /// Soma dos tempos, em nanossegundos.
    pub total_ns: u64,
    /// A execução mais rápida.
    pub min_ns: u64,
    /// A mais lenta — o que um pico de frame realmente custou.
    pub max_ns: u64,
    /// Maior aninhamento observado (1 = nunca reentrou).
    pub depth_max: u32,
}

impl PhaseStats {
    /// Média em milissegundos. Média SEM o `max_ns` ao lado engana: um frame
    /// médio de 4 ms com pico de 90 ms é uma página que trava, não uma suave.
    pub fn avg_ms(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            self.total_ns as f64 / self.calls as f64 / 1e6
        }
    }
    pub fn total_ms(&self) -> f64 {
        self.total_ns as f64 / 1e6
    }
    pub fn max_ms(&self) -> f64 {
        self.max_ns as f64 / 1e6
    }
    pub fn min_ms(&self) -> f64 {
        self.min_ns as f64 / 1e6
    }
}

/// Todas as fases vistas, na ordem em que apareceram pela primeira vez — que é a
/// ordem do PIPELINE quando a instrumentação segue o caminho do dado, e vale
/// mais do que a alfabética.
#[derive(Clone, Debug, Default)]
pub struct Phases {
    entries: Vec<(&'static str, PhaseStats)>,
}

impl Phases {
    pub fn get(&self, name: &str) -> Option<PhaseStats> {
        self.entries
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| *s)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, PhaseStats)> + '_ {
        self.entries.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `self - base`, fase a fase — o intervalo entre dois snapshots. `min`/`max`
    /// não são subtraíveis: ficam os do snapshot atual, e o relatório os rotula
    /// como acumulados desde o último `reset`.
    pub fn delta(&self, base: &Phases) -> Phases {
        let mut out = Phases::default();
        for (name, s) in self.iter() {
            let b = base.get(name).unwrap_or_default();
            out.entries.push((
                name,
                PhaseStats {
                    calls: s.calls.saturating_sub(b.calls),
                    total_ns: s.total_ns.saturating_sub(b.total_ns),
                    min_ns: s.min_ns,
                    max_ns: s.max_ns,
                    depth_max: s.depth_max,
                },
            ));
        }
        out
    }

    /// Tabela legível, com a fatia de cada fase sobre a MAIOR delas.
    pub fn report(&self) -> String {
        let widest = self.iter().map(|(_, s)| s.total_ns).max().unwrap_or(0);
        self.report_over(widest as f64 / 1e6)
    }

    /// Tabela legível com a fatia calculada sobre um total EXTERNO — o tempo de
    /// parede do cenário, que é a referência certa: sem ela, uma fase de 3 ms
    /// não diz se comeu o frame ou se foi ruído dentro de 300 ms. As fases se
    /// aninham, então a soma das fatias pode passar de 100% — é o que permite
    /// ler "layout é 80% do frame E cascade é 60% do layout".
    pub fn report_over(&self, total_ms: f64) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let root = (total_ms * 1e6).max(1.0) as u64;
        let mut out = format!(
            "    {:<22} {:>7} {:>11} {:>10} {:>10} {:>7}\n",
            "fase", "vezes", "total ms", "média ms", "máx ms", "% total"
        );
        for (name, s) in self.iter() {
            if s.calls == 0 {
                continue;
            }
            out.push_str(&format!(
                "    {:<22} {:>7} {:>11.3} {:>10.3} {:>10.3} {:>6.1}%\n",
                name,
                s.calls,
                s.total_ms(),
                s.avg_ms(),
                s.max_ms(),
                s.total_ns as f64 * 100.0 / root as f64,
            ));
        }
        out
    }

    /// JSON `{"fase": {"calls":…, "total_ns":…, …}, …}`.
    pub fn to_json(&self) -> String {
        let body: Vec<String> = self
            .iter()
            .map(|(name, s)| {
                format!(
                    "    \"{name}\": {{ \"calls\": {}, \"total_ns\": {}, \"min_ns\": {}, \"max_ns\": {}, \"depth_max\": {} }}",
                    s.calls, s.total_ns, s.min_ns, s.max_ns, s.depth_max
                )
            })
            .collect();
        format!("{{\n{}\n  }}", body.join(",\n"))
    }
}

#[cfg(feature = "metrics")]
thread_local! {
    static PHASES: std::cell::RefCell<Phases> = std::cell::RefCell::new(Phases::default());
    /// Profundidade corrente por fase — para medir aninhamento sem procurar na
    /// pilha de chamadas.
    static DEPTH: std::cell::RefCell<Vec<(&'static str, u32)>> =
        std::cell::RefCell::new(Vec::new());
}

/// Guarda RAII de uma fase: começa a contar ao ser criada, fecha ao sair do
/// escopo — inclusive por `?` ou por pânico, que é o motivo de ser um guard e
/// não um par `begin`/`end` (um `end` esquecido num caminho de erro produz uma
/// fase que nunca fecha e um relatório que mente).
#[must_use = "a fase é medida enquanto o guard vive; descartá-lo a fecha na hora"]
pub struct Scope {
    #[cfg(feature = "metrics")]
    name: &'static str,
    #[cfg(feature = "metrics")]
    start: std::time::Instant,
}

impl Drop for Scope {
    fn drop(&mut self) {
        #[cfg(feature = "metrics")]
        {
            let ns = self.start.elapsed().as_nanos() as u64;
            let depth = DEPTH.with(|d| {
                let mut d = d.borrow_mut();
                let cur = d
                    .iter_mut()
                    .find(|(n, _)| *n == self.name)
                    .map(|(_, c)| {
                        *c = c.saturating_sub(1);
                        *c + 1
                    })
                    .unwrap_or(1);
                cur
            });
            PHASES.with(|p| {
                let mut p = p.borrow_mut();
                let name = self.name;
                if let Some((_, s)) = p.entries.iter_mut().find(|(n, _)| *n == name) {
                    s.calls += 1;
                    s.total_ns = s.total_ns.wrapping_add(ns);
                    s.min_ns = if s.min_ns == 0 { ns } else { s.min_ns.min(ns) };
                    s.max_ns = s.max_ns.max(ns);
                    s.depth_max = s.depth_max.max(depth);
                } else {
                    p.entries.push((
                        name,
                        PhaseStats {
                            calls: 1,
                            total_ns: ns,
                            min_ns: ns,
                            max_ns: ns,
                            depth_max: depth,
                        },
                    ));
                }
            });
        }
    }
}

/// Abre uma fase. Guarde o retorno numa variável — soltá-lo imediatamente
/// (`let _ = scope("x")`) mede zero, e é o erro que o `#[must_use]` pega.
///
/// ```ignore
/// let _t = rts_dom::metrics::phases::scope("layout");
/// ```
pub fn scope(name: &'static str) -> Scope {
    #[cfg(feature = "metrics")]
    {
        DEPTH.with(|d| {
            let mut d = d.borrow_mut();
            match d.iter_mut().find(|(n, _)| *n == name) {
                Some((_, c)) => *c += 1,
                None => d.push((name, 1)),
            }
        });
        Scope {
            name,
            start: std::time::Instant::now(),
        }
    }
    #[cfg(not(feature = "metrics"))]
    {
        let _ = name;
        Scope {}
    }
}

/// O acumulado de fases desta thread.
pub fn phase_snapshot() -> Phases {
    #[cfg(feature = "metrics")]
    {
        PHASES.with(|p| p.borrow().clone())
    }
    #[cfg(not(feature = "metrics"))]
    {
        Phases::default()
    }
}

/// Zera as fases desta thread.
pub fn reset() {
    #[cfg(feature = "metrics")]
    {
        PHASES.with(|p| *p.borrow_mut() = Phases::default());
        DEPTH.with(|d| d.borrow_mut().clear());
    }
}

#[cfg(all(test, feature = "metrics"))]
mod tests {
    use super::*;

    /// Uma fase fechada aparece com a contagem certa — e o relatório sem fases
    /// não imprime cabeçalho nenhum (o harness distingue "não mediu" de "mediu
    /// zero").
    #[test]
    fn fase_acumula_e_o_vazio_nao_imprime() {
        reset();
        assert!(phase_snapshot().report().is_empty());
        {
            let _t = scope("teste");
        }
        {
            let _t = scope("teste");
        }
        let p = phase_snapshot();
        assert_eq!(p.get("teste").unwrap().calls, 2);
        assert!(p.report().contains("teste"));
    }

    /// Aninhar a MESMA fase é reentrância legítima (layout dentro de layout) e
    /// precisa aparecer, senão uma recursão inesperada passa despercebida.
    #[test]
    fn aninhamento_e_registrado() {
        reset();
        {
            let _a = scope("rec");
            let _b = scope("rec");
        }
        assert_eq!(phase_snapshot().get("rec").unwrap().depth_max, 2);
    }
}
