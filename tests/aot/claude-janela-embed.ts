// Reprodução mínima pedida depois de um relato de janela que nunca abre com
// `--embed-compiler`: abrir uma janela `rts:egui` e correr um laço curto de
// `pump`, imprimindo o que `openWindow`/`isOpen` responderam ANTES do laço —
// exatamente o que uma falha real de arranque (handle 0, `isOpen` falso já na
// primeira volta, ou o processo a terminar antes do laço) deixaria à vista.
//
// Não é um `#[test]` do Rust: `crates/rts-host/tests/ui_surface.rs` já
// documenta porque nenhum é — o winit entra em pânico ao criar o event loop
// fora da thread PRINCIPAL, e um `#[test]` do cargo corre numa secundária.
// Este ficheiro corre como o PRÓPRIO processo de um `.exe` compilado, na sua
// única thread, que é onde `rts-runtime-boot::run` corre o programa inteiro —
// a mesma razão por que a janela do comparativo RTS vs Electron abre.
//
// Investigado (04-09) depois de um relato de janela que nunca abre com
// `--embed-compiler` na app React do comparativo: reproduzido tanto quanto
// possível sem o bundle exato — janela sozinha, janela com um `<script>`
// simples via `runScriptsAt`, e a app React inteira (HTML +
// `runScriptsAt` inserido a seguir a `loadResources`, como no relato) — todos
// consistentemente OK em várias corridas, com e sem `--all-namespaces`. A
// única falha observada foi um TIMEOUT de 8s na primeira tentativa, sob a
// carga de dezenas de agentes a compilar em paralelo na mesma máquina, que
// não se repetiu nas corridas seguintes — a leitura mais provável é
// contenção transitória, não um defeito deste lote. Este ficheiro fica como
// a reprodução para verificar de novo se o sintoma voltar.
import egui from "rts:egui";

const win = egui.openWindow("claude-janela-embed", 320, 240, 0);
console.log("handle=" + win);
console.log("isOpen_inicial=" + egui.isOpen(win));

let frames = 0;
const inicio = Date.now();
while (egui.isOpen(win)) {
  if (!egui.pump(win)) break;
  egui.beginFrame(win);
  egui.drawText(win, "frame " + frames, 0);
  egui.endFrame(win);
  frames = frames + 1;
  // Curto de propósito: isto corre no smoke de CI, headless — a claim é que
  // o LAÇO correu, não que alguém viu a janela pintada.
  if (Date.now() - inicio > 1500) break;
}
egui.close(win);
console.log("frames=" + frames);
