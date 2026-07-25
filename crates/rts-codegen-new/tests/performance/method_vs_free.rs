//! Issue #1999 — laço dentro de MÉTODO é ~5,6x mais lento que em função livre.
//!
//! O mesmo laço, sobre os mesmos dados, custa muito mais dentro de um método de
//! classe do que numa função livre com parâmetro anotado. Anotar o tipo do local
//! dentro do método NÃO ajuda: a informação de tipo existe (o campo é declarado
//! `ps: P[]`), mas não é usada no corpo do método, e cada leitura de campo cai no
//! caminho dinâmico de propriedade.
//!
//! Medido no rts-game (Windows, release), 600 mil leituras de campo:
//!
//! | Onde o laço vive                       | Tempo   |
//! |----------------------------------------|---------|
//! | escopo de topo                         | 0,20 s  |
//! | função livre com parâmetro anotado     | 0,21 s  |
//! | método lendo `this.ps`                 | 1,16 s  |
//! | método com `const ps: P[] = this.ps`   | 1,12 s  |
//!
//! ## Por que RAZÃO e não tempo absoluto
//!
//! Um limite em milissegundos vira flake na primeira máquina lenta. Estes testes
//! medem os DOIS caminhos no mesmo processo, na mesma máquina, e comparam a razão
//! — o que cancela a velocidade da máquina. A margem é folgada de propósito (3x,
//! contra os 5,6x observados): o alvo é a regressão estrutural, não precisão.

use rts_codegen_new::front::run::render_source;
use std::time::Instant;

/// Roda `src` e devolve quanto tempo levou.
fn time_source(src: &str) -> f64 {
    let t0 = Instant::now();
    let r = render_source(src);
    let dt = t0.elapsed().as_secs_f64();
    assert!(
        r.is_ok(),
        "a fonte precisa RODAR para o tempo valer:\n{src}\n  -> {r:?}"
    );
    dt
}

/// Falha se o caminho "lento" passar de `max_ratio` vezes o "rápido". A mensagem
/// carrega os tempos medidos, para quem ler a falha saber o quanto falta.
fn assert_ratio_under(slow_name: &str, slow: f64, fast_name: &str, fast: f64, max_ratio: f64) {
    let ratio = slow / fast;
    assert!(
        ratio < max_ratio,
        "#1999: `{slow_name}` custou {ratio:.1}x o `{fast_name}` (limite {max_ratio:.1}x).\n\
         \x20 {slow_name}: {slow:.3}s\n\
         \x20 {fast_name}: {fast:.3}s\n\
         O mesmo laço sobre os mesmos dados deveria custar o mesmo nos dois."
    );
}

/// Corpo compartilhado: 2000 objetos de 3 campos, somados 300 vezes. `call` é o
/// único trecho que muda entre as variantes.
fn source_with(call: &str, extra_decls: &str) -> String {
    format!(
        r#"
class P {{ x: number; y: number; z: number;
  constructor() {{ this.x = 1.0; this.y = 2.0; this.z = 3.0; }} }}
{extra_decls}
const ps: P[] = [];
let i = 0;
while (i < 2000) {{ ps.push(new P()); i = i + 1; }}
let acc = 0.0;
let r = 0;
while (r < 300) {{ acc = acc + {call}; r = r + 1; }}
console.log(acc);
"#
    )
}

/// A função livre com parâmetro anotado — o caminho RÁPIDO, a referência.
const FREE_FN: &str = r#"
function sumFree(ps: P[]): number {
  let a = 0.0;
  let i = 0;
  while (i < ps.length) { const p: P = ps[i]; a = a + p.x + p.y + p.z; i = i + 1; }
  return a;
}
"#;

/// O método lendo `this.ps` — o caminho LENTO.
const HOLDER_PLAIN: &str = r#"
class Holder {
  ps: P[];
  constructor(ps: P[]) { this.ps = ps; }
  sum(): number {
    let a = 0.0;
    let i = 0;
    while (i < this.ps.length) { const p = this.ps[i]; a = a + p.x + p.y + p.z; i = i + 1; }
    return a;
  }
}
"#;

/// O método que HOISTA para um local anotado antes do laço. Se a anotação
/// bastasse, este seria tão rápido quanto a função livre — não é.
const HOLDER_HOISTED: &str = r#"
class Holder {
  ps: P[];
  constructor(ps: P[]) { this.ps = ps; }
  sum(): number {
    const ps: P[] = this.ps;
    let a = 0.0;
    let i = 0;
    while (i < ps.length) { const p: P = ps[i]; a = a + p.x + p.y + p.z; i = i + 1; }
    return a;
  }
}
"#;

