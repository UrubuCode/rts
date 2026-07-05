// RTS-MINE — configuração central (janela, resolução do renderer, mundo, teclas).
//
// REGRA DO MOTOR (#1726): estas consts exportadas são usadas APENAS no top-level
// da main. Dentro de funções de módulo, valores chegam por PARÂMETRO ou pelo
// state buffer (stbuf) — const importada usada como argumento de chamada dentro
// de uma função não captura.

// janela
export const W = 960;
export const H = 640;

// resolução interna do renderer (framebuffer escalado pra janela), com render
// CHECKERBOARD (metade dos pixels por frame, alternando). Default 240×160:
// teto medido ~60 FPS (casa com o vsync). Configurável em jogo (menu Esc);
// o framebuffer é alocado para o MÁXIMO (RMAXW×RMAXH).
export const RW = 240;
export const RH = 160;
export const RMAXW = 384;
export const RMAXH = 256;

// mundo (dimensões também são gravadas no stbuf, slots 14..17)
export const WX = 64;  // largura (x)
export const WY = 32;  // altura (y)
export const WZ = 64;  // profundidade (z)
export const WL = 9;   // nível da água

// ids de bloco: 0 ar, 1 grama, 2 terra, 3 pedra, 4 água, 5 areia, 6 tronco, 7 folhas

// teclas (códigos neutros do rts-input): letras 100+idx, setas 5-8, dígitos 130+
export const KW = 122;
export const KA = 100;
export const KS = 118;
export const KD = 103;
export const KSPACE = 3;
export const KESC = 2;
export const KUP = 5;
export const KDOWN = 6;
export const KLEFT = 7;
export const KRIGHT = 8;
export const K1 = 131;
export const K2 = 132;
export const K3 = 133;
export const K4 = 134;
export const K5 = 135;

// ── STATE BUFFER (stbuf) — contrato entre os módulos ─────────────────────────
// f64 por slot (offset = slot * 8). Quem escreve/lê está indicado:
//   0 px   1 py   2 pz      — posição do olho do jogador   (main ← player)
//   3 vy                    — (reservado)
//   4 yaw  5 pitch          — câmera                        (main ← player)
//   6 grounded  7 inWater   — (reservado)
//   8 selX 9 selY 10 selZ   — bloco mirado (-1 = nenhum)    (raycast.castEditRay)
//   11 plX 12 plY 13 plZ    — célula adjacente p/ colocar   (raycast.castEditRay)
//   14 WX  15 WY  16 WZ  17 WL — dimensões do mundo         (main, uma vez)
//   18 RW  19 RH             — resolução ATIVA do renderer  (main, ao trocar)
//   20 tsec                  — relógio do jogo (animações)  (main, por frame)
//   21 maxH                  — topo global do mundo (sky-culling do raycaster)
export const ST_SLOTS = 22;
