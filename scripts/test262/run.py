#!/usr/bin/env python3
"""Corre o test262 CONTRA O MOTOR, um processo por ficheiro.

`crates/rts-codegen/tests/test262.rs` faz a outra metade da pergunta: se o
front end LE cada programa como a norma diz. Nada lá corre. Aqui corre — e a
distância entre os dois números é exatamente "aceita o programa" menos "faz o
que o programa manda".

O corpus é lido, nunca carregado: `fetch.sh` clona um SHA fixo para `.test262/`,
que está no `.gitignore`. Nenhum ficheiro da Ecma entra neste repositório, pela
razão que o arnês de parsing já documenta.

    python3 scripts/test262/run.py                    # tudo
    python3 scripts/test262/run.py built-ins/Array    # só um prefixo
    STRIDE=20 python3 scripts/test262/run.py          # amostra determinista

Como cada ficheiro é contado, e porquê:

  ok        correu e a norma ficou satisfeita — inclui o negativo que falhou
            como devia, e o assíncrono que imprimiu o `AsyncTestComplete`
  fail      correu e deu resposta ERRADA: uma asserção do test262 falhou, ou
            um programa inválido foi aceite
  error     exceção não apanhada antes de qualquer asserção — quase sempre um
            nome que não existe. Conta como falha na mesma: a norma exige-o
  timeout   não terminou
  skipped   FORA do denominador, e só por duas razões, ambas sobre o ARNÊS e
            não sobre nós: o ficheiro precisa do objeto de host `$262`
            (`createRealm`, `detachArrayBuffer`, `evalScript`, `agent`), que
            não é superfície da linguagem, ou de `CanBlockIsFalse`.

Excluir por causa da `feature` que o teste usa seria escolher o corpus depois
de ver o resultado, e é por isso que um `Temporal` ou um `decorators` entra
como `error` e não como `skipped`.

Um ficheiro sem `onlyStrict` nem `noStrict` corre DUAS VEZES — sloppy e strict —
e só conta `ok` se as duas passarem. É o que a norma pede e é metade do custo
da corrida; `STRIDE` existe para isso, não para melhorar o número.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SUITE = ROOT / ".test262" / "test262"
TESTS = SUITE / "test"
HARNESS = SUITE / "harness"

RTS = os.environ.get("RTS_BIN") or str(ROOT / "target" / "release" / "rts")
TIMEOUT = int(os.environ.get("TIMEOUT", "15"))
JOBS = int(os.environ.get("JOBS", str(max(1, (os.cpu_count() or 4) - 1))))
STRIDE = int(os.environ.get("STRIDE", "1"))
# `SHARD=3/8` corre o terceiro oitavo. A divisao e por INDICE na lista ordenada
# (`files[i::n]`) e nao por diretorio: por diretorio, o `built-ins/Temporal`
# sozinho sao 9% do corpus e a maquina que o apanhasse decidia o tempo de todas
# as outras. Intercalada, cada fatia tem a mesma mistura de areas — e continua
# determinista, que e o que faz duas corridas serem comparaveis.
SHARD = os.environ.get("SHARD", "")
ROWS = Path(os.environ.get("ROWS", ROOT / ".test262" / "rows.tsv"))
REPORT = Path(os.environ.get("REPORT", ROOT / ".test262" / "report.json"))

FRONT = re.compile(r"/\*---(.*?)---\*/", re.S)
# Não é um leitor de YAML, e é deliberado: são cinco campos, todos de uma linha
# ou de uma lista em linha. O arnês de parsing tomou a mesma decisão.
LIST = re.compile(r"\[(.*?)\]", re.S)

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
    neg = None
    mm = re.search(r"negative:\s*\n\s*phase:\s*(\S+)\s*\n\s*type:\s*(\S+)", body)
    if mm:
        neg = (mm.group(1), mm.group(2))
    return {
        "flags": listfield("flags"),
        "includes": listfield("includes"),
        "features": listfield("features"),
        "negative": neg,
    }


def read_harness(names):
    out = []
    for n in names:
        out.append((HARNESS / n).read_text(encoding="utf-8", errors="replace"))
    return "".join(out)


def run_once(path, src, meta, strict):
    """Uma execução. Devolve (status, detalhe)."""
    raw = "raw" in meta["flags"]
    parts = []
    if strict:
        parts.append('"use strict";\n')
    if not raw:
        inc = ["assert.js", "sta.js"] + list(meta["includes"])
        if "async" in meta["flags"]:
            inc.append("doneprintHandle.js")
        parts.append(read_harness(inc))
        # O `sta.js` define `Test262Error` sem lhe dar `name`, entao uma
        # assercao falhada chega ca fora como `Error: …` e fica
        # indistinguivel de uma excecao qualquer. A diferenca e a que este
        # arnes existe para reportar — resposta ERRADA contra nome que nao
        # existe — e uma linha da propria classe do test262 repoe-a.
        parts.append('\nTest262Error.prototype.name = "Test262Error";\n')
    parts.append(src)
    program = "".join(parts)

    # Escrito AO LADO do próprio teste porque um ficheiro de módulo importa
    # caminhos relativos ao lugar dele; um diretório temporário mediria a nossa
    # escolha de diretório em vez do teste. O nome tem prefixo próprio e é
    # apagado a seguir.
    tmp = path.parent / ("__rts262_%d_%s" % (os.getpid(), path.name))
    try:
        tmp.write_text(program, encoding="utf-8")
        try:
            p = subprocess.run(
                [RTS, "run", str(tmp)],
                capture_output=True, text=True, timeout=TIMEOUT,
                cwd=str(path.parent),
                env={**os.environ, "NO_COLOR": "1", "FORCE_COLOR": "0"},
            )
        except subprocess.TimeoutExpired:
            return ("timeout", "")
    finally:
        tmp.unlink(missing_ok=True)

    out = (p.stdout or "") + (p.stderr or "")
    first = " ".join(out.split())[:200]

    if meta["negative"]:
        phase, typ = meta["negative"]
        if p.returncode == 0:
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

    if p.returncode == 0:
        return ("ok", "")
    if "Test262Error" in out or "AssertionError" in out:
        return ("fail", first)
    return ("error", first)


def one(rel):
    path = TESTS / rel
    src = path.read_text(encoding="utf-8", errors="replace")
    meta = meta_of(src)
    area = "/".join(rel.split("/")[:2])

    if "CanBlockIsFalse" in meta["flags"] or set(meta["includes"]) & HOST_INCLUDES \
            or re.search(r"\$262\b", src):
        return (area, rel, "skipped", "precisa do objeto de host $262")

    modes = []
    if "raw" in meta["flags"] or "noStrict" in meta["flags"] or "module" in meta["flags"]:
        modes = [False]
    elif "onlyStrict" in meta["flags"]:
        modes = [True]
    else:
        modes = [False, True]

    worst = ("ok", "")
    order = {"ok": 0, "fail": 3, "error": 2, "timeout": 1}
    for strict in modes:
        st, detail = run_once(path, src, meta, strict)
        if order[st] > order[worst[0]]:
            worst = (st, ("[strict] " if strict else "") + detail)
    return (area, rel, worst[0], worst[1])


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
    if STRIDE > 1:
        files = files[::STRIDE]
    if SHARD:
        i, n = (int(x) for x in SHARD.split("/"))
        if not 1 <= i <= n:
            sys.exit("SHARD=i/n com 1 <= i <= n")
        files = files[i - 1::n]
    return files


def merge(paths):
    """Junta as linhas de varias fatias e produz UM relatorio.

    Cada fatia escreve o seu `rows.tsv` e mais nada: uma percentagem por fatia
    nao e uma percentagem de nada, porque o denominador dela e arbitrario. O
    numero so existe depois de estarem todas.
    """
    rows, seen = [], set()
    for p in paths:
        for line in Path(p).read_text(encoding="utf-8").splitlines():
            row = tuple((line.split("\t", 3) + ["", "", "", ""])[:4])
            # Uma fatia repetida (um `re-run failed jobs`) traria o mesmo
            # ficheiro duas vezes e contava-o duas vezes.
            if row[1] in seen:
                continue
            seen.add(row[1])
            rows.append(row)
    print("%d ficheiros de %d fatias" % (len(rows), len(paths)))
    report(rows)


def main():
    if sys.argv[1:2] == ["--merge"]:
        return merge(sys.argv[2:])
    if not TESTS.is_dir():
        sys.exit("sem corpus: corra bash scripts/test262/fetch.sh")
    if not os.access(RTS, os.X_OK):
        sys.exit("sem binário: %s (cargo build --release, ou RTS_BIN=...)" % RTS)

    files = collect(sys.argv[1:])
    print("%d ficheiros, %d trabalhos, stride %d" % (len(files), JOBS, STRIDE), flush=True)

    ROWS.parent.mkdir(parents=True, exist_ok=True)
    rows = []
    with ROWS.open("w", encoding="utf-8") as fh, ProcessPoolExecutor(JOBS) as ex:
        for i, row in enumerate(ex.map(one, files, chunksize=8), 1):
            rows.append(row)
            fh.write("\t".join(row) + "\n")
            if i % 200 == 0:
                fh.flush()
                print("  %d/%d" % (i, len(files)), file=sys.stderr, flush=True)

    report(rows)


# As causas, e nao as falhas. Uma mensagem repetida 300 vezes e um nome que
# falta, nao 300 problemas — e a tabela que diz isso e a unica parte do
# relatorio que se usa para decidir o que fazer a seguir. O que esta lista
# apaga da mensagem e o que varia SEM mudar a causa: o numero, o caminho do
# ficheiro, e os valores que o `assert` interpolou.
NOISE = [
    (re.compile(r"«[^»]*»"), "«…»"),
    (re.compile(r"/\S*?/[^\s'\"]*"), "<caminho>"),
    (re.compile(r"\b\d+\b"), "N"),
]


def cause_of(detail):
    """A mensagem reduzida ao que nela e a causa."""
    d = detail
    # A partir do nome do erro: o prefixo do processo (`rts: uncaught
    # exception (tag 1):`) e o mesmo para tudo e nao distingue nada.
    m = re.search(r"(Test262Error|TypeError|RangeError|SyntaxError|ReferenceError|"
                  r"error: \w+|Error)\b.*", d)
    if m:
        d = m.group(0)
    for rx, rep in NOISE:
        d = rx.sub(rep, d)
    return d.strip()[:110]


def report(rows):
    by = {}
    tot = {"ok": 0, "fail": 0, "error": 0, "timeout": 0, "skipped": 0}
    for area, rel, st, _ in rows:
        a = by.setdefault(area, {"ok": 0, "fail": 0, "error": 0, "timeout": 0, "skipped": 0})
        a[st] += 1
        tot[st] += 1

    def share(d):
        den = d["ok"] + d["fail"] + d["error"] + d["timeout"]
        return (100.0 * d["ok"] / den) if den else 0.0

    print("\n%-42s %6s %6s %6s %6s %6s  %s" % ("área", "ok", "fail", "erro", "t/o", "skip", "%"))
    for area in sorted(by, key=lambda a: -by[a]["ok"]):
        d = by[area]
        print("%-42s %6d %6d %6d %6d %6d  %5.1f%%" % (
            area, d["ok"], d["fail"], d["error"], d["timeout"], d["skipped"], share(d)))
    causes = {}
    for _, _, st, detail in rows:
        if st in ("fail", "error") and detail:
            causes[cause_of(detail)] = causes.get(cause_of(detail), 0) + 1
    top = sorted(causes.items(), key=lambda kv: -kv[1])[:15]
    if top:
        print("\nas causas mais frequentes — uma mensagem repetida e UM defeito\n")
        for msg, n in top:
            print("%6d  %s" % (n, msg))

    den = tot["ok"] + tot["fail"] + tot["error"] + tot["timeout"]
    print("\nTOTAL %d/%d = %.1f%%  (%d skipped, fora do denominador)" % (
        tot["ok"], den, share(tot), tot["skipped"]))

    REPORT.write_text(json.dumps({
        "sha": (SUITE.parent / "SHA").read_text().strip() if (SUITE.parent / "SHA").exists() else None,
        "stride": STRIDE,
        "total": tot,
        "passed": tot["ok"],
        "denominator": den,
        "share": round(share(tot), 2),
        "by_area": by,
        "causes": [{"n": n, "message": m} for m, n in top],
    }, indent=2), encoding="utf-8")
    print("relatório: %s\nlinhas:    %s" % (REPORT, ROWS))

    if os.environ.get("UPDATE_README") == "1":
        update_readme(tot, den, share(tot), by, top)


def update_readme(tot, den, pct, by, causes):
    """Reescreve o bloco do README em vez de o deixar escrever à mão.

    A régua cross-runtime já pagou o preço da alternativa: uma cópia do número
    no CLAUDE.md ficou obsoleta duas vezes, a segunda por dois pontos e meio.
    Um bloco gerado não pode discordar da sua fonte.
    """
    filled = int(round(pct / 5.0))
    bar = "▰" * filled + "▱" * (20 - filled)
    top = sorted(by.items(), key=lambda kv: -(sum(kv[1].values()) - kv[1]["skipped"]))[:10]
    rows = "\n".join(
        "| `%s` | **%.1f%%** | %d/%d |" % (
            a, 100.0 * d["ok"] / max(1, sum(d.values()) - d["skipped"]),
            d["ok"], sum(d.values()) - d["skipped"])
        for a, d in top)
    causerows = "\n".join("| %d | `%s` |" % (n, m.replace("|", "\\|")) for m, n in causes[:10])
    sha = (SUITE.parent / "SHA").read_text().strip()[:9] if (SUITE.parent / "SHA").exists() else "?"
    amostra = "corpus inteiro" if STRIDE == 1 else (
        "amostra determinista de 1 em %d — os mesmos ficheiros em cada corrida" % STRIDE)
    block = """<!-- TEST262_STATS_START -->
