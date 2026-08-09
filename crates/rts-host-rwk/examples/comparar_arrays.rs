//! Custo de um acesso indexado no motor novo, por forma de receptor.
//!
//! O `s` é declarado ANTES do aquecimento de propósito: na primeira versão deste
//! arquivo ele era declarado depois, o aquecimento não compilava, e as medições
//! saíram inconsistentes entre execuções (1682 ns contra 318 ns para a mesma
//! escrita). Um instrumento que não compila o que mede não mede nada.
fn medir(nome: &str, prep: &str, corpo: &str) {
    let src = format!(r#"
const N = 100000;
{prep}
let s = 0.0;
let aquece = 0;
while (aquece < 2000) {{ {corpo} aquece = aquece + 1; }}
const t0 = performance.now();
let j = 0;
while (j < N) {{ {corpo} j = j + 1; }}
const t1 = performance.now();
return (t1 - t0) * 1000000.0 / N + s * 0.0;
"#);
    match rts_host_rwk::compile(&src) {
        Ok(mut p) => println!("{nome:<34} {:>8.1} ns", rts_cranelift::tags::decode_double(p.run())),
        Err(e) => println!("{nome:<34} RECUSADO: {e:?}"),
    }
}

fn main() {
    medir("laco vazio (a referencia)", "", "s = s + 1.0;");
    medir("objeto      o.x  leitura", "const o = { x: 1.0 };", "s = s + o.x;");
    medir("array comum a[0] leitura", "const a = [1.0, 2.0];", "s = s + a[0];");
    medir("array comum a[0] escrita", "const a = [1.0, 2.0];", "a[0] = 1.0;");
    medir("Float32Array a[0] leitura", "const a = new Float32Array(4);", "s = s + a[0];");
    medir("Float32Array a[0] escrita", "const a = new Float32Array(4);", "a[0] = 1.0;");
}
