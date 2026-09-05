#!/usr/bin/env python3
"""Lock cooperativo de ficheiros, para vários agentes na MESMA árvore de trabalho.

    python scripts/claude_lock.py take crates/rts-dom/src/layout/bloco.rs meu-lote
    python scripts/claude_lock.py free crates/rts-dom/src/layout/bloco.rs meu-lote
    python scripts/claude_lock.py status
    python scripts/claude_lock.py mine meu-lote

`take` sai com 0 quando o ficheiro ficou seu e com 1 quando já é de outro — e
nesse caso imprime de quem e há quanto tempo, para se poder esperar ou pedir.

## Porque isto existe, e porque é COOPERATIVO e não obrigatório

Vários agentes a trabalhar na mesma árvore poupam o que mais custa aqui: uma só
cópia do repositório e um só `target/`, portanto uma só compilação para todos.
Em 2026-09-05 o método anterior — uma worktree por agente — deixou 68 delas com
o seu `target/` cada, **98 GB**, e encheu o SSD a 100 %.

O que uma árvore partilhada arrisca é dois agentes a escrever no mesmo ficheiro.
A primeira linha de defesa é a atribuição: cada lote recebe ficheiros
exclusivos. Este lock é a segunda, para o caso que a atribuição não previu — um
agente que descobre a meio que precisa de tocar num ficheiro alheio.

Não impede a escrita (nada aqui pode impedir um `Write`); torna a colisão
VISÍVEL antes de acontecer, que é o que falta quando duas edições se
sobrepõem em silêncio e a última ganha.

## Locks velhos

Um lock com mais de `--velho` minutos (por omissão 45) é marcado `VELHO` no
`status` e pode ser tomado com `--forcar`. Um agente que morre deixa o lock
para trás, e sem isto a árvore ficava trancada para sempre.
"""

import argparse
import json
import os
import sys
import time

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PASTA = os.path.join(RAIZ, ".claude", "locks")


def _caminho_lock(alvo: str) -> str:
    plano = alvo.replace("\\", "/").strip("/").replace("/", "__")
    return os.path.join(PASTA, plano + ".json")


def _ler(p: str):
    try:
        with open(p, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def take(alvo: str, quem: str, velho_min: int, forcar: bool) -> int:
    os.makedirs(PASTA, exist_ok=True)
    p = _caminho_lock(alvo)
    atual = _ler(p)
    if atual and atual.get("quem") != quem:
        idade = (time.time() - atual.get("t", 0)) / 60
        if idade < velho_min and not forcar:
            print(f"OCUPADO: {alvo} é de '{atual['quem']}' há {idade:.0f} min.")
            print("  espere e tente outra vez, ou peça ao master para arbitrar.")
            return 1
        print(f"(lock de '{atual['quem']}' tinha {idade:.0f} min — tomado)")
    with open(p, "w", encoding="utf-8") as f:
        json.dump({"alvo": alvo, "quem": quem, "t": time.time()}, f)
    print(f"OK: {alvo} é seu.")
    return 0


def free(alvo: str, quem: str) -> int:
    p = _caminho_lock(alvo)
    atual = _ler(p)
    if not atual:
        print(f"(não estava trancado: {alvo})")
        return 0
    if atual.get("quem") != quem:
        print(f"RECUSADO: {alvo} é de '{atual['quem']}', não seu.")
        return 1
    os.remove(p)
    print(f"libertado: {alvo}")
    return 0


def status(velho_min: int) -> int:
    if not os.path.isdir(PASTA):
        print("nenhum ficheiro trancado.")
        return 0
    linhas = []
    for nome in sorted(os.listdir(PASTA)):
        d = _ler(os.path.join(PASTA, nome))
        if not d:
            continue
        idade = (time.time() - d.get("t", 0)) / 60
        marca = "  VELHO" if idade >= velho_min else ""
        linhas.append(f"  {d['quem']:<28} {idade:5.0f} min  {d['alvo']}{marca}")
    print("\n".join(linhas) if linhas else "nenhum ficheiro trancado.")
    return 0


def mine(quem: str) -> int:
    if not os.path.isdir(PASTA):
        return 0
    for nome in sorted(os.listdir(PASTA)):
        d = _ler(os.path.join(PASTA, nome))
        if d and d.get("quem") == quem:
            print(d["alvo"])
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="lock cooperativo de ficheiros entre agentes")
    ap.add_argument("acao", choices=["take", "free", "status", "mine"])
    ap.add_argument("alvo", nargs="?", help="caminho do ficheiro (ou o nome do agente, em `mine`)")
    ap.add_argument("quem", nargs="?", help="nome do agente/lote")
    ap.add_argument("--velho", type=int, default=45, help="minutos a partir dos quais um lock é VELHO")
    ap.add_argument("--forcar", action="store_true", help="tomar mesmo um lock que não é velho")
    a = ap.parse_args()

    if a.acao == "status":
        return status(a.velho)
    if a.acao == "mine":
        if not a.alvo:
            ap.error("mine precisa do nome do agente")
        return mine(a.alvo)
    if not a.alvo or not a.quem:
        ap.error(f"{a.acao} precisa de <ficheiro> <quem>")
    return take(a.alvo, a.quem, a.velho, a.forcar) if a.acao == "take" else free(a.alvo, a.quem)


if __name__ == "__main__":
    sys.exit(main())
