import egui from "rts:egui";
import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";

// app.ts — janela RTS AOT do comparativo rts-vs-electron. Irmã de
// examples/view.ts (que lê o caminho do HTML por process.argv), mas o .exe
// compilado precisa ser STANDALONE numa pasta — igual ao instalador do
// Electron, que também abre sempre o app.asar ao lado de si mesmo — então o
// caminho do HTML é derivado do próprio executável (process.execPath), não de
// um argumento que o utilizador teria de lembrar de passar. O fallback por
// process.argv fica só para permitir testar via `rts run app.ts <html>` sem
// precisar copiar app.html para o lado do binário do motor de desenvolvimento.
function findHtmlPath(): string {
  const exeDir = dirname(process.execPath);
  const beside = join(exeDir, "app.html");
  if (existsSync(beside)) return beside;
  for (const a of process.argv) {
    if (a.length > 5 && a.substring(a.length - 5) === ".html") return a;
  }
  return beside;
}

const path = findHtmlPath();
const html = readFileSync(path, "utf8") as string;
if (html === undefined || html.length === 0) {
  console.log("nao consegui ler: " + path);
} else {
  console.log("HTML: " + html.length + " bytes");
  const doc = parseDocument(html);
  const loaded = loadResources(doc, path);
  console.log("recursos externos carregados: " + loaded);
  // `runScriptsAt` compila e corre cada <script> da página EM RUNTIME (new
  // Function -> pipeline swc->HIR->JIT, DomScope.run em
  // crates/rts-dom-bridge/src/scope.rs) — e o binário AOT não leva esse
  // compilador consigo (`rts compile` só gera código nativo para o que o
  // PRÓPRIO app.ts referencia estaticamente, não para texto que só aparece
  // como conteúdo de um <script> lido de um HTML). Chamamos na mesma, e não
  // saltamos o <script> da página, por duas razões: o `.exe` fica igual ao
  // lado JIT (examples/claude-react-janela.ts, que chama a mesma função) em
  // vez de divergir por omissão, e o erro real aparece no stderr medido —
  // hoje cada <script> falha com "a fonte não compilou" e a janela fica em
  // branco para ESTA app (o HTML/CSS estático continua a pintar, ver
  // README.md deste diretório).
  console.log("scripts da pagina corridos: " + runScriptsAt(doc, "https://localhost/"));
  const win = egui.openWindow("RTS vs Electron", 1100, 750, 0);
  while (egui.isOpen(win)) {
    if (!egui.pump(win)) break;
    egui.beginFrame(win);
    egui.render(win, doc._dom);
    egui.endFrame(win);
  }
  egui.close(win);
}
