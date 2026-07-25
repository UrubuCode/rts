//! Issue #2000 — `performance.now()` só tem resolução de 1 ms inteiro.
//!
//! O relógio FUNCIONA (avança, não é stub), mas devolve sempre milissegundos
//! inteiros. Na web ele é sub-milissegundo justamente para permitir medir
//! trabalho curto; com granularidade de 1 ms não dá para perfilar o interior de
//! um frame de 16,7 ms dividido em 4-5 fases.
//!
//! Diferente de `method_vs_free`, estes testes são DETERMINÍSTICOS: não medem
//! tempo de parede, só perguntam ao próprio programa se o valor tem parte
//! fracionária. Não flocam em máquina lenta.

use rts_codegen_new::front::run::render_source;

/// Roda `src` e afirma o stdout exato.
fn assert_stdout(src: &str, expected: &str) {
    match render_source(src) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch para:\n{src}"),
        Err(e) => panic!("render_source falhou para:\n{src}\n  -> {e}"),
    }
}

/// O relógio avança. Este PASSA hoje — está aqui para que, ao corrigir #2000,
/// ninguém quebre o que já funciona (um `now()` fracionário mas parado seria
/// pior que o atual).
#[test]
#[ignore = "suíte de performance; roda com `--ignored` (este passa hoje)"]
fn performance_now_advances() {
    assert_stdout(
        r#"
const a = performance.now();
let acc = 0.0;
let i = 0;
while (i < 3000000) { acc = acc + i * 0.5; i = i + 1; }
const b = performance.now();
console.log(b > a);
"#,
        "true\n",
    );
}

/// O NÚCLEO da #2000: 300 leituras consecutivas não produzem nenhum valor com
/// parte fracionária. Falha hoje (imprime `false`); passa quando `now()` devolver
/// um `f64` de verdade, alimentado por um relógio monotônico de alta resolução.
#[test]
#[ignore = "#2000 aberta: falha de propósito. `cargo test --release --test performance -- --ignored`"]
fn performance_now_has_sub_millisecond_resolution() {
    assert_stdout(
        r#"
let frac = 0;
let i = 0;
while (i < 300) {
  const t = performance.now();
  if (t !== Math.floor(t)) frac = frac + 1;
  i = i + 1;
}
console.log(frac > 0);
"#,
        "true\n",
    );
}

/// A consequência prática: um trecho de ~1 ms tem de ser medível. Hoje o delta é
/// 0 ou 1 — erro de 100%. Este teste pede só que não seja ZERO, o mínimo para
/// instrumentar um frame de dentro do programa.
#[test]
#[ignore = "#2000 aberta: falha de propósito. `cargo test --release --test performance -- --ignored`"]
fn a_sub_millisecond_span_is_measurable() {
    assert_stdout(
        r#"
let zeros = 0;
let r = 0;
while (r < 20) {
  const a = performance.now();
  let acc = 0.0;
  let j = 0;
  while (j < 100000) { acc = acc + j * 0.5; j = j + 1; }
  if (performance.now() - a === 0.0) zeros = zeros + 1;
  r = r + 1;
}
console.log(zeros === 0);
"#,
        "true\n",
    );
}
