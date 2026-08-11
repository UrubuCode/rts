//! Roda o probe da máquina e imprime a tabela.
//!
//! ```text
//! cargo run --release -p rts-cranelift --example probe_run
//! ```
//!
//! # Por que isto faltava
//!
//! `src/probe/` existe desde que este crate existe e diz, no próprio doc, que os
//! números dele são um CONTRATO: uma regressão neles é regressão da máquina, e um
//! programa lento cujos números do probe não mudaram tem problema noutro lugar.
//!
//! Só que nada os rodava. `.claude/skills/perf-claim/SKILL.md` manda
//! `cargo test -p rts-cranelift --test probe`, e esse alvo não existe — não há
//! `tests/probe.rs`. Um contrato que ninguém pode ler não vincula nada.
//!
//! Um EXEMPLO e não um teste, de propósito: um teste que falha por lentidão
//! falha na máquina de outra pessoa, no CI compartilhado, na terça de manhã. O
//! probe é uma régua para ser lida por quem está medindo, não um portão.

fn main() {
    let profile = rts_cranelift::probe::Profile::current();
    println!("probe da maquina — perfil: {profile:?}");
    if !profile.is_meaningful() {
        println!();
        println!("  ATENCAO: este e um build DEBUG e o numero nao vale nada.");
        println!("  Rode com --release. (`CLAUDE.md`: um numero de debug nao e um numero.)");
        println!();
    }
    println!();
    println!("  {:<14} {:>12}   {}", "fixture", "ns/op", "o que e");
    println!("  {:-<14} {:->12}   {:-<48}", "", "", "");

    let mut base = 0.0_f64;
    for fixture in rts_cranelift::probe::all() {
        // Iterações suficientes para que o relógio do SO não seja o limite: o
        // piso é uma soma, e uma soma medida em dezenas de milhares de passadas
        // some no ruído.
        let measurement = rts_cranelift::probe::measure(&fixture, 2_000_000);
        let ns = measurement.nanos_per_op();
        if fixture.name == "arithmetic" {
            base = ns;
        }
        // A RAZÃO contra a aritmética é o que se lê, não o valor absoluto: o
        // absoluto varia com a máquina, e a razão é o que diz se uma operação
        // custa o que deveria.
        let vezes = if base > 0.0 { format!("{:.1}x", ns / base) } else { "-".to_owned() };
        println!(
            "  {:<14} {:>9.2} ns {:>6}   {}",
            fixture.name, ns, vezes, fixture.about
        );
        // O checksum é impresso para que um otimizador que apagasse o trabalho
        // apareça como um número bom demais em vez de como um número rápido.
        debug_assert!(measurement.checksum != i64::MIN, "o fixture nao computou nada");
    }
    println!();
    println!("  A coluna de vezes e contra `arithmetic`, que e o piso da maquina.");
    println!("  Uma leitura de campo que custe MUITO mais que o piso e onde a");
    println!("  camada de cima deve procurar antes de culpar o proprio codigo.");
}
