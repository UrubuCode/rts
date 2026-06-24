// TESTE VISUAL DE TRANSPARÊNCIA — você olha a janela e confirma a olho.
//
//   target/release/rts.exe run examples/egui_transparent_test.ts
//
// A janela é FRAMELESS (sem header do SO) + fundo TRANSPARENTE. O texto fica só
// no topo; o resto é vazio. COMO LER O RESULTADO:
//   - TRANSPARENTE OK  -> você VÊ o desktop / janelas atrás na área vazia, e o
//                         texto "flutuando" sem um retângulo de fundo.
//   - TRANSPARENTE FALHOU -> a área vazia aparece como um retângulo PRETO ou
//                            CINZA sólido (o fundo não foi composto pelo SO).
// Mova outra janela (ex. o explorer) PARA TRÁS desta p/ ver através com clareza.
// Feche pelo Alt+F4 / matando o processo (é frameless, não tem botão de fechar).
//
// Troque o backend no toggle abaixo p/ comparar wgpu (DX12) vs glow (OpenGL):
//   GLOW=true  -> render OpenGL  (bits: 16 transparent + 32 frameless + 8 glow = 56)
//   GLOW=false -> render wgpu    (bits: 16 transparent + 32 frameless        = 48)
import egui from "rts:egui";

const GLOW = true;
const config = GLOW ? 56 : 48;

const page =
  "<h1>TRANSPARENTE?</h1>" +
  "<p>Se voce VE o desktop atras desta area (sem retangulo solido), a</p>" +
  "<p><b>transparencia funcionou</b>. Se aparece um bloco preto/cinza, falhou.</p>" +
  "<h2>" + (GLOW ? "backend: OpenGL (glow)" : "backend: wgpu (DX12)") + "</h2>" +
  "<p>Mova outra janela para tras desta para confirmar.</p>";

const h = egui.openWindow("transparencia", 420, 320, config);
while (egui.isOpen(h) !== 0) {
  if (egui.pump(h) !== 0) break;
  egui.beginFrame(h);
  egui.html(h, page);
  egui.endFrame(h);
}
egui.close(h);
