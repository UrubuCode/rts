#[test]
fn medir_pagina_css() {
    let css = std::fs::read_to_string("../../pagina.css").unwrap();
    let mut total = 0usize;
    let mut ok = 0usize;
    for bloco in css.split('}') {
        let Some(sel) = bloco.split('{').next() else { continue };
        let sel = sel.trim();
        if sel.is_empty() || sel.starts_with('@') || sel.contains('@') { continue; }
        for parte in sel.split(',') {
            let p = parte.trim();
            if p.is_empty() { continue; }
            total += 1;
            if super::parse_selector(p).is_some() { ok += 1; }
        }
    }
    println!("MEDIDO: {ok} de {total} seletores parseiam");
}