## 📏 test262 — a suíte da própria norma, EXECUTADA

`crates/rts-codegen/tests/test262.rs` pergunta se o front end **lê** cada
programa como a norma diz. Esta régua pergunta a outra metade: se o motor **faz
o que o programa manda**. Um processo por ficheiro, sloppy e strict, sem
tradução nenhuma — o arnês do test262 corre como está.

```
[%s] %.1f%%   %d/%d ficheiros passando
```

| Metric | Value |
|---|---|
| **Conformidade** | **%.1f%%** (%d/%d) |
| ✅ A norma ficou satisfeita | %d |
| ❌ Resposta errada | %d |
| 💥 Exceção não apanhada | %d |
| ⏱️ Não terminou | %d |
| ➖ Fora da conta (host `$262`) | %d |

**Por área** (as dez maiores):

| Área | %% | ok/total |
|---|---|---|
%s

**As causas mais frequentes** — uma mensagem repetida é **um** defeito, não N:

| Ficheiros | Mensagem |
|---|---|
%s

_SHA %s · %s · %s_

<!-- TEST262_STATS_END -->""" % (
        bar, pct, tot["ok"], den, pct, tot["ok"], den,
        tot["ok"], tot["fail"], tot["error"], tot["timeout"], tot["skipped"],
        rows, causerows, sha, amostra, __import__("datetime").date.today().isoformat())

    # A cor sai de um NÚMERO e nunca da percentagem já formatada: `"5.0" >= 95`
    # compara com coerção, e sairia certa por acidente. A régua do Node tem a
    # mesma nota, pela mesma razão.
    color = ("red" if pct < 30 else "orange" if pct < 50 else "yellow" if pct < 70
             else "yellowgreen" if pct < 85 else "green" if pct < 95 else "brightgreen")
    badge = ("<!-- TEST262_BADGE_START -->\n"
             "[![test262](https://img.shields.io/badge/test262%%20(executado)-%.1f%%25-%s"
             "?style=flat-square)](scripts/test262/README.md)\n"
             "<!-- TEST262_BADGE_END -->" % (pct, color))

    readme = ROOT / "README.md"
    txt = readme.read_text(encoding="utf-8")
    if "<!-- TEST262_STATS_START -->" not in txt:
        sys.exit("README.md não tem os marcadores TEST262_STATS")

    # Recusar um zero é a guarda que o badge de paridade aprendeu à sua custa:
    # um 0% quase nunca é o motor, é o instrumento — uma lib a faltar no runner
    # e os 48 000 ficheiros falham por igual. Publicá-lo apagaria o último
    # número verdadeiro.
    if tot["ok"] == 0:
        sys.exit("recusado: ok=0 — o instrumento, não o motor. O README fica como estava")

    txt = re.sub(r"<!-- TEST262_BADGE_START -->.*?<!-- TEST262_BADGE_END -->",
                 lambda _: badge, txt, flags=re.S)
    txt = re.sub(r"<!-- TEST262_STATS_START -->.*?<!-- TEST262_STATS_END -->",
                 lambda _: block, txt, flags=re.S)
    readme.write_text(txt, encoding="utf-8")
    print("README.md: bloco TEST262_STATS reescrito")


if __name__ == "__main__":
    main()
