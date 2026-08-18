//! AMOSTRAS nomeadas — um contador diz *quantos*, isto diz *quais*.
//!
//! "2632 regras descartadas" é um número acionável só depois de virar
//! "`:is(...)`, `::before`, `[data-x=y i]`". A diferença entre os dois é o que
//! separa um relatório de desempenho de uma lista de trabalho, e é por isso que
//! as duas coisas existem lado a lado: o contador é barato e completo, a
//! amostra é cara e limitada.
//!
//! ## O limite é a razão de isto ser seguro
//!
//! Cada categoria guarda no máximo [`MAX_PER_KIND`] amostras DISTINTAS e para.
//! Um `<style>` do Bootstrap descarta milhares de seletores; guardar todos
//! transformaria a instrumentação num log e o custo dela na medição. Trinta e
//! duas amostras distintas já dizem de que TIPO é o problema — que é a pergunta
//! — e a contagem completa continua no contador ao lado.
//!
//! Sem a feature `metrics`, [`note!`] não avalia nem a string: o `format!` que
//! monta a amostra é a parte cara, e ele não acontece.

/// Teto de amostras distintas por categoria. Trinta e duas cobrem a variedade
/// sem virar log; a contagem exata vive no contador correspondente.
pub const MAX_PER_KIND: usize = 32;

/// As amostras coletadas, por categoria, na ordem em que apareceram.
#[derive(Clone, Debug, Default)]
pub struct Samples {
    entries: Vec<(&'static str, Vec<String>, u64)>,
}

impl Samples {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// As amostras de uma categoria e quantas vezes ela ocorreu ao todo.
    pub fn get(&self, kind: &str) -> Option<(&[String], u64)> {
        self.entries.iter().find(|(k, _, _)| *k == kind).map(|(_, v, n)| (v.as_slice(), *n))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &[String], u64)> + '_ {
        self.entries.iter().map(|(k, v, n)| (*k, v.as_slice(), *n))
    }

    /// Relatório legível: a categoria, o total, e as amostras distintas.
    /// Diz explicitamente quando truncou — uma lista cortada que não avisa é
    /// uma lista que mente sobre a variedade do problema.
    pub fn report(&self) -> String {
        let mut out = String::new();
        for (kind, list, total) in self.iter() {
            out.push_str(&format!("    {kind} (×{total}):\n"));
            for s in list {
                out.push_str(&format!("      · {s}\n"));
            }
            if list.len() >= MAX_PER_KIND {
                out.push_str(&format!(
                    "      … só as {MAX_PER_KIND} primeiras distintas; o total está acima\n"
                ));
            }
        }
        out
    }
}

#[cfg(feature = "metrics")]
thread_local! {
    static SAMPLES: std::cell::RefCell<Samples> = std::cell::RefCell::new(Samples::default());
}

/// Registra uma amostra: `note!("seletor-descartado", format!("{sel}"))`.
/// Sem a feature, NADA é avaliado — nem a expressão que monta o texto.
#[macro_export]
macro_rules! note {
    ($kind:literal, $text:expr) => {{
        #[cfg(feature = "metrics")]
        $crate::metrics::samples::record($kind, || $text);
    }};
}

/// Guarda uma amostra distinta na categoria, respeitando o teto. A closure só é
/// chamada enquanto há espaço: passado o teto, o custo por ocorrência volta a
/// ser um incremento.
#[cfg(feature = "metrics")]
pub fn record(kind: &'static str, text: impl FnOnce() -> String) {
    SAMPLES.with(|s| {
        let Ok(mut s) = s.try_borrow_mut() else { return };
        match s.entries.iter_mut().find(|(k, _, _)| *k == kind) {
            Some((_, list, total)) => {
                *total += 1;
                if list.len() >= MAX_PER_KIND {
                    return;
                }
                let t = text();
                if !list.contains(&t) {
                    list.push(t);
                }
            }
            None => s.entries.push((kind, vec![text()], 1)),
        }
    });
}

/// As amostras desta thread.
pub fn snapshot() -> Samples {
    #[cfg(feature = "metrics")]
    {
        SAMPLES.with(|s| s.borrow().clone())
    }
    #[cfg(not(feature = "metrics"))]
    {
        Samples::default()
    }
}

/// Esquece as amostras desta thread.
pub fn reset() {
    #[cfg(feature = "metrics")]
    SAMPLES.with(|s| *s.borrow_mut() = Samples::default());
}

#[cfg(all(test, feature = "metrics"))]
mod tests {
    use super::*;

    /// Repetição não infla a lista, mas conta no total — é o que faz "×2632"
    /// caber ao lado de três exemplos.
    #[test]
    fn distintas_na_lista_todas_no_total() {
        reset();
        crate::note!("t", "a".to_string());
        crate::note!("t", "a".to_string());
        crate::note!("t", "b".to_string());
        let taken = snapshot();
        let (list, total) = taken.get("t").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(total, 3);
    }

    /// Passado o teto, a coleta para e o relatório AVISA que parou.
    #[test]
    fn o_teto_e_declarado_no_relatorio() {
        reset();
        for i in 0..(MAX_PER_KIND + 10) {
            crate::note!("t", format!("s{i}"));
        }
        let s = snapshot();
        assert_eq!(s.get("t").unwrap().0.len(), MAX_PER_KIND);
        assert!(s.report().contains("primeiras distintas"));
    }
}
