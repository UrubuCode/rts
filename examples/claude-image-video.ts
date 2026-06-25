import egui from "rts:egui";
import render from "rts:render";
import buffer from "rts:buffer";

// render.image — desenhar um BITMAP dentro da janela. Aqui um "vídeo" procedural:
// um padrão RGBA gerado por código que se move a cada frame (prova conteúdo
// dinâmico/vídeo num retângulo, sem codec). É a base pra imagem/vídeo/viewport.
//   target/release/rts.exe run examples/claude-image-video.ts

const app = createApp("render.image (video procedural)", 480, 420);

// imagem pequena (o TS gera os pixels). RGBA8, linha-major.
const IW = 96;
const IH = 72;
const buf = buffer.alloc(IW * IH * 4);
const ptr = buffer.ptr(buf);

let t = 0;
let moved = 0;

while (app.running()) {
  if (!app.beginFrame()) break;
  // move pra TELA 2 apos a janela existir (winit so aplica set_outer_position
  // depois do event loop rodar; chamar antes do 1o frame nao pega).
  if (moved === 0 && app.frameCount() > 2) {
    app.moveTo(2200, 450); // centro da tela 2 (monitor left=1920, top=313)
    moved = 1;
  }
  app.fillRect(0, 0, 480, 420, 0x0E1218FF);
  app.text(16, 12, "Video procedural via render.image", 0xB0B8C0FF, 15);

  // GERA o frame: gradiente animado (cada pixel = f(x,y,t)). 1 i32 por pixel RGBA.
  let py = 0;
  while (py < IH) {
    let px = 0;
    while (px < IW) {
      const r = (px * 4 + t) & 0xFF;
      const g = (py * 4 + t * 2) & 0xFF;
      const b = ((px + py) * 2 + t * 3) & 0xFF;
      // RGBA empacotado: o buffer é little-endian; escrevemos R,G,B,A como i32
      // na ordem de bytes R(0) G(1) B(2) A(3) => valor = R | G<<8 | B<<16 | A<<24
      const rgba = r | (g << 8) | (b << 16) | (0xFF << 24);
      const off = (py * IW + px) * 4;
      buffer.write_i32(buf, off, rgba);
      px = px + 1;
    }
    py = py + 1;
  }

  // desenha a imagem escalada num retângulo grande (a "tela do video")
  render.image(app._win, 40, 50, 400, 300, ptr, IW, IH);

  app.box(40, 50, 400, 300, 0x00000000, 2, 0x3399FFFF, 0); // moldura
  app.text(16, 380, "frame " + t + " — pixels gerados em TS, pintados pelo backend", 0x808890FF, 12);

  t = t + 2;
  if (t > 100000) t = 0;
  app.endFrame();
}

buffer.free(buf);
app.close();
