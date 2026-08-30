// Monta o `preact-app.html` que o `claude-preact-janela.ts` abre.
//
// Existe separado porque a pagina e ENTRADA de uma medicao e nao fonte: fica
// ignorada pelo git ao lado do `react-*.html`, e este ficheiro e o que a
// reproduz. Vai buscar o Preact 10.24.3 ao CDN com o nosso proprio `fetch`, o
// que tambem faz dele um teste da pilha de rede — HTTPS, SNI, de-chunking.
import { writeFileSync } from "node:fs";

async function buscar(url: string): Promise<string> {
  const r: any = await fetch(url);
  const t = await r.text();
  console.log(url.substring(url.lastIndexOf("/") + 1) + ": " + r.status + " " + t.length + " bytes");
  return t;
}

const preact = await buscar("https://cdnjs.cloudflare.com/ajax/libs/preact/10.24.3/preact.umd.js");
const hooks = await buscar("https://cdnjs.cloudflare.com/ajax/libs/preact/10.24.3/hooks.umd.js");

const app = "var h = preact.h;\n"
  + "var useState = preactHooks.useState;\n"
  + "function App() {\n"
  + "  var s = useState(0); var n = s[0], setN = s[1];\n"
  + "  var t = useState(['ler o HTML','montar o DOM','reconciliar']);\n"
  + "  var tarefas = t[0], setTarefas = t[1];\n"
  + "  return h('div', { style: 'font-family:sans-serif;padding:24px' },\n"
  + "    h('h1', { style: 'color:#673ab8' }, 'Preact a correr no RTS'),\n"
  + "    h('p', null, tarefas.length + ' tarefas, ' + n + ' cliques'),\n"
  + "    h('ul', null, tarefas.map(function (x, i) {\n"
  + "      return h('li', { style: 'padding:6px 0;cursor:pointer',\n"
  + "        onClick: function () {\n"
  + "          setTarefas(tarefas.filter(function (_, j) { return j !== i; }));\n"
  + "          setN(n + 1);\n"
  + "        } }, x + '  (clica para remover)');\n"
  + "    })),\n"
  + "    h('button', { onClick: function () { setN(n + 1); } }, 'so contar: ' + n)\n"
  + "  );\n"
  + "}\n"
  + "preact.render(h(App), document.getElementById('root'));\n";

// Cada script vai como data-URI base64, que e como o `react-app.html` os carrega.
// Re-embutir JS decodificado dentro do HTML corrompe-o no primeiro `</script>`
// que aparecer numa string do bundle — ja custou uma sessao a descobrir.
function embutir(fonte: string): string {
  return "<script src=\"data:application/x-javascript;base64," + btoa(fonte) + "\"></script>";
}

const html = "<html><head><style>body{background:#fff;color:#222}</style></head><body>"
  + "<div id='root'></div>"
  + embutir(preact) + embutir(hooks) + embutir(app)
  + "</body></html>";
writeFileSync("preact-app.html", html);
console.log("preact-app.html: " + html.length + " bytes");
