import { describe, test, expect } from "rts:test";

// `window` global de browser injetado nos <script> da página (location/navigator/
// history/localStorage/aliases). Pré-computado no top-level.

const html = "<div id='out'>x</div><script>"
  + "const parts = [];"
  + "parts.push('href:' + location.href);"
  + "parts.push('host:' + location.hostname);"
  + "parts.push('port:' + location.port);"
  + "parts.push('path:' + location.pathname);"
  + "parts.push('proto:' + location.protocol);"
  + "parts.push('lang:' + navigator.language);"
  + "parts.push('w:' + window.innerWidth);"
  + "parts.push('h:' + window.innerHeight);"
  + "localStorage.setItem('chave', 'valor');"
  + "parts.push('ls:' + localStorage.getItem('chave'));"
  + "parts.push('self:' + (self === window));"
  + "parts.push('doc:' + (window.document !== null));"
  + "const o = document.getElementById('out');"
  + "if (o !== null) { o.setInnerHTML(parts.join('|')); }"
  + "</script>";

const doc = parseDocument(html);
const ran = runScriptsAt(doc, "https://exemplo.com:8080/pag?q=1#top");
const o = doc.getElementById("out");
const result = o === null ? "" : o.textContent;

// checa presença de "chave:valor" no result (mais robusto que split no subset).
function has(s: string, kv: string): boolean {
  return s.indexOf(kv) >= 0;
}

describe("window global de browser nos <script>", () => {
  test("script com window roda", () => {
    expect(ran).toBe(1);
  });
  test("location parseia a URL da página", () => {
    expect(has(result, "href:https://exemplo.com:8080/pag?q=1#top")).toBe(true);
    expect(has(result, "host:exemplo.com")).toBe(true);
    expect(has(result, "port:8080")).toBe(true);
    expect(has(result, "path:/pag")).toBe(true);
    expect(has(result, "proto:https:")).toBe(true);
  });
  test("navigator + dimensões", () => {
    expect(has(result, "lang:pt-BR")).toBe(true);
    expect(has(result, "w:1000")).toBe(true);
    expect(has(result, "h:800")).toBe(true);
  });
  test("localStorage guarda e lê", () => {
    expect(has(result, "ls:valor")).toBe(true);
  });
  test("aliases self/window/document", () => {
    expect(has(result, "self:true")).toBe(true);
    expect(has(result, "doc:true")).toBe(true);
  });
});
