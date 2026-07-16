// Site de apresentacao do RTS — renderizado pelo motor CSS nativo E compilavel
// para .exe standalone. O HTML fica EMBUTIDO inline (string literal) para o
// binario AOT nao depender de nenhum arquivo externo em disco.
//
//   JIT:  target/release/rts.exe run examples/claude-rts-site.ts
//   AOT:  target/release/rts.exe compile --all-namespaces examples/claude-rts-site.ts dist/RTS-Site.exe
import egui from "rts:egui";
import dom from "rts:dom";

const html = `<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <meta charset="utf-8">
  <title>RTS — TypeScript compilado para nativo</title>
  <style>
    /* ════════════════════════════════════════════════════════════════════════
       RTS — landing de apresentação do próprio compilador, renderizada pelo
       motor CSS nativo do RTS (sem browser, sem deps). É meta: o RTS se
       apresenta usando a si mesmo. Compilável para .exe standalone via
       \`rts compile --all-namespaces\`.
       Paleta: laranja-ferrugem (Rust/Cranelift) + ciano sobre grafite.
       ════════════════════════════════════════════════════════════════════════ */
    body {
      background: #0b0d11;
      color: #e6e9ef;
      font-family: sans-serif;
      font-size: 16px;
      line-height: 1.6;
      margin: 0;
      padding: 0;
    }
    .shell {
      max-width: 1040px;
      margin: 0 auto;
      padding: 0 28px;
    }

    @keyframes spark {
      0%   { background: #f97316 }
      50%  { background: #fb923c }
      100% { background: #f97316 }
    }
    @keyframes cyanpulse {
      from { background: #22d3ee }
      to   { background: #67e8f9 }
    }

    /* ── Navbar ─────────────────────────────────────────────────────────────── */
    .nav {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 24px 0;
    }
    .nav .logo { display: flex; align-items: center; gap: 12px }
    .nav .mark {
      width: 34px; height: 34px;
      background: #f97316;
      border-radius: 9px;
      animation: spark 2.4s ease-in-out infinite;
    }
    .nav .brand { font-size: 21px; font-weight: bold; color: #ffffff }
    .nav .links { display: flex; align-items: center; gap: 26px }
    .nav .links a { color: #97a2b6 }
    .nav .cta {
      background: #f97316;
      color: #1a0e03;
      font-weight: bold;
      padding: 10px 20px;
      border-radius: 999px;
    }

    /* ── Hero ───────────────────────────────────────────────────────────────── */
    .hero { text-align: center; padding: 66px 0 50px 0 }
    .hero .pill {
      display: inline-block;
      background: #1c1408;
      color: #fdba74;
      font-size: 13px;
      letter-spacing: 1px;
      text-transform: uppercase;
      padding: 7px 16px;
      border-radius: 999px;
      margin-bottom: 26px;
    }
    .hero h1 {
      font-size: 54px;
      line-height: 1.08;
      color: #ffffff;
      margin: 0 0 20px 0;
    }
    .hero h1 .accent { color: #f97316 }
    .hero h1 .cy { color: #22d3ee }
    .hero p {
      font-size: 19px;
      color: #a4afc4;
      max-width: 640px;
      margin: 0 auto 34px auto;
    }
    .hero .actions { display: flex; justify-content: center; gap: 16px }
    .hero .primary {
      background: #f97316;
      color: #1a0e03;
      font-weight: bold;
      padding: 14px 30px;
      border-radius: 12px;
    }
    .hero .ghost {
      background: #161b24;
      color: #e6e9ef;
      padding: 14px 30px;
      border-radius: 12px;
    }

    /* ── Bloco de código (o pitch é: isto vira binário nativo) ─────────────── */
    .code {
      background: #10141c;
      border-radius: 14px;
      padding: 22px 26px;
      margin: 8px auto 60px auto;
      max-width: 660px;
      font-family: monospace;
      line-height: 1.7;
    }
    .code .cm { color: #5b6678 }
    .code .kw { color: #f97316 }
    .code .st { color: #22d3ee }
    .code .fn { color: #c4b5fd }

    /* ── Métricas (benchmarks reais) ────────────────────────────────────────── */
    .stats {
      display: flex;
      justify-content: space-between;
      gap: 18px;
      padding: 0 0 62px 0;
    }
    .stat {
      background: #10141c;
      border-radius: 16px;
      padding: 26px 22px;
      text-align: center;
    }
    .stat .num { font-size: 32px; font-weight: bold; color: #f97316 }
    .stat:nth-child(2) .num { color: #22d3ee }
    .stat:nth-child(3) .num { color: #a78bfa }
    .stat:nth-child(4) .num { color: #34d399 }
    .stat .lbl { font-size: 13px; color: #8592a8; text-transform: uppercase; letter-spacing: 1px }

    /* ── Recursos ───────────────────────────────────────────────────────────── */
    .section-head { text-align: center; margin-bottom: 44px }
    .section-head h2 { font-size: 34px; color: #ffffff; margin: 0 0 12px 0 }
    .section-head p { color: #97a2b6; font-size: 17px; margin: 0 }

    .cards { display: flex; justify-content: space-between; gap: 20px }
    .card { background: #10141c; border-radius: 18px; padding: 30px 26px }
    .card .icon {
      width: 46px; height: 46px;
      border-radius: 12px;
      margin-bottom: 20px;
      background: #f97316;
    }
    .card:nth-child(2) .icon { background: #22d3ee; animation: cyanpulse 1.8s ease-in-out infinite alternate }
    .card:nth-child(3) .icon { background: #a78bfa }
    .card h3 { font-size: 20px; color: #ffffff; margin: 0 0 10px 0 }
    .card p { color: #93a0b6; font-size: 15px; margin: 0 }

    /* ── Faixa CTA ──────────────────────────────────────────────────────────── */
    .band {
      background: #17110a;
      border-radius: 22px;
      padding: 52px 40px;
      text-align: center;
      margin: 72px 0;
    }
    .band h2 { font-size: 32px; color: #ffffff; margin: 0 0 14px 0 }
    .band p { color: #a4afc4; max-width: 540px; margin: 0 auto 28px auto }
    .band .big {
      display: inline-block;
      background: #f97316;
      color: #1a0e03;
      font-weight: bold;
      font-size: 17px;
      padding: 16px 40px;
      border-radius: 14px;
    }

    /* ── Rodapé ─────────────────────────────────────────────────────────────── */
    .foot {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 30px 0 50px 0;
      color: #64748b;
      font-size: 14px;
    }
    .foot .fbrand { color: #cbd5e1; font-weight: bold }
  </style>
</head>
<body>
  <div class="shell">

    <nav class="nav">
      <div class="logo">
        <div class="mark"></div>
        <span class="brand">RTS</span>
      </div>
      <div class="links">
        <a>Motor</a>
        <a>Benchmarks</a>
        <a>Docs</a>
        <span class="cta">GitHub</span>
      </div>
    </nav>

    <section class="hero">
      <span class="pill">TypeScript → Cranelift → Nativo</span>
      <h1>Seu <span class="cy">TypeScript</span>, compilado para <span class="accent">binário nativo</span></h1>
      <p>RTS compila TS/JS direto para código de máquina via Cranelift — sem runtime externo, sem V8. Um único .exe standalone, com um modelo de valor NaN-box, shapes e inline caches.</p>
      <div class="actions">
        <span class="primary">Começar</span>
        <span class="ghost">Ver benchmarks</span>
      </div>
    </section>

    <div class="code">
      <div><span class="cm">// esta própria página foi renderizada e compilada pelo RTS</span></div>
      <div><span class="kw">const</span> pi = <span class="fn">montecarlo</span>(<span class="st">10_000_000</span>);</div>
      <div><span class="fn">print</span>(<span class="st">"pi ≈ "</span> + pi);  <span class="cm">// AOT: 16.9ms · 5.14× mais rápido que Bun</span></div>
    </div>

    <section class="stats">
      <div class="stat">
        <div class="num">16.9ms</div>
        <div class="lbl">Monte Carlo 10M · AOT</div>
      </div>
      <div class="stat">
        <div class="num">5.14×</div>
        <div class="lbl">Mais rápido que Bun</div>
      </div>
      <div class="stat">
        <div class="num">29k</div>
        <div class="lbl">Requisições / seg</div>
      </div>
      <div class="stat">
        <div class="num">79%</div>
        <div class="lbl">Paridade Bun / Node</div>
      </div>
    </section>

    <div class="section-head">
      <h2>Não é um interpretador. É um compilador.</h2>
      <p>Três decisões de arquitetura que fazem o RTS entregar velocidade de Rust com ergonomia de TypeScript.</p>
    </div>

    <section class="cards">
      <div class="card">
        <div class="icon"></div>
        <h3>Cranelift como backend</h3>
        <p>Um único caminho HIR → Cranelift IR. O egraph é o único otimizador: const-fold, CSE, DCE, inlining. JIT e AOT compartilham a mesma emissão.</p>
      </div>
      <div class="card">
        <div class="icon"></div>
        <h3>Modelo de valor NaN-box</h3>
        <p>Uma palavra de 64 bits: números ficam desempacotados em registradores onde o tipo prova monomorfismo; caem para tagged só onde precisam.</p>
      </div>
      <div class="card">
        <div class="icon"></div>
        <h3>Shapes + Inline Caches</h3>
        <p>Acesso a propriedade é comparação de shape-id + load em offset fixo. Despacho de método é shape-keyed, não busca de string O(N).</p>
      </div>
    </section>

    <section class="band">
      <h2>TypeScript que vira um executável de verdade</h2>
      <p>Sem Node, sem Bun, sem runtime empacotado. \`rts compile\` gera um .exe nativo que roda sozinho — inclusive esta landing page.</p>
      <span class="big">rts compile app.ts</span>
    </section>

    <footer class="foot">
      <span class="fbrand">RTS</span>
      <span>© 2026 · Esta página é um .exe nativo renderizado pelo motor CSS do RTS</span>
    </footer>

  </div>
</body>
</html>
`;

const d = dom.parseHtml(html);
const win = egui.openWindow("RTS — TypeScript compilado para nativo", 1100, 900, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
