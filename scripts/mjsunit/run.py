#!/usr/bin/env python3
"""Corre o `test/mjsunit` do V8 contra o rts, um processo por ficheiro.

O test262 pergunta o que a NORMA exige. Esta pergunta outra coisa: o que um
motor de produção aprendeu a não errar. `mjsunit` é meio-século-máquina de
regressões — cada `regress-*.js` é um bug que alguém teve — e é JavaScript
comum sobre um arnês de um ficheiro só, que se põe à frente como o `sta.js`.

    python3 scripts/mjsunit/run.py              # tudo
    python3 scripts/mjsunit/run.py es6 harmony  # só estes diretórios
    SHARD=3/8 python3 scripts/mjsunit/run.py    # uma fatia, para CI
    python3 scripts/mjsunit/run.py --merge rows-*.tsv

`scripts/mjsunit/README.md` diz o que fica fora do denominador e porquê.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from suites import common as c  # noqa: E402

TESTS = c.ROOT / ".mjsunit" / "v8" / "test" / "mjsunit"

# A sintaxe nativa do V8 — `%OptimizeFunctionOnNextCall(f)`, `%DeoptimizeNow()`.
# Um teste que a usa está a espreitar o INTERIOR do V8: pede que uma função seja
# optimizada agora e verifica que foi. Não há motor de terceiros que isso possa
# medir, e é a mesma decisão que a suíte do Node toma para `internal/…`.
NATIVE = re.compile(r"%[A-Z][A-Za-z0-9_]*\(")
# O `d8` é o shell do V8, não a linguagem: `load()` traz outro ficheiro, `read()`
# lê o disco, `gc()` só existe com `--expose-gc`, e `Realm`, `Worker`, `Sandbox`
# e `d8.*` são objetos de host. É o mesmo argumento que põe o `$262` fora do
# test262 e o `--expose-gc` fora da régua do Node.
#
# O lookbehind não é um detalhe: `\bload\s*\(` apanha `Atomics.load(` e
# `module.exports.load(`, e media 117 ficheiros onde os do shell eram menos.
# Um skip a mais sobe a percentagem sem nada ter melhorado, que é precisamente
# o que uma régua não pode deixar acontecer por descuido de expressão regular.
D8 = re.compile(
    r"(?<![.$\w])(load|loadRelativeToScript|quit|read|readbuffer|readline|gc|Sandbox)\s*\("
    r"|\bd8\.|\bRealm\.|\bnew Worker\b")


def one(rel):
    path = TESTS / rel
    src = path.read_text(encoding="utf-8", errors="replace")
    group = rel.split("/")[0] if "/" in rel else "(raiz)"

    if NATIVE.search(src):
        return (group, rel, "skipped", "sintaxe nativa do V8 (%Native)")
    if D8.search(src):
        return (group, rel, "skipped", "precisa do shell d8")

    # `mjsunit.js` à frente e mais nada: é assim que o próprio V8 o corre, e é o
    # que faz o resultado ser sobre o teste em vez de sobre um tradutor nosso.
    program = (TESTS / "mjsunit.js").read_text(encoding="utf-8", errors="replace") + "\n" + src
    code, out = c.run_program(program, path.parent, path.name, c.rts_bin())

    if code is None:
        return (group, rel, "timeout", "")
    if code == 0:
        return (group, rel, "ok", "")
    first = c.first_line(out)
    # O arnês do V8 lança `MjsUnitAssertionError` quando a resposta está errada;
    # tudo o resto morreu antes de chegar a uma asserção.
    st = "fail" if "MjsUnitAssertionError" in out else "error"
    return (group, rel, st, first)


def collect(prefixes):
    files = []
    for p in sorted(TESTS.rglob("*.js")):
        rel = p.relative_to(TESTS).as_posix()
        # O próprio arnês e os ficheiros que ele carrega não são testes.
        if p.name in ("mjsunit.js", "mjsunit_numfuzz.js") or rel.startswith("tools/"):
            continue
        if prefixes and not any(rel.startswith(x) for x in prefixes):
            continue
        files.append(rel)
    return c.slice_of(files)


README = dict(
    marker="MJSUNIT",
    badge_label="V8 mjsunit",
    badge_href="scripts/mjsunit/README.md",
    heading="## 🧪 V8 `mjsunit` — as regressões de um motor de produção",
    intro=("O test262 mede o que a **norma exige**. Esta régua mede o que um motor de\n"
           "produção **aprendeu a não errar**: cada `regress-*.js` é um bug que alguém\n"
           "teve. Corre com o `mjsunit.js` do próprio V8 à frente, sem tradução."),
    group_label="Por diretório",
    footer="",
)


def main():
    if sys.argv[1:2] == ["--merge"]:
        rows = c.merge(sys.argv[2:])
    else:
        if not TESTS.is_dir():
            sys.exit("sem corpus: corra bash scripts/mjsunit/fetch.sh")
        rows = c.run_all(collect(sys.argv[1:]), one, ".mjsunit")

    sha = c.ROOT / ".mjsunit" / "SHA"
    README["footer"] = "V8 %s · %s" % (
        sha.read_text().strip()[:9] if sha.exists() else "?",
        "corpus inteiro" if c.STRIDE == 1 else
        "amostra determinista de 1 em %d" % c.STRIDE)
    c.report(rows, ".mjsunit", readme=README)


if __name__ == "__main__":
    main()
