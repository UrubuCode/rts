#!/usr/bin/env python3
"""Desenha os sprites do jogo do dinossauro como PNG RGBA de 8 bits.

    python scripts/claude_dino_sprites.py > /tmp/sprites.json

Escreve um JSON `{ nome: "data:image/png;base64,..." }` que o `claude-dino.html`
embute. Sao data URLs e nao ficheiros porque o caminho de um `<img src>` e
resolvido contra a base do documento, e a pagina do jogo e carregada de sitios
diferentes (a janela, o rasterizador da regua); uma data URL nao depende disso.

O formato e o subconjunto que `rts_dom::imagem::png` aceita: 8 bits por canal,
RGBA, sem entrelacamento, filtro 0 em todas as linhas. Nada aqui usa PIL — o
repositorio nao tem dependencias de Python, e um PNG sem filtros sao trinta
linhas de zlib e struct.

Os sprites sao desenhados por RECTANGULOS e nao por uma grelha de pixeis a mao:
o dinossauro do Chrome e arte de blocos, e uma lista de rectangulos le-se e
edita-se, enquanto uma grelha de 44x47 caracteres nao.
"""

import base64
import json
import struct
import zlib

PRETO = (83, 83, 83, 255)
CLARO = (172, 172, 172, 255)
BRANCO = (255, 255, 255, 255)
VAZIO = (0, 0, 0, 0)


class Tela:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [VAZIO] * (w * h)

    def rect(self, x, y, w, h, cor=PRETO):
        for j in range(max(0, y), min(self.h, y + h)):
            for i in range(max(0, x), min(self.w, x + w)):
                self.px[j * self.w + i] = cor

    def png(self):
        cru = bytearray()
        for j in range(self.h):
            cru.append(0)  # filtro 0: sem previsao, e o que o descodificador le mais depressa
            for i in range(self.w):
                cru.extend(self.px[j * self.w + i])
        def chunk(tipo, dados):
            return (struct.pack(">I", len(dados)) + tipo + dados
                    + struct.pack(">I", zlib.crc32(tipo + dados) & 0xFFFFFFFF))
        ihdr = struct.pack(">IIBBBBB", self.w, self.h, 8, 6, 0, 0, 0)
        return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
                + chunk(b"IDAT", zlib.compress(bytes(cru), 9)) + chunk(b"IEND", b""))

    def url(self):
        return "data:image/png;base64," + base64.b64encode(self.png()).decode("ascii")


def corpo_do_dino(t, perna):
    """O tronco, a cabeca e a cauda — comuns as tres poses de corrida.

    `perna` = 0 esquerda a frente, 1 direita a frente, 2 ambas em baixo (parado).
    """
    # cabeca
    t.rect(22, 0, 20, 16)
    t.rect(20, 4, 2, 10)
    # focinho
    t.rect(42, 8, 2, 6)
    # olho (buraco branco na cabeca)
    t.rect(35, 4, 4, 4, BRANCO)
    # queixo
    t.rect(24, 16, 16, 4)
    # pescoco
    t.rect(18, 14, 8, 8)
    # tronco
    t.rect(8, 20, 22, 14)
    # cauda
    t.rect(0, 16, 10, 8)
    t.rect(4, 24, 8, 4)
    # braco
    t.rect(26, 24, 8, 3)
    # pernas
    if perna == 0:
        t.rect(10, 34, 6, 12)
        t.rect(10, 44, 10, 3)
        t.rect(20, 34, 6, 6)
    elif perna == 1:
        t.rect(20, 34, 6, 12)
        t.rect(20, 44, 10, 3)
        t.rect(10, 34, 6, 6)
    else:
        t.rect(10, 34, 6, 13)
        t.rect(20, 34, 6, 13)


def dino(perna, morto=False):
    t = Tela(46, 48)
    corpo_do_dino(t, perna)
    if morto:
        # o olho do dino morto e um X: no jogo original ele fecha os olhos
        t.rect(35, 4, 4, 4, PRETO)
        t.rect(36, 5, 2, 2, BRANCO)
    return t


