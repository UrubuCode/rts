// Chat com long-polling via http_server (actix-web).
//
// Usa string global como buffer ("id\tuser\ttext\n" linhas concatenadas)
// + mutex_lock/unlock para serializar acesso entre threads tokio do
// actix. Cada novo append faz `buffer = buffer + line` no escopo do
// mutex.
//
// Uso:
//   target/release/rts.exe run examples/http_chat.ts
//   abra http://127.0.0.1:8080/ em 2+ abas

import { http_server, io, sync, string } from "rts";

// Estado global. seq deriva de buffer (conta '\n') sob mutex —
// evita race entre threads tokio do actix.
const lock = sync.mutex_new(0);
let buffer = "";
let seq: i64 = 0;

io.print("RTS chat escutando em http://127.0.0.1:8080/");
io.print("Ctrl+C pra parar.");

http_server.serve("127.0.0.1:8080", handler);

function handler(req: i64): void {
  const path = http_server.req_path(req);
  const method = http_server.req_method(req);

  if (path == "/") {
    http_server.respond(req, 200, "text/html; charset=utf-8", renderHome());
    return;
  }

  if (path == "/api/messages") {
    sync.mutex_lock(lock);
    const snap = buffer;
    sync.mutex_unlock(lock);
    http_server.respond(req, 200, "text/plain; charset=utf-8", snap);
    return;
  }

  // /api/poll/<since> — since como parte do path
  // (req.path() do actix nao inclui query string).
  if (string.starts_with(path, "/api/poll/")) {
    handlePoll(req, path);
    return;
  }
  if (path == "/api/poll") {
    handlePoll(req, "/api/poll/0");
    return;
  }

  if (path == "/api/send" && method == "POST") {
    const body = http_server.req_body(req);
    addMessage(body);
    http_server.respond(req, 200, "text/plain", "ok");
    return;
  }

  http_server.respond(req, 404, "text/plain", "not found");
}

function addMessage(body: string): void {
  // body = "user|text"
  const sep = string.find(body, "|");
  let user = "anon";
  let text = body;
  if (sep > 0) {
    user = body.slice(0, sep);
    text = body.slice(sep + 1);
  }
  // Sanitiza tab/newline (delimitadores).
  user = string.replace(user, "\t", " ");
  user = string.replace(user, "\n", " ");
  text = string.replace(text, "\t", " ");
  text = string.replace(text, "\n", " ");

  sync.mutex_lock(lock);
  seq = seq + 1;
  const id = seq;
  buffer = buffer + id + "\t" + user + "\t" + text + "\n";
  sync.mutex_unlock(lock);
}

// Long-poll: espera ate buffer ter MAIS bytes que o cliente ja' tem.
// Cliente passa "since=N" onde N e' o tamanho em bytes do que ele
// recebeu da ultima vez. Server retorna apenas o slice apos N.
// Filtro por byte-offset eh mais simples e robusto que filtro por id.
// Short-poll: retorna imediatamente o que ha desde `since`, ou 204 se
// nada novo. Cliente faz polling a cada 1s.
//
// Era pra ser long-poll mas top-level globals em RTS atual nao sao
// thread-shared entre workers tokio do actix — busy-wait no servidor
// nunca via update do buffer. Cliente reagenda em 1s, simples e
// funciona.
function handlePoll(req: i64, path: string): void {
  let since: i64 = 0;
  const prefix: i64 = 10; // len("/api/poll/")
  if (string.byte_len(path) > prefix) {
    since = parseI64(path.slice(prefix));
  }

  sync.mutex_lock(lock);
  const totalLen = string.byte_len(buffer);
  if (since > totalLen) {
    // Cliente esta a frente — sinal de que o server reiniciou e perdeu
    // estado. Devolve 205 Reset Content (semantica: cliente deve
    // re-bootstrap) com snapshot completo no body.
    const snap = buffer;
    sync.mutex_unlock(lock);
    http_server.respond(req, 205, "text/plain; charset=utf-8", snap);
    return;
  }
  if (totalLen > since) {
    const snap = buffer.slice(since);
    sync.mutex_unlock(lock);
    http_server.respond(req, 200, "text/plain; charset=utf-8", snap);
    return;
  }
  sync.mutex_unlock(lock);
  http_server.respond(req, 204, "text/plain", "");
}

function parseI64(s: string): i64 {
  let n: i64 = 0;
  let i: i64 = 0;
  const len = string.byte_len(s);
  while (i < len) {
    const c = string.char_code_at(s, i);
    if (c < 48 || c > 57) break;
    n = n * 10 + (c - 48);
    i = i + 1;
  }
  return n;
}

