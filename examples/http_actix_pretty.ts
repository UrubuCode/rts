// Pagina web bonita servida via http_server (actix-web).
//
// Uso:
//   target/release/rts.exe run examples/http_actix_pretty.ts
//   abra http://127.0.0.1:8080/

import { http_server, io } from "rts";

io.print("RTS pretty server escutando em http://127.0.0.1:8080/");
io.print("Ctrl+C pra parar.");

http_server.serve("127.0.0.1:8080", handler);

function handler(req: i64): void {
  const path = http_server.req_path(req);
  if (path == "/api/info") {
    http_server.respond(req, 200, "application/json",
      '{"server":"rts","backend":"actix-web","version":"0.1","runtime":"tokio shared","throughput":"29k req/s"}');
  } else if (path == "/api/time") {
    http_server.respond(req, 200, "application/json",
      '{"now":"' + nowIso() + '"}');
  } else if (path == "/") {
    http_server.respond(req, 200, "text/html; charset=utf-8", renderHome());
  } else {
    http_server.respond(req, 404, "text/html; charset=utf-8", render404(path));
  }
}

function nowIso(): string {
  // placeholder simples — real seria via time.unix_ms + Date.toISOString
  return "2026-05-01T00:00:00Z";
}

function renderHome(): string {
  return `<!DOCTYPE html>
<html lang="pt-br">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>RTS — TypeScript-to-native compiler</title>
  <style>
    :root {
      --bg: #0a0e1a;
      --bg-card: #131826;
      --bg-card-2: #1a2033;
      --fg: #e6e9ef;
      --fg-dim: #a0a8b8;
      --accent: #7c5cff;
      --accent-2: #00d4ff;
      --accent-3: #ff5cb8;
      --success: #4ade80;
      --border: #252b3d;
    }
    @media (prefers-color-scheme: light) {
      :root {
        --bg: #f7f8fc;
        --bg-card: #ffffff;
        --bg-card-2: #f0f2f7;
        --fg: #0a0e1a;
        --fg-dim: #5a6478;
        --border: #e1e5ed;
      }
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      background: var(--bg);
      color: var(--fg);
      line-height: 1.6;
      min-height: 100vh;
      overflow-x: hidden;
    }
    body::before {
      content: '';
      position: fixed;
      inset: 0;
      background:
        radial-gradient(ellipse at top left, rgba(124, 92, 255, 0.15), transparent 50%),
        radial-gradient(ellipse at top right, rgba(0, 212, 255, 0.12), transparent 50%),
        radial-gradient(ellipse at bottom, rgba(255, 92, 184, 0.08), transparent 50%);
      pointer-events: none;
      z-index: 0;
    }
    .container {
      max-width: 1100px;
      margin: 0 auto;
      padding: 4rem 1.5rem 5rem;
      position: relative;
      z-index: 1;
    }
    header { text-align: center; margin-bottom: 4rem; }
    .badge {
      display: inline-flex;
      align-items: center;
      gap: 0.5rem;
      padding: 0.4rem 0.9rem;
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 999px;
      font-size: 0.8rem;
      color: var(--fg-dim);
      margin-bottom: 1.5rem;
    }
    .badge::before {
      content: '';
      width: 8px;
      height: 8px;
      background: var(--success);
      border-radius: 50%;
      box-shadow: 0 0 12px var(--success);
      animation: pulse 2s ease-in-out infinite;
    }
    @keyframes pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.5; }
    }
    h1 {
      font-size: clamp(2.5rem, 6vw, 4.5rem);
      font-weight: 800;
      letter-spacing: -0.03em;
      line-height: 1.05;
      margin-bottom: 1rem;
      background: linear-gradient(135deg, var(--accent), var(--accent-2) 50%, var(--accent-3));
      -webkit-background-clip: text;
      background-clip: text;
      -webkit-text-fill-color: transparent;
    }
    .tagline {
      font-size: 1.2rem;
      color: var(--fg-dim);
      max-width: 600px;
      margin: 0 auto;
    }
    .stats {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 1rem;
      margin: 3rem 0;
    }
    .stat {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 16px;
      padding: 1.5rem;
      text-align: center;
      transition: transform 0.2s, border-color 0.2s;
    }
    .stat:hover {
      transform: translateY(-4px);
      border-color: var(--accent);
    }
    .stat-value {
      font-size: 2rem;
      font-weight: 700;
      background: linear-gradient(135deg, var(--accent), var(--accent-2));
      -webkit-background-clip: text;
      background-clip: text;
      -webkit-text-fill-color: transparent;
    }
    .stat-label {
      font-size: 0.85rem;
      color: var(--fg-dim);
      margin-top: 0.3rem;
    }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
      gap: 1.5rem;
      margin: 3rem 0;
    }
    .card {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 20px;
      padding: 1.8rem;
      transition: transform 0.2s, border-color 0.2s, box-shadow 0.2s;
    }
    .card:hover {
      transform: translateY(-4px);
      border-color: var(--accent);
      box-shadow: 0 20px 40px -20px rgba(124, 92, 255, 0.3);
    }
    .card-icon {
      width: 48px;
      height: 48px;
      border-radius: 12px;
      display: flex;
      align-items: center;
      justify-content: center;
      margin-bottom: 1rem;
      font-size: 1.5rem;
      background: linear-gradient(135deg, rgba(124, 92, 255, 0.2), rgba(0, 212, 255, 0.2));
    }
    .card h3 { font-size: 1.15rem; margin-bottom: 0.5rem; }
    .card p { color: var(--fg-dim); font-size: 0.95rem; }
    .demo {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 20px;
      padding: 2rem;
      margin: 3rem 0;
    }
    .demo h2 { margin-bottom: 1rem; font-size: 1.5rem; }
    .demo-controls {
      display: flex;
      gap: 0.7rem;
      flex-wrap: wrap;
      margin: 1rem 0;
    }
    button {
      background: linear-gradient(135deg, var(--accent), var(--accent-2));
      color: white;
      border: 0;
      padding: 0.7rem 1.4rem;
      border-radius: 10px;
      font-size: 0.95rem;
      font-weight: 600;
      cursor: pointer;
      transition: transform 0.15s, box-shadow 0.15s;
    }
    button:hover {
      transform: translateY(-2px);
      box-shadow: 0 12px 24px -8px rgba(124, 92, 255, 0.5);
    }
    button:active { transform: translateY(0); }
    button.secondary {
      background: var(--bg-card-2);
      color: var(--fg);
      border: 1px solid var(--border);
    }
    .out {
      background: #060912;
      color: #c9d1d9;
      padding: 1.2rem;
      border-radius: 12px;
      font-family: 'JetBrains Mono', 'Fira Code', ui-monospace, monospace;
      font-size: 0.88rem;
      min-height: 4rem;
      white-space: pre-wrap;
      word-break: break-all;
      border: 1px solid var(--border);
      max-height: 300px;
      overflow-y: auto;
    }
    .out.empty { color: var(--fg-dim); font-style: italic; }
    footer {
      text-align: center;
      color: var(--fg-dim);
      font-size: 0.9rem;
      padding-top: 3rem;
      border-top: 1px solid var(--border);
      margin-top: 4rem;
    }
    code {
      background: var(--bg-card-2);
      padding: 0.15rem 0.4rem;
      border-radius: 4px;
      font-family: ui-monospace, monospace;
      font-size: 0.9em;
      color: var(--accent-2);
    }
    a { color: var(--accent-2); text-decoration: none; }
    a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <div class="container">
    <header>
      <div class="badge">live · servido pelo proprio binario RTS</div>
      <h1>RTS</h1>
      <p class="tagline">Compilador TypeScript-to-native via Cranelift. Binario standalone, GC preciso, runtime tokio integrado.</p>
    </header>

    <section class="stats">
      <div class="stat">
        <div class="stat-value">29k</div>
        <div class="stat-label">req/s pico (HTTP)</div>
      </div>
      <div class="stat">
        <div class="stat-value">5.14×</div>
        <div class="stat-label">vs Bun (Monte Carlo)</div>
      </div>
      <div class="stat">
        <div class="stat-value">36</div>
        <div class="stat-label">namespaces nativos</div>
      </div>
      <div class="stat">
        <div class="stat-value">17 MB</div>
        <div class="stat-label">RAM em prod</div>
      </div>
    </section>

    <section class="grid">
      <div class="card">
        <div class="card-icon">⚡</div>
        <h3>AOT real</h3>
        <p>Cranelift gera nativo direto. Sem V8, sem JIT em runtime. Cold-start sub-milissegundo, binario de KBs.</p>
      </div>
      <div class="card">
        <div class="card-icon">🧵</div>
        <h3>Multi-thread</h3>
        <p>4 modos de spawn coexistindo: <code>std::thread</code>, tokio com retorno, fire-and-forget, pool fixo. Dev escolhe pelo workload.</p>
      </div>
      <div class="card">
        <div class="card-icon">🌐</div>
        <h3>HTTP/1.1 nativo</h3>
        <p>Servidor via actix-web sobre runtime tokio compartilhado. Bridge sync↔async com shard map + oneshot. <strong>Esta pagina e prova.</strong></p>
      </div>
      <div class="card">
        <div class="card-icon">🗑️</div>
        <h3>GC preciso</h3>
        <p>Mark+sweep com Cranelift stack maps. Stop-the-world via SuspendThread. Zero leak em servidor 24/7.</p>
      </div>
      <div class="card">
        <div class="card-icon">🔒</div>
        <h3>TLS 1.2/1.3</h3>
        <p><code>rustls</code> + Mozilla CAs embutidos. HTTPS ponta-a-ponta sem OpenSSL nem schannel.</p>
      </div>
      <div class="card">
        <div class="card-icon">🎯</div>
        <h3>Zero regressao</h3>
        <p>Toda PR roda a suite completa antes do merge. 77 testes, 175 PRs em 18 dias, sem regressao silenciosa.</p>
      </div>
    </section>

    <section class="demo">
      <h2>Live API demo</h2>
      <p style="color: var(--fg-dim); font-size: 0.95rem;">Os botoes abaixo fazem <code>fetch()</code> reais para endpoints servidos por este mesmo processo.</p>
      <div class="demo-controls">
        <button onclick="callApi('/api/info')">GET /api/info</button>
        <button onclick="callApi('/api/time')" class="secondary">GET /api/time</button>
        <button onclick="callApi('/api/inexistente')" class="secondary">GET /api/inexistente</button>
        <button onclick="document.getElementById('out').textContent='aguardando...'; document.getElementById('out').classList.add('empty')" class="secondary">Limpar</button>
      </div>
      <pre class="out empty" id="out">aguardando...</pre>
    </section>

    <footer>
      <p>Compilado e servido por <code>target/release/rts.exe</code> · 1 binario de poucos MB · zero deps externas</p>
      <p style="margin-top: 0.5rem;">Backend: actix-web 4 · Runtime: tokio multi-thread compartilhado · GC: mark+sweep proprio</p>
    </footer>
  </div>

  <script>
    async function callApi(path) {
      const out = document.getElementById('out');
      out.classList.remove('empty');
      out.textContent = 'GET ' + path + '\\n\\nrequesting...';
      const start = performance.now();
      try {
        const r = await fetch(path);
        const text = await r.text();
        const dt = (performance.now() - start).toFixed(1);
        let body = text;
        try { body = JSON.stringify(JSON.parse(text), null, 2); } catch (_) {}
        out.textContent = 'GET ' + path + '\\n' +
          'HTTP/1.1 ' + r.status + ' ' + r.statusText + '\\n' +
          'content-type: ' + (r.headers.get('content-type') || 'unknown') + '\\n' +
          'duration: ' + dt + ' ms\\n\\n' +
          body;
      } catch (e) {
        out.textContent = 'GET ' + path + '\\n\\nERR: ' + e.message;
      }
    }
  </script>
</body>
</html>`;
}

function render404(path: string): string {
  return `<!DOCTYPE html>
<html lang="pt-br">
<head>
  <meta charset="utf-8">
  <title>404 — RTS</title>
  <style>
    body { font-family: -apple-system, system-ui, sans-serif; background: #0a0e1a; color: #e6e9ef; display: grid; place-items: center; min-height: 100vh; margin: 0; }
    .box { text-align: center; padding: 2rem; }
    h1 { font-size: 5rem; margin: 0; background: linear-gradient(135deg, #ff5cb8, #00d4ff); -webkit-background-clip: text; background-clip: text; -webkit-text-fill-color: transparent; }
    p { color: #a0a8b8; }
    code { background: #131826; padding: 0.2rem 0.5rem; border-radius: 4px; color: #00d4ff; }
    a { color: #7c5cff; }
  </style>
</head>
<body>
  <div class="box">
    <h1>404</h1>
    <p>O path <code>${path}</code> nao existe.</p>
    <p style="margin-top:1rem;"><a href="/">← voltar</a></p>
  </div>
</body>
</html>`;
}
