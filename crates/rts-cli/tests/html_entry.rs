//! Pins the shell `rts compile pagina.html`/`rts run pagina.html` write, from
//! the checked-in fixture `tests/aot/claude-pagina-entrada.html` (a `<title>`
//! and one `<script>` writing into a `<div>`) — WITHOUT linking or running
//! anything, which is the honest half of this batch's own scope: the
//! coordinator's closing pass links and runs the same fixture end to end
//! (`rts compile`, click response) on a single build. This file only pins the
//! generated TEXT: that the HTML crosses escaped, that the title crosses, and
//! that the `app.ts` frame loop is there.

use std::path::{Path, PathBuf};

use rts_cli::cli::html_entry;

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/aot/claude-pagina-entrada.html")
}

fn fixture_html() -> String {
    let path = fixture_path();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

#[test]
fn is_html_matches_the_extension_case_insensitively() {
    assert!(html_entry::is_html(Path::new("pagina.html")));
    assert!(html_entry::is_html(Path::new("PAGINA.HTML")));
    assert!(html_entry::is_html(Path::new("pagina.htm")));
    assert!(!html_entry::is_html(Path::new("pagina.ts")));
    assert!(!html_entry::is_html(Path::new("pagina")));
}

#[test]
fn compile_shell_embeds_the_html_escaped_the_title_and_the_frame_loop() {
    let html = fixture_html();
    let entry = fixture_path();
    let program = html_entry::for_compile(&entry, &html).expect("gera a casca de compile");

    // O HTML entra como literal JSON-escapado: a mesma string que
    // `serde_json::to_string` produziria, colada inteira no texto gerado —
    // não uma leitura em runtime.
    let escaped = serde_json::to_string(&html).expect("html serializa como string JSON");
    assert!(
        program.contains(&escaped),
        "HTML embutido nao esta json-escaped no programa gerado:\n{program}"
    );
    assert!(!program.contains("readFileSync"), "compile NUNCA le o HTML do disco em runtime:\n{program}");

    // O <title> da própria página, não o nome do ficheiro — a fixture tem um.
    assert!(
        program.contains("\"Pagina de Entrada\""),
        "titulo da pagina nao apareceu no programa gerado:\n{program}"
    );

    // O laco de app.ts: abrir janela, renderizar por frame, bombear
    // input/eventos/timers, e o texto exato do print da regua.
    for needle in [
        "egui.openWindow",
        "egui.isOpen",
        "egui.pump",
        "egui.beginFrame",
        "egui.render(win, doc._dom)",
        "egui.endFrame",
        "pumpInputEvents(doc)",
        "pumpEventCallbacks(doc)",
        "pumpTimerCallbacks(doc)",
        "loadDocumentFrom(html, scriptUrl, resourceBase)",
        "pagina carregada: ",
    ] {
        assert!(program.contains(needle), "faltou `{needle}` no programa gerado:\n{program}");
    }
}

#[test]
fn compile_shell_has_no_relative_import_so_it_never_compiles_as_a_graph() {
    let html = fixture_html();
    let entry = fixture_path();
    let program = html_entry::for_compile(&entry, &html).expect("gera a casca de compile");

    // `rts_cli::cli::new_engine::imports_a_file` decide grafo por estas
    // mesmas substrings — o programa gerado não deve acionar nenhuma, senão
    // um `.html` de entrada passaria a compilar como grafo por engano.
    for needle in ["from \"./", "from \"../", "require(", "module.exports", "import.meta"] {
        assert!(!program.contains(needle), "casca de compile parece um grafo (`{needle}`):\n{program}");
    }
}

#[test]
fn run_shell_reads_the_page_from_disk_instead_of_embedding_it() {
    let html = fixture_html();
    let entry = fixture_path();
    let program = html_entry::for_run(&html, &entry);

    assert!(program.contains("readFileSync"), "run deve ler o HTML do disco:\n{program}");
    let path_literal =
        serde_json::to_string(&entry.to_string_lossy()).expect("caminho serializa como string JSON");
    assert!(program.contains(&path_literal), "caminho da pagina nao apareceu no programa gerado:\n{program}");

    // O HTML NAO entra como literal aqui — só o caminho para o ler em runtime.
    let raw_html_literal = serde_json::to_string(&html).expect("html serializa como string JSON");
    assert!(!program.contains(&raw_html_literal), "run nao deveria embutir o HTML:\n{program}");

    assert!(program.contains("\"Pagina de Entrada\""), "titulo da pagina nao apareceu:\n{program}");
    assert!(program.contains("egui.render(win, doc._dom)"), "laco de frame nao apareceu:\n{program}");
}

#[test]
fn window_title_falls_back_to_the_file_stem_when_the_page_has_no_title() {
    let html = "<html><body>sem titulo</body></html>";
    let entry = Path::new("some/dir/minha-pagina.html");
    let program = html_entry::for_compile(entry, html).expect("gera a casca de compile");
    assert!(
        program.contains("\"minha-pagina\""),
        "titulo deveria cair para o nome do ficheiro:\n{program}"
    );
}
