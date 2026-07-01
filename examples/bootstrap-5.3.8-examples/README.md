# Bootstrap 5.3.8 — exemplo "cover" (subconjunto)

Material de terceiros sob licença MIT, extraído do pacote oficial de exemplos do
Bootstrap 5.3.8 (<https://getbootstrap.com/docs/5.3/examples/>). Mantemos aqui
**apenas o subconjunto usado pelos exemplos do motor de render** (`cover/` +
`assets/dist/css/bootstrap.min.css`, ~250KB) — o pacote completo (~4MB, 40+
exemplos) é re-baixável do site oficial quando precisar de mais páginas de teste.

Uso: `examples/claude-bootstrap/render_cover.ts` carrega `cover/index.html` via
`rts:dom` (`parseDocument` + `loadResources`) e renderiza na janela egui. A
referência visual do Chrome está em `examples/claude-bootstrap/cover_chrome.png`.