def dino_agachado(quadro):
    """Agachado: mais comprido, mais baixo — e por isso passa por baixo da ave."""
    t = Tela(60, 30)
    t.rect(36, 4, 20, 12)          # cabeca
    t.rect(56, 10, 2, 4)           # focinho
    t.rect(48, 7, 4, 4, BRANCO)    # olho
    t.rect(38, 16, 16, 3)          # queixo
    t.rect(8, 6, 34, 14)           # tronco esticado
    t.rect(0, 4, 10, 8)            # cauda
    if quadro == 0:
        t.rect(12, 20, 6, 8)
        t.rect(12, 26, 10, 3)
        t.rect(26, 20, 6, 4)
    else:
        t.rect(26, 20, 6, 8)
        t.rect(26, 26, 10, 3)
        t.rect(12, 20, 6, 4)
    return t


def cacto_pequeno(n):
    largura = 17 * n
    t = Tela(largura, 36)
    for k in range(n):
        x = k * 17
        t.rect(x + 6, 0, 5, 36)     # tronco
        t.rect(x + 2, 8, 4, 12)     # braco esquerdo
        t.rect(x + 2, 8, 2, 4)
        t.rect(x + 11, 12, 4, 12)   # braco direito
        t.rect(x + 13, 12, 2, 4)
    return t


def cacto_grande(n):
    largura = 25 * n
    t = Tela(largura, 50)
    for k in range(n):
        x = k * 25
        t.rect(x + 9, 0, 7, 50)
        t.rect(x + 3, 12, 6, 16)
        t.rect(x + 3, 12, 3, 6)
        t.rect(x + 16, 18, 6, 16)
        t.rect(x + 19, 18, 3, 6)
    return t


def ave(asa):
    """O pterodatilo. `asa` = 0 asa em cima, 1 asa em baixo."""
    t = Tela(46, 40)
    t.rect(24, 14, 18, 6)          # corpo
    t.rect(40, 12, 6, 4)           # cabeca
    t.rect(44, 15, 2, 2)           # bico
    t.rect(36, 13, 2, 2, BRANCO)   # olho
    t.rect(18, 16, 8, 4)           # cauda
    if asa == 0:
        t.rect(24, 0, 14, 14)      # asa erguida
        t.rect(20, 6, 6, 8)
    else:
        t.rect(24, 20, 14, 12)     # asa em baixo
        t.rect(20, 20, 6, 8)
    return t


def nuvem():
    t = Tela(46, 14)
    t.rect(10, 0, 24, 5, CLARO)
    t.rect(4, 4, 38, 6, CLARO)
    t.rect(0, 8, 46, 4, CLARO)
    return t


def chao():
    """Uma faixa de 300px que se repete: a linha mais os seixos.

    Duas peças diferentes e não uma repetida, porque um padrão que se repete de
    300 em 300 é visível a olho quando o jogo acelera.
    """
    t = Tela(300, 14)
    t.rect(0, 0, 300, 2)
    seixos = [(14, 4, 3), (43, 3, 2), (72, 5, 4), (110, 4, 2), (139, 3, 3),
              (176, 5, 2), (203, 4, 4), (238, 3, 2), (266, 5, 3), (288, 4, 2)]
    for x, y, w in seixos:
        t.rect(x, y, w, 2)
    return t


def lua(fase):
    """A lua muda de fase ao longo das noites, como no jogo original."""
    t = Tela(20, 40)
    t.rect(4, 0, 12, 40, CLARO)
    t.rect(0, 4, 6, 32, CLARO)
    if fase == 1:
        t.rect(0, 0, 8, 40, VAZIO)
    elif fase == 2:
        t.rect(0, 0, 12, 40, VAZIO)
    return t


def estrela():
    t = Tela(9, 9)
    t.rect(3, 0, 3, 3, CLARO)
    t.rect(0, 3, 9, 3, CLARO)
    t.rect(3, 6, 3, 3, CLARO)
    return t


if __name__ == "__main__":
    sprites = {
        "dinoParado": dino(2).url(),
        "dinoA": dino(0).url(),
        "dinoB": dino(1).url(),
        "dinoMorto": dino(2, morto=True).url(),
        "agachadoA": dino_agachado(0).url(),
        "agachadoB": dino_agachado(1).url(),
        "cactoP1": cacto_pequeno(1).url(),
        "cactoP2": cacto_pequeno(2).url(),
        "cactoP3": cacto_pequeno(3).url(),
        "cactoG1": cacto_grande(1).url(),
        "cactoG2": cacto_grande(2).url(),
        "aveA": ave(0).url(),
        "aveB": ave(1).url(),
        "nuvem": nuvem().url(),
        "chao": chao().url(),
        "lua0": lua(0).url(),
        "lua1": lua(1).url(),
        "lua2": lua(2).url(),
        "estrela": estrela().url(),
    }
    print(json.dumps(sprites))