function renderHome(): string {
  return `<!DOCTYPE html>
<html lang="pt-br">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>RTS Chat — long polling demo</title>
  <style>
    :root {
      --bg: #0a0e1a;
      --bg-card: #131826;
      --bg-card-2: #1a2033;
      --fg: #e6e9ef;
      --fg-dim: #a0a8b8;
      --accent: #7c5cff;
      --accent-2: #00d4ff;
      --success: #4ade80;
      --border: #252b3d;
      --them: #1a2033;
    }
    @media (prefers-color-scheme: light) {
      :root {
        --bg: #f7f8fc;
        --bg-card: #ffffff;
        --bg-card-2: #f0f2f7;
        --fg: #0a0e1a;
        --fg-dim: #5a6478;
        --border: #e1e5ed;
        --them: #e8ecf3;
      }
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    html, body { height: 100%; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      background: var(--bg);
      color: var(--fg);
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    body::before {
      content: '';
      position: fixed;
      inset: 0;
      background:
        radial-gradient(ellipse at top, rgba(124, 92, 255, 0.12), transparent 60%),
        radial-gradient(ellipse at bottom right, rgba(0, 212, 255, 0.08), transparent 50%);
      pointer-events: none;
      z-index: 0;
    }
    .topbar {
      position: relative;
      z-index: 2;
      padding: 1rem 1.5rem;
      background: var(--bg-card);
      border-bottom: 1px solid var(--border);
      display: flex;
      align-items: center;
      gap: 1rem;
      flex-wrap: wrap;
    }
    .logo {
      font-weight: 800;
      font-size: 1.2rem;
      background: linear-gradient(135deg, var(--accent), var(--accent-2));
      -webkit-background-clip: text;
      background-clip: text;
      -webkit-text-fill-color: transparent;
      letter-spacing: -0.02em;
    }
    .conn {
      display: flex;
      align-items: center;
      gap: 0.4rem;
      font-size: 0.85rem;
      color: var(--fg-dim);
      margin-left: auto;
    }
    .conn-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--success);
      box-shadow: 0 0 8px var(--success);
      animation: pulse 2s ease-in-out infinite;
    }
    .conn-dot.offline { background: #ff5c5c; box-shadow: 0 0 8px #ff5c5c; animation: none; }
    @keyframes pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.5; }
    }
    .name-input {
      background: var(--bg-card-2);
      color: var(--fg);
      border: 1px solid var(--border);
      padding: 0.4rem 0.7rem;
      border-radius: 8px;
      font-size: 0.9rem;
      width: 130px;
    }
    .name-input:focus { outline: 2px solid var(--accent); outline-offset: 0; }
    main {
      flex: 1;
      max-width: 800px;
      width: 100%;
      margin: 0 auto;
      padding: 1.5rem;
      display: flex;
      flex-direction: column;
      gap: 1rem;
      position: relative;
      z-index: 1;
      min-height: 0;
    }
    #log {
      flex: 1 1 0;
      min-height: 0;
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 16px;
      padding: 1rem;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
    }
    .msg {
      max-width: 75%;
      padding: 0.6rem 0.9rem;
      border-radius: 14px;
      animation: slideIn 0.25s ease-out;
      word-wrap: break-word;
    }
    @keyframes slideIn {
      from { opacity: 0; transform: translateY(6px); }
      to { opacity: 1; transform: translateY(0); }
    }
    .msg.them {
      background: var(--them);
      align-self: flex-start;
      border-bottom-left-radius: 4px;
    }
    .msg.me {
      background: linear-gradient(135deg, var(--accent), var(--accent-2));
      color: white;
      align-self: flex-end;
      border-bottom-right-radius: 4px;
    }
    .msg .who {
      font-size: 0.72rem;
      opacity: 0.7;
      margin-bottom: 0.15rem;
      font-weight: 600;
    }
    .msg .text { font-size: 0.95rem; line-height: 1.4; }
    .empty-state {
      color: var(--fg-dim);
      text-align: center;
      padding: 2rem;
      font-style: italic;
    }
    form {
      display: flex;
      gap: 0.5rem;
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 0.5rem;
    }
    input.text {
      flex: 1;
      background: transparent;
      color: var(--fg);
      border: 0;
      padding: 0.6rem 0.8rem;
      font-size: 0.95rem;
    }
    input.text:focus { outline: none; }
    button {
      background: linear-gradient(135deg, var(--accent), var(--accent-2));
      color: white;
      border: 0;
      padding: 0.6rem 1.2rem;
      border-radius: 10px;
      font-size: 0.95rem;
      font-weight: 600;
      cursor: pointer;
      transition: transform 0.15s, box-shadow 0.15s;
    }
    button:hover {
      transform: translateY(-1px);
      box-shadow: 0 8px 18px -6px rgba(124, 92, 255, 0.5);
    }
    button:active { transform: translateY(0); }
    .meta {
      font-size: 0.78rem;
      color: var(--fg-dim);
      text-align: center;
      padding: 0.5rem 0 1rem;
    }
    code {
      background: var(--bg-card-2);
      padding: 0.1rem 0.35rem;
      border-radius: 4px;
      font-family: ui-monospace, monospace;
      font-size: 0.85em;
      color: var(--accent-2);
    }
  </style>
</head>
<body>
  <div class="topbar">
    <div class="logo">RTS Chat</div>
    <input id="name" class="name-input" placeholder="seu nome" maxlength="20">
    <div class="conn"><span class="conn-dot" id="dot"></span><span id="conn-label">conectando...</span></div>
  </div>
  <main>
    <div id="log">
      <div class="empty-state">aguardando primeira mensagem...</div>
    </div>
    <form id="form" autocomplete="off">
      <input id="msg" class="text" placeholder="diga algo..." maxlength="500" required>
      <button type="submit">Enviar</button>
    </form>
    <div class="meta">long-polling em <code>/api/poll/&lt;byte-offset&gt;</code> · backend RTS + actix-web</div>
  </main>
  <script>
    const log = document.getElementById('log');
    const form = document.getElementById('form');
    const inputMsg = document.getElementById('msg');
    const inputName = document.getElementById('name');
    const dot = document.getElementById('dot');
    const connLabel = document.getElementById('conn-label');

    inputName.value = localStorage.getItem('rts-chat-name') || ('user' + (Math.random()*9999|0));
    inputName.addEventListener('change', () => localStorage.setItem('rts-chat-name', inputName.value || 'anon'));

    // since = byte offset no buffer global do server.
    // Cada poll retorna so' o slice depois desse offset.
    let since = 0;
    let firstLoad = true;

    function appendMsg(id, user, text) {
      if (firstLoad) { log.innerHTML = ''; firstLoad = false; }
      const el = document.createElement('div');
      const me = user === inputName.value;
      el.className = 'msg ' + (me ? 'me' : 'them');
      el.innerHTML = '<div class="who">' + escapeHtml(user) + '</div><div class="text">' + escapeHtml(text) + '</div>';
      log.appendChild(el);
      log.scrollTop = log.scrollHeight;
    }

    function escapeHtml(s) {
      return s.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
    }

    function parseLines(text) {
      if (!text) return [];
      return text.split('\\n').filter(l => l.length > 0).map(line => {
        const parts = line.split('\\t');
        if (parts.length < 3) return null;
        return { id: +parts[0], user: parts[1], text: parts.slice(2).join('\\t') };
      }).filter(Boolean);
    }

    let totalMsgs = 0;

    function ingest(text) {
      const msgs = parseLines(text);
      for (const m of msgs) {
        appendMsg(m.id, m.user, m.text);
        totalMsgs++;
      }
    }

    // Calcula byte-length de uma string em UTF-8 — equivalente ao
    // string.byte_len() do RTS no server.
    function utf8Len(s) {
      return new TextEncoder().encode(s).length;
    }

    async function bootstrap() {
      try {
        const r = await fetch('/api/messages');
        if (!r.ok) throw new Error('http ' + r.status);
        const text = await r.text();
        ingest(text);
        since = utf8Len(text);
        setOnline();
        poll();
      } catch (e) {
        setOffline(e.message);
        setTimeout(bootstrap, 2000);
      }
    }

    const POLL_INTERVAL = 1000;

    async function poll() {
      try {
        const r = await fetch('/api/poll/' + since);
        if (r.status === 204) {
          setOnline();
          setTimeout(poll, POLL_INTERVAL);
          return;
        }
        if (r.status === 205) {
          // Server reiniciou — limpa UI e refaz bootstrap.
          log.innerHTML = '<div class="empty-state">aguardando primeira mensagem...</div>';
          firstLoad = true;
          totalMsgs = 0;
          const text = await r.text();
          ingest(text);
          since = utf8Len(text);
          setOnline();
          setTimeout(poll, POLL_INTERVAL);
          return;
        }
        if (!r.ok) throw new Error('http ' + r.status);
        const text = await r.text();
        ingest(text);
        since += utf8Len(text);
        setOnline();
        setTimeout(poll, POLL_INTERVAL);
      } catch (e) {
        setOffline(e.message);
        setTimeout(poll, POLL_INTERVAL);
      }
    }

    function setOnline() {
      dot.classList.remove('offline');
      connLabel.textContent = 'conectado · ' + totalMsgs + ' msgs';
    }
    function setOffline(why) {
      dot.classList.add('offline');
      connLabel.textContent = 'reconectando... ' + (why || '');
    }

    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const text = inputMsg.value.trim();
      if (!text) return;
      const user = inputName.value.trim() || 'anon';
      inputMsg.value = '';
      try {
        await fetch('/api/send', {
          method: 'POST',
          headers: { 'content-type': 'text/plain' },
          body: user + '|' + text,
        });
      } catch (err) {
        inputMsg.value = text;
        setOffline(err.message);
      }
      inputMsg.focus();
    });

    bootstrap();
  </script>
</body>
</html>`;
}
