import { io } from "rts";
import { net } from "rts";

// Mini dashboard HTTP exercitando a maior parte do que o RTS
// suporta hoje: classe com estado, metodos de instancia,
// string concat com numero, JSON.stringify/parse, HTTP server.
//
// Rotas:
//   GET  /                 → HTML com o contador (auto-refresh)
//   GET  /api/state        → JSON { count, lastAction, requestCount }
//   POST /api/increment    → incrementa, retorna JSON
//   POST /api/decrement    → decrementa, retorna JSON
//   POST /api/reset        → zera, retorna JSON
//   GET  /api/health       → { status: "ok", requestCount: N }
//   *                      → 404
//
// Rode:
//   target/release/rts.exe run examples/site/server.ts
// E abra no browser:
//   http://127.0.0.1:3000

class Counter {
  count: number;
  lastAction: string;
  requestCount: number;

  increment(): void {
    this.count = this.count + 1;
    this.lastAction = "increment";
  }

  decrement(): void {
    this.count = this.count - 1;
    this.lastAction = "decrement";
  }

  reset(): void {
    this.count = 0;
    this.lastAction = "reset";
  }

  trackRequest(): void {
    this.requestCount = this.requestCount + 1;
  }

  toJson(): string {
    // Usa JSON.stringify direto via classe — requires que JSON.stringify
    // saiba serializar todos os campos. Numbers inteiros saem sem .0,
    // strings ok, bools ok.
    return JSON.stringify(this);
  }
}

function renderIndex(c: Counter): string {
  // HTML simples com JS inline fazendo fetch das APIs.
  // Usa o namespace str indiretamente via concat.
  const html =
    "<!DOCTYPE html><html><head><title>RTS Counter</title>" +
    "<style>body{font-family:system-ui,sans-serif;max-width:600px;margin:2em auto;padding:1em;}" +
    "h1{color:#333}button{font-size:1.2em;padding:0.5em 1em;margin:0.2em;cursor:pointer;}" +
    ".count{font-size:3em;text-align:center;color:#0066cc;margin:0.5em 0;}" +
    ".meta{color:#888;font-size:0.9em;text-align:center;}" +
    "</style></head><body>" +
    "<h1>RTS HTTP Demo</h1>" +
    "<p>Servidor HTTP/1.1 escrito em TypeScript, compilado para codigo nativo via Cranelift.</p>" +
    "<div class=\"count\" id=\"count\">" + c.count + "</div>" +
    "<div class=\"meta\">last action: <b>" + c.lastAction + "</b> &middot; requests: " + c.requestCount + "</div>" +
    "<div style=\"text-align:center;margin:1em 0;\">" +
    "<button onclick=\"act('increment')\">+1</button>" +
    "<button onclick=\"act('decrement')\">-1</button>" +
    "<button onclick=\"act('reset')\">reset</button>" +
    "</div>" +
    "<div style=\"text-align:center;margin:1em 0;\">" +
    "<button id=\"autobtn\" onclick=\"toggleAuto()\" style=\"font-size:1em;padding:0.5em 1.5em;cursor:pointer;\">Start stress test</button>" +
    "<div id=\"autostats\" style=\"color:#666;margin-top:0.5em;\"></div>" +
    "</div>" +
    "<pre id=\"log\" style=\"background:#f4f4f4;padding:1em;font-size:0.85em;overflow:auto;\"></pre>" +
    "<script>" +
    "let autoId=null;let autoCount=0;" +
    "async function refresh(){const r=await fetch('/api/state');const j=await r.json();" +
    "document.getElementById('count').textContent=j.count;" +
    "document.querySelector('.meta').innerHTML='last action: <b>'+j.lastAction+'</b> &middot; requests: '+j.requestCount;" +
    "document.getElementById('log').textContent=JSON.stringify(j,null,2);}" +
    "async function act(op){await fetch('/api/'+op,{method:'POST'});refresh();}" +
    "function toggleAuto(){const btn=document.getElementById('autobtn');" +
    "if(autoId){clearInterval(autoId);autoId=null;btn.textContent='Start stress test';btn.style.background='';}" +
    "else{autoCount=0;let startT=Date.now();autoId=setInterval(()=>{const ops=['increment','decrement','increment'];" +
    "for(let b=0;b<10;b++){fetch('/api/'+ops[(autoCount+b)%3],{method:'POST'}).then(()=>{" +
    "autoCount++;const elapsed=(Date.now()-startT)/1000;const rps=Math.round(autoCount/elapsed);" +
    "document.getElementById('autostats').textContent='stress: '+autoCount+' reqs | '+rps+' req/s';" +
    "if(autoCount%50===0)refresh();});}" +
    "},1);btn.textContent='Stop stress test';btn.style.background='#ff4444';}}" +
    "refresh();" +
    "</script>" +
    "</body></html>";
  return html;
}

// Simples: responde a uma request e sai. O loop em main() chama de novo.
function handleOne(listener: number, counter: Counter): void {
  const acceptResult = net.tcp_accept(listener);
  if (!acceptResult.ok) {
    io.print("accept failed: " + acceptResult.error);
    return;
  }
  const stream = acceptResult.value.stream;

  const reqResult = net.http_read_request(stream);
  if (!reqResult.ok) {
    io.print("read_request failed: " + reqResult.error);
    net.tcp_shutdown(stream, "Both");
    return;
  }
  const req = reqResult.value;

  counter.trackRequest();

  const method = net.http_request_method(req).value;
  const path = net.http_request_path(req).value;
  io.print(method + " " + path);

  // Roteamento.
  if (path == "/") {
    const html = renderIndex(counter);
    net.http_response_write(stream, 200, html, "text/html; charset=utf-8");
  } else if (path == "/api/state") {
    net.http_response_write(stream, 200, counter.toJson(), "application/json");
  } else if (path == "/api/increment") {
    counter.increment();
    net.http_response_write(stream, 200, counter.toJson(), "application/json");
  } else if (path == "/api/decrement") {
    counter.decrement();
    net.http_response_write(stream, 200, counter.toJson(), "application/json");
  } else if (path == "/api/reset") {
    counter.reset();
    net.http_response_write(stream, 200, counter.toJson(), "application/json");
  } else if (path == "/api/health") {
    const health = "{\"status\":\"ok\",\"requestCount\":" + counter.requestCount + "}";
    net.http_response_write(stream, 200, health, "application/json");
  } else {
    const body = "{\"error\":\"not found\",\"path\":\"" + path + "\"}";
    net.http_response_write(stream, 404, body, "application/json");
  }

  net.http_request_free(req);
  net.tcp_shutdown(stream, "Both");
}

function main(): void {
  const counter = new Counter();
  counter.count = 0;
  counter.lastAction = "init";
  counter.requestCount = 0;

  const listenResult = net.tcp_listen("127.0.0.1:3000");
  if (!listenResult.ok) {
    io.print("failed to listen: " + listenResult.error);
    return;
  }
  const listener = listenResult.value;

  io.print("RTS HTTP Demo running on http://127.0.0.1:3000");
  io.print("press Ctrl+C to stop");
  io.print("");

  while (true) {
    handleOne(listener, counter);
  }
}
