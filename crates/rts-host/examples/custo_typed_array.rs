//! Quanto custa um acesso a typed array no motor novo.
//!
//! Medido em 2026-08-09, release: **~1,7 µs por acesso**, contra 10 ns pelo
//! `rts:buffer` do motor antigo — 170×.
//!
//! O laço vazio custa 0,9 ns/volta, então não é o laço; e o índice CONSTANTE
//! (`a[0]`) custa o mesmo que o variável, o que descarta o cálculo de índice e
//! aponta para o acesso cair no caminho dinâmico. É a mesma forma do gap que
//! `funcval::module_global_shapes` corrigiu para arrays de módulo (260 → 20 ns):
//! sem forma provada, todo acesso vira busca genérica com coerção.
//!
//! Isto é um bloqueio para portar um jogo: o `rts-game` faz dezenas de milhares
//! de acessos por frame.
//!
//! ```text
//! cargo run --release -p rts-host --example custo_typed_array
//! ```

fn medir(nome: &str, corpo: &str) {
    let src = format!(r#"
const N = 100000;
const a = new Float32Array(4096);
let aquece = 0;
while (aquece < 1000) {{ a[aquece % 4096] = 1.0; aquece = aquece + 1; }}
const t0 = performance.now();
let s = 0.0;
let j = 0;
while (j < N) {{ {corpo} j = j + 1; }}
const t1 = performance.now();
return (t1 - t0) * 1000000.0 / N + s * 0.0;
"#);
    match rts_host::compile(&src) {
        Ok(mut p) => println!("{nome:<34} {:.1} ns/volta", rts_cranelift::tags::decode_double(p.run())),
        Err(e) => println!("{nome:<34} RECUSADO: {e:?}"),
    }
}

fn main() {
    medir("laco vazio", "s = s + 1.0;");
    medir("so o modulo (j % 4096)", "s = s + (j % 4096) * 1.0;");
    medir("indice CONSTANTE a[0]", "a[0] = 1.0; s = s + a[0];");
    medir("indice variavel a[j % 4096]", "a[j % 4096] = 1.0; s = s + a[j % 4096];");
    medir("so escrita a[0]", "a[0] = 1.0;");
    medir("so leitura a[0]", "s = s + a[0];");
}
