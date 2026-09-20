#!/usr/bin/env python3
"""Corre o `JSTests/stress` do JavaScriptCore contra o rts.

A terceira das réguas importadas, e a que pergunta o que as outras duas não
perguntam. O test262 mede o que a norma EXIGE; o `mjsunit` mede o que o V8
aprendeu a não errar; este mede o mesmo para o motor do Safari — e a
sobreposição entre os dois últimos é pequena, porque um bug de motor é
descoberto por quem o tem.

    python3 scripts/jsc/run.py                  # tudo
    SHARD=3/8 python3 scripts/jsc/run.py        # uma fatia, para CI
    python3 scripts/jsc/run.py --merge rows-*.tsv
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from suites import common as c  # noqa: E402

TESTS = c.ROOT / ".jsc" / "webkit" / "JSTests" / "stress"

# O interior do JSC: `$vm` é a janela para a máquina virtual, e
# `createGlobalObject`/`runString` criam realms do shell. Fora do denominador
# pelo mesmo argumento que põe lá o `%Native` do V8 e o `$262` do test262.
HOST = re.compile(r"\$vm\b|(?<![\w])\$\.(agent|globalObjectFor|evalScript)"
                  r"|(?<![.$\w])(createGlobalObject|runString|transferArrayBuffer"
                  r"|loadString|readFile|checkModuleSyntax|gc)\s*\(")

# Estas NÃO ficam de fora, e a diferença vale escrita. `noInline(f)` diz ao JIT
# do JSC para não inlinar aquela função — é uma pista de compilação, e um motor
# que não a atende continua a computar exatamente o mesmo programa. Um corpo
# vazio é a resposta honesta: não é traduzir o teste, é dar-lhe o que o nome
# promete, que é "não inlines" — e não inlinar é algo que qualquer motor pode
# fazer. Contrasta com `$vm`, que se fosse falsificado responderia mentira sobre
# o estado da máquina.
SHIM = """\
function noInline() {}
function noDFG() {}
function noFTL() {}
function ensureArrayStorage() {}
function OSRExit() {}
function noOSRExitFuzzing() {}
function fiatInt52(x) { return x; }
"""


def one(rel):
    path = TESTS / rel
    src = path.read_text(encoding="utf-8", errors="replace")
    group = rel.split("-")[0][:18] if "-" in rel else "(outros)"

    if HOST.search(src):
        return (group, rel, "skipped", "precisa do interior do JSC ($vm, realms)")

    code, out = c.run_program(SHIM + src, path.parent, path.name, c.rts_bin())
    if code is None:
        return (group, rel, "timeout", "")
    if code == 0:
        return (group, rel, "ok", "")
    first = c.first_line(out)
    # Cada ficheiro do `stress` traz o seu próprio `shouldBe`/`shouldThrow`, que
    # lança um `Error` com "Bad value" ou "Expected". Não há arnês comum, e é
    # por isso que a leitura do resultado é mais grosseira aqui do que nas
    # outras duas réguas: o que se distingue é resposta errada de morte cedo.
    st = "fail" if re.search(r"Bad value|Expected|bad value|assertion", out) else "error"
    return (group, rel, st, first)


def collect(prefixes):
    files = []
    for p in sorted(TESTS.rglob("*.js")):
        rel = p.relative_to(TESTS).as_posix()
        if prefixes and not any(rel.startswith(x) for x in prefixes):
            continue
        files.append(rel)
    return c.slice_of(files)


README = dict(
    marker="JSC",
    heading="## 🧯 JavaScriptCore `stress` — as regressões do motor do Safari",
    intro=("A mesma pergunta que o `mjsunit` faz, a outro motor de produção. A\n"
           "sobreposição é pequena de propósito: um bug de motor é descoberto por quem\n"
           "o tem, e o que o JSC aprendeu não é o que o V8 aprendeu."),
    ok_label="Passou — saiu com 0",
    group_label="Por prefixo do ficheiro",
    footer="",
)


def main():
    if sys.argv[1:2] == ["--merge"]:
        rows = c.merge(sys.argv[2:])
    else:
        if not TESTS.is_dir():
            sys.exit("sem corpus: corra bash scripts/jsc/fetch.sh")
        rows = c.run_all(collect(sys.argv[1:]), one, ".jsc")

    sha = c.ROOT / ".jsc" / "SHA"
    README["footer"] = "WebKit %s · %s" % (
        sha.read_text().strip()[:9] if sha.exists() else "?",
        "corpus inteiro" if c.STRIDE == 1 else "amostra determinista de 1 em %d" % c.STRIDE)
    c.report(rows, ".jsc", readme=README)


if __name__ == "__main__":
    main()