/// O laço no escopo de topo, INLINE — o controle que prova que o custo é do
/// MÉTODO e não do laço em si.
///
/// Cuidado que já me mordeu: envolver este laço numa arrow (`(() => {...})()`)
/// para poder usá-lo como expressão custa 7x sozinho — a arrow é recriada a cada
/// volta das 300. O controle tem de ser inline, ou mede a arrow em vez do laço.
/// Por isso `source_top_level` é um molde separado em vez de um `call`.
fn source_top_level() -> String {
    String::from(
        r#"
class P { x: number; y: number; z: number;
  constructor() { this.x = 1.0; this.y = 2.0; this.z = 3.0; } }
const ps: P[] = [];
let i = 0;
while (i < 2000) { ps.push(new P()); i = i + 1; }
let acc = 0.0;
let r = 0;
while (r < 300) {
  let k = 0;
  while (k < ps.length) { const p: P = ps[k]; acc = acc + p.x + p.y + p.z; k = k + 1; }
  r = r + 1;
}
console.log(acc);
"#,
    )
}

#[test]
#[ignore = "#1999 aberta: falha de propósito. `cargo test --release --test performance -- --ignored`"]
fn method_field_read_is_not_slower_than_free_function() {
    let free = time_source(&source_with("sumFree(ps)", FREE_FN));
    let method = time_source(&source_with(
        "h.sum()",
        &format!("{HOLDER_PLAIN}\nconst h = new Holder(ps);"),
    ));
    assert_ratio_under("método lendo this.ps", method, "função livre", free, 3.0);
}

/// O achado que torna #1999 acionável: anotar o local DENTRO do método não
/// recupera o desempenho. Não é falta de tipo declarado — é o contexto.
#[test]
#[ignore = "#1999 aberta: falha de propósito. `cargo test --release --test performance -- --ignored`"]
fn annotating_the_local_inside_the_method_recovers_speed() {
    let free = time_source(&source_with("sumFree(ps)", FREE_FN));
    let hoisted = time_source(&source_with(
        "h.sum()",
        &format!("{HOLDER_HOISTED}\nconst h = new Holder(ps);"),
    ));
    assert_ratio_under(
        "método com `const ps: P[] = this.ps`",
        hoisted,
        "função livre",
        free,
        3.0,
    );
}

/// Controle: o mesmo laço no escopo de topo. Este deve PASSAR mesmo com #1999
/// aberta — se falhar, o problema não é o método e sim a medição, e os outros
/// dois testes não valem nada.
/// A TERCEIRA forma do mesmo problema: um valor que chega por CAPTURA de closure
/// também perde a prova de tipo, como o que chega por `this.campo`. Só o
/// parâmetro anotado a carrega.
///
/// Medido: por parâmetro 0,52 s, por captura 0,69 s — 33% mais caro com corpo
/// idêntico. Isso sugere que a correção é fazer o tipo sobreviver ao ponto de
/// entrada, não um caso especial para leitura de campo.
#[test]
#[ignore = "#1999 aberta: falha de propósito. `cargo test --release --test performance -- --ignored`"]
fn captured_variable_is_not_slower_than_parameter() {
    let by_param = time_source(
        r#"
class P { x: number; constructor() { this.x = 1.0; } }
const ps: P[] = [];
let i = 0;
while (i < 2000) { ps.push(new P()); i = i + 1; }
function sum(ps: P[]): number {
  let a = 0.0; let k = 0;
  while (k < ps.length) { a = a + ps[k].x; k = k + 1; }
  return a;
}
let acc = 0.0; let r = 0;
while (r < 300) { acc = acc + sum(ps); r = r + 1; }
console.log(acc);
"#,
    );
    let by_capture = time_source(
        r#"
class P { x: number; constructor() { this.x = 1.0; } }
const ps: P[] = [];
let i = 0;
while (i < 2000) { ps.push(new P()); i = i + 1; }
const sum = () => {
  let a = 0.0; let k = 0;
  while (k < ps.length) { a = a + ps[k].x; k = k + 1; }
  return a;
};
let acc = 0.0; let r = 0;
while (r < 300) { acc = acc + sum(); r = r + 1; }
console.log(acc);
"#,
    );
    // margem menor: o efeito medido aqui é de 1,33x, não de 5x
    assert_ratio_under(
        "valor por captura de closure",
        by_capture,
        "valor por parâmetro anotado",
        by_param,
        1.25,
    );
}

/// Controle: o mesmo laço no escopo de topo. Este deve PASSAR mesmo com #1999
/// aberta — se falhar, o problema não é o método e sim a medição, e os outros
/// testes não valem nada.
#[test]
#[ignore = "suíte de performance; roda com `--ignored` (este passa mesmo com #1999 aberta)"]
fn top_level_loop_matches_free_function() {
    let free = time_source(&source_with("sumFree(ps)", FREE_FN));
    let top = time_source(&source_top_level());
    assert_ratio_under("laço no escopo de topo", top, "função livre", free, 3.0);
}
