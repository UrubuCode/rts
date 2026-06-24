import egui from "rts:egui";

// Incrementador AO VIVO: prova que mutar o DOM retido (setText) dentro do loop
// reflete na UI frame a frame. O número sobe sozinho enquanto a janela fica aberta.
//
// Fluxo correto (o que faltava no egui_dom_mutacao.ts):
//   1. html() UMA vez (parse inicial da árvore retida);
//   2. dentro do loop, querySelector + setText no nó do contador A CADA frame;
//   3. NUNCA re-chamar html() (senão re-parseia e perde a mutação).
//
// Rodar:  target/release/rts.exe run examples/claude-egui-incrementador.ts

// Layout das tags (top-level, literais — workaround do #1726).
egui.defineBlock("h1", 0, 26, 0, 4); // heading, fonte 26
egui.defineBlock("p", 1, 0, 0, 0); // parágrafo (wrap)

const NONE = -1; // sentinela "não encontrado" (invariante 3: -1, nunca u64::MAX)

const win = egui.openWindow("Incrementador — DOM ao vivo", 420, 200, 0);

// 1) Parse inicial: um título fixo + um parágrafo que vai virar o contador.
egui.beginFrame(win);
egui.html(win, "<h1>Contador ao vivo</h1><p id='contador'>0</p>");
egui.endFrame(win);

// 2) Pega o nó do contador UMA vez (NodeId estável durante a vida da árvore).
const contador = egui.querySelector(win, "#contador");

let n = 0;
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break; // janela fechada

  // 3) Muta o texto do nó a cada frame — SEM re-parsear HTML.
  n = n + 1;
  if (contador !== NONE) {
    egui.setText(win, contador, "" + n);
  }

  // 4) Re-renderiza o DOM mutado (frame vazio: o render desenha a árvore retida).
  egui.beginFrame(win);
  egui.endFrame(win);

  // Prova no stderr (sem depender de enxergar a janela): a cada 60 frames,
  // dumpa a árvore — o texto do <p id='contador'> deve ir subindo.
  if (n === 60) egui.domDump(win);
  if (n === 120) egui.domDump(win);
  if (n === 180) egui.domDump(win);
}

egui.close(win);
