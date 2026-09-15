#!/usr/bin/env python3
"""Corre o test262 CONTRA O MOTOR, um processo por ficheiro.

`crates/rts-codegen/tests/test262.rs` faz a outra metade da pergunta: se o front
end LÊ cada programa como a norma diz. Nada lá corre. Aqui corre — e a distância
entre os dois números é exatamente "aceita o programa" menos "faz o que ele diz".

    python3 scripts/test262/run.py                    # tudo
    python3 scripts/test262/run.py built-ins/Array    # só um prefixo
    STRIDE=20 python3 scripts/test262/run.py          # amostra determinista
    SHARD=3/8 python3 scripts/test262/run.py          # uma fatia, para CI
    python3 scripts/test262/run.py --merge rows-*.tsv # o número, das fatias

`scripts/test262/README.md` diz como ler o número e o que fica fora dele.
O que é comum a todos os arneses está em `scripts/suites/common.py`.
"""

import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from suites import common as c  # noqa: E402

SUITE = c.ROOT / ".test262" / "test262"
TESTS = SUITE / "test"
HARNESS = SUITE / "harness"

FRONT = re.compile(r"/\*---(.*?)---\*/", re.S)
# Não é um leitor de YAML, e é deliberado: são cinco campos, todos de uma linha
# ou de uma lista em linha. O arnês de parsing tomou a mesma decisão.
HOST_INCLUDES = {"detachArrayBuffer.js", "atomicsHelper.js", "testAtomics.js"}


def meta_of(src):
    m = FRONT.search(src)
    body = m.group(1) if m else ""

    def listfield(name):
        mm = re.search(name + r":\s*\[(.*?)\]", body, re.S)
        if mm:
            return [x.strip().strip("'\"") for x in mm.group(1).split(",") if x.strip()]
        mm = re.search(name + r":\s*\n((?:\s*-\s*\S+\n?)+)", body)
        if mm:
            return [l.strip()[1:].strip() for l in mm.group(1).strip().splitlines()]
        return []

    neg = re.search(r"negative:\s*\n\s*phase:\s*(\S+)\s*\n\s*type:\s*(\S+)", body)
    return {
        "flags": listfield("flags"),
        "includes": listfield("includes"),
        "negative": (neg.group(1), neg.group(2)) if neg else None,
    }


def program_for(src, meta, strict):
    if "raw" in meta["flags"]:
        return src
    inc = ["assert.js", "sta.js"] + list(meta["includes"])
    if "async" in meta["flags"]:
        inc.append("doneprintHandle.js")
    return "".join(
        (['"use strict";\n'] if strict else [])
        + [(HARNESS / n).read_text(encoding="utf-8", errors="replace") for n in inc]
        # O `sta.js` define `Test262Error` sem lhe dar `name`, então uma asserção
        # falhada chega cá fora como `Error: …` e fica indistinguível de uma
        # exceção qualquer. A diferença é a que este arnês existe para reportar —
        # resposta ERRADA contra nome que não existe — e uma linha da própria
        # classe do test262 repõe-a.
        + ['\nTest262Error.prototype.name = "Test262Error";\n', src])


def verdict(meta, code, out):
    if code is None:
        return ("timeout", "")
    first = c.first_line(out)
    if meta["negative"]:
        phase, typ = meta["negative"]
        if code == 0:
            return ("fail", "aceite, devia falhar (%s %s)" % (phase, typ))
        # O tipo só é exigido quando o motor chega a nomeá-lo: um `SyntaxError`
        # de fase `parse` é recusado pelo front end, que tem mensagem própria.
        if phase != "parse" and typ not in out:
            return ("fail", "falhou com o erro errado, esperado %s: %s" % (typ, first))
        return ("ok", "")
    if "async" in meta["flags"]:
        if "Test262:AsyncTestComplete" in out:
            return ("ok", "")
        return ("fail" if "Test262:AsyncTestFailure" in out else "error", first)
    if code == 0:
        return ("ok", "")
    if "Test262Error" in out or "AssertionError" in out:
        return ("fail", first)
    return ("error", first)


def one(rel):
    path = TESTS / rel
    src = path.read_text(encoding="utf-8", errors="replace")
    meta = meta_of(src)
    group = "/".join(rel.split("/")[:2])

    # Fora do denominador, e só por isto: o ficheiro precisa do objeto de host
    # `$262` (`createRealm`, `detachArrayBuffer`, `evalScript`, `agent`), que não
    # é superfície da linguagem. Nada é excluído por causa da *feature* que usa —
    # um teste de `Temporal` conta como falha.
    if "CanBlockIsFalse" in meta["flags"] or set(meta["includes"]) & HOST_INCLUDES \
            or re.search(r"\$262\b", src):
        return (group, rel, "skipped", "precisa do objeto de host $262")

    # Um ficheiro sem `onlyStrict` nem `noStrict` corre DUAS VEZES e só conta
    # `ok` se as duas passarem. É o que a norma pede.
    if {"raw", "noStrict", "module"} & set(meta["flags"]):
        modes = [False]
    elif "onlyStrict" in meta["flags"]:
        modes = [True]
    else:
        modes = [False, True]

    binary = c.rts_bin()
    worst = ("ok", "")
    for strict in modes:
        code, out = c.run_program(
            program_for(src, meta, strict), path.parent, path.name, binary)
        st, detail = verdict(meta, code, out)
        worst = c.worse(worst, (st, ("[strict] " if strict else "") + detail))
    return (group, rel, worst[0], worst[1])


def collect(prefixes):
    files = []
    for p in sorted(TESTS.rglob("*.js")):
        rel = p.relative_to(TESTS).as_posix()
        # `_FIXTURE.js` é importado por um teste de módulo, não é um teste;
        # `staging/` é a antecâmara da suíte e não faz parte dela.
        if p.name.endswith("_FIXTURE.js") or rel.startswith("staging/"):
            continue
        if prefixes and not any(rel.startswith(x) for x in prefixes):
            continue
        files.append(rel)
    return c.slice_of(files)


README = dict(
    marker="TEST262",
    badge_label="test262 (executado)",
    badge_href="scripts/test262/README.md",
    heading="## 📏 test262 — a suíte da própria norma, EXECUTADA",
    intro=("`crates/rts-codegen/tests/test262.rs` pergunta se o front end **lê** cada\n"
           "programa como a norma diz. Esta régua pergunta a outra metade: se o motor **faz\n"
           "o que o programa manda**. Um processo por ficheiro, sloppy e strict, sem\n"
           "tradução nenhuma — o arnês do test262 corre como está."),
    group_label="Por área",
    footer="",
)


def main():
    if sys.argv[1:2] == ["--merge"]:
        rows = c.merge(sys.argv[2:])
    else:
        if not TESTS.is_dir():
            sys.exit("sem corpus: corra bash scripts/test262/fetch.sh")
        rows = c.run_all(collect(sys.argv[1:]), one, ".test262")

    sha = (SUITE.parent / "SHA")
    README["footer"] = "SHA %s · %s" % (
        sha.read_text().strip()[:9] if sha.exists() else "?",
        "corpus inteiro" if c.STRIDE == 1 else
        "amostra determinista de 1 em %d" % c.STRIDE)
    c.report(rows, ".test262", readme=README)


if __name__ == "__main__":
    main()
