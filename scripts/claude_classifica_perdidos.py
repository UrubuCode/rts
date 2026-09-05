#!/usr/bin/env python3
"""Classifica os reftests que um lote PERDEU: acidente a desfazer-se ou regressao?

    python scripts/claude_classifica_perdidos.py <pasta-wpt> <raster-antes.exe> <raster-depois.exe> <nome...>

Para cada teste, rasteriza os DOIS lados (o teste e a sua referencia) com os
DOIS binarios, e compara cada lado contra si mesmo:

    ref mudou   -> CONVERGENCIA TRIVIAL a desfazer-se, nao e regressao
    so o teste  -> REGRESSAO a investigar
    nenhum      -> outra causa (o par ja divergia)

## Porque esta pergunta existe

Um reftest cujos dois lados dependem da mesma feature em falta **passa por
convergencia trivial**: os dois igualmente errados, e dois erros iguais batem.
Quando a feature chega, o teste CAI — e isso e progresso, nao regressao.

Em 2026-09-05 isto apareceu em cinco lotes independentes num so dia: o
`<![CDATA[]]>` que matava a folha de estilo (a referencia partilhada de centenas
de reftests renderizava em branco, e o teste tambem); o `writing-mode` sem
efeito nenhum (os dois lados cegos a troca de eixo); a `hanging-punctuation`
inexistente (os dois lados a mover-se pelo MESMO delta de 594 px); o
`background-image` nunca pintado; e as unidades `in`/`cm`/`mm` em falta.

Ler o numero liquido sem fazer esta pergunta leva a rejeitar correccoes certas.
"""

import os
import subprocess
import sys
import re

TIMEOUT = 30


def rasteriza(binario: str, html: str, png: str) -> bool:
    try:
        subprocess.run([binario, html, png], timeout=TIMEOUT,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        return os.path.exists(png)
    except (subprocess.TimeoutExpired, OSError):
        return False


def bytes_de(p: str):
    try:
        with open(p, "rb") as f:
            return f.read()
    except OSError:
        return None


def par(pasta: str, nome: str):
    """Devolve (teste, referencia) ou None — a ref sai do `<link rel=match>`."""
    for ext in (".xht", ".html"):
        teste = os.path.join(pasta, nome + ext)
        if not os.path.exists(teste):
            continue
        with open(teste, encoding="utf-8", errors="ignore") as f:
            src = f.read()
        m = (re.search(r'<link[^>]*rel=["\']?match["\']?[^>]*href=["\']([^"\']+)["\']', src, re.I)
             or re.search(r'<link[^>]*href=["\']([^"\']+)["\'][^>]*rel=["\']?match["\']?', src, re.I))
        if not m:
            return None
        ref = os.path.normpath(os.path.join(os.path.dirname(teste), m.group(1)))
        return (teste, ref) if os.path.exists(ref) else None
    return None


def main() -> int:
    if len(sys.argv) < 5:
        print(__doc__)
        return 2
    pasta, antes, depois, nomes = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
    tmp = os.environ.get("TEMP", ".")
    contagem = {"convergencia trivial": 0, "REGRESSAO": 0, "outra causa": 0, "sem par": 0}

    for nome in nomes:
        p = par(pasta, nome)
        if not p:
            contagem["sem par"] += 1
            print(f"  {nome:<48} sem par legivel")
            continue
        mudou = {}
        for rotulo, caminho in (("teste", p[0]), ("ref", p[1])):
            saidas = []
            for tag, binario in (("a", antes), ("b", depois)):
                png = os.path.join(tmp, f"cls-{tag}.png")
                saidas.append(bytes_de(png) if rasteriza(binario, caminho, png) else None)
            mudou[rotulo] = saidas[0] != saidas[1]
        if mudou["ref"]:
            veredito = "convergencia trivial"
        elif mudou["teste"]:
            veredito = "REGRESSAO"
        else:
            veredito = "outra causa"
        contagem[veredito] += 1
        print(f"  {nome:<48} {veredito}")

    print()
    for k, v in contagem.items():
        if v:
            print(f"{k}: {v}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
