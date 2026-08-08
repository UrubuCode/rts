//! Compute WGSL pelo motor novo: soma 1024 elementos na GPU e confere o
//! resultado no programa.
//!
//! ```text
//! cargo run -p rts-host-rwk --example gpu
//! ```
//!
//! Exemplo e não teste pela razão que `examples/janela.rs` documenta e que a
//! medição confirmou aqui também: criar o device wgpu numa thread secundária —
//! que é onde um `#[test]` do cargo roda — não retorna. O `tests/ui_surface.rs`
//! cobre o que é verificável sem device (os nomes existem e são chamáveis).

fn main() {
    let source = r#"
import { available, shader, buffer, write, bindBuffer, dispatch, read,
         bufferFree, adapterName } from "rts:gpu";

if (!available()) {
    println("sem GPU utilizável — nada a fazer");
} else {
    println("adapter: " + adapterName());

    // Um kernel que dobra cada u32 do buffer.
    const pipe = shader(`
        @group(0) @binding(0) var<storage, read_write> dados: array<u32>;
        @compute @workgroup_size(64)
        fn main(@builtin(global_invocation_id) id: vec3<u32>) {
            dados[id.x] = dados[id.x] * 2u;
        }
    `);
    if (pipe === 0) {
        println("o kernel não compilou");
    } else {
        const n = 1024;
        const entrada = new Uint32Array(n);
        let i = 0;
        while (i < n) { entrada[i] = i; i = i + 1; }

        const gbuf = buffer(n * 4);
        write(gbuf, entrada);
        bindBuffer(pipe, 0, gbuf);
        dispatch(pipe, n / 64, 1, 1);

        const saida = new Uint32Array(read(gbuf, n * 4).buffer);
        let erros = 0;
        let j = 0;
        while (j < n) {
            if (saida[j] !== j * 2) { erros = erros + 1; }
            j = j + 1;
        }
        println("elementos: " + n + "  divergências: " + erros);
        println(erros === 0 ? "a GPU computou o que o kernel diz" : "DIVERGIU");
        bufferFree(gbuf);
    }
}
"#;
    let mut program = match rts_host_rwk::compile(source) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("o programa não compilou: {error:?}");
            std::process::exit(1);
        }
    };
    program.run();
}
