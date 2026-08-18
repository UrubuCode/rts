// Serve `tests/css/` por HTTP, só para o Chrome poder medir as fixtures dentro
// de um iframe de MESMA ORIGEM.
//
//   bun scripts/css_fixtures_serve.ts        # porta 8731
//
// Por que um servidor e não `file://`: duas páginas `file://` são origens
// opacas distintas para o Chrome, então o harness não conseguiria ler o
// `contentDocument` do iframe. A alternativa era navegar o separador uma vez
// por fixture — 34 navegações e 34 avaliações em vez de uma — e cada navegação
// é um sítio a mais onde uma medição pode falhar em silêncio.
import { serve } from "bun";
import { readFileSync, readdirSync } from "node:fs";

const RAIZ = "tests/css";
const PORTA = 8731;

serve({
  port: PORTA,
  fetch(req) {
    const caminho = new URL(req.url).pathname;
    if (caminho === "/lista") {
      const nomes = readdirSync(RAIZ).filter((n) => n.endsWith(".html")).sort();
      return Response.json(nomes);
    }
    try {
      const corpo = readFileSync(RAIZ + caminho);
      return new Response(corpo, { headers: { "content-type": "text/html; charset=utf-8" } });
    } catch {
      return new Response("não há", { status: 404 });
    }
  },
});

console.log("a servir " + RAIZ + " em http://127.0.0.1:" + PORTA);
