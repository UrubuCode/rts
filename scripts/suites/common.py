"""O que é comum a qualquer corpus de outro motor corrido contra o rts.

Três arneses fazem a mesma coisa em volta de uma pergunta diferente: escolher os
ficheiros, dividi-los por máquinas, correr um processo por ficheiro, agregar as
causas e escrever o relatório. O que muda entre eles é apenas **como se
constrói o programa a partir do ficheiro** e **como se lê o resultado** — e é só
isso que cada arnês escreve.

Isto existe porque a alternativa estava a começar: o segundo arnês seria uma
cópia de 250 linhas do primeiro, e a partir daí uma correção num deles é uma
correção que o outro não tem. É a mesma regra que o CLAUDE.md aplica a um
símbolo do runtime — uma fonte, várias vistas.
"""

import json
import os
import re
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

STRIDE = int(os.environ.get("STRIDE", "1"))
# `SHARD=3/8` corre o terceiro oitavo. A divisão é por ÍNDICE na lista ordenada
# (`files[i::n]`) e não por diretório: por diretório, o `built-ins/Temporal`
# sozinho pesa quase um décimo de um corpus e a máquina que o apanhasse decidia o tempo de todas
# as outras. Intercalada, cada fatia leva a mesma mistura — e continua
# determinista, que é o que faz duas corridas serem comparáveis.
SHARD = os.environ.get("SHARD", "")
JOBS = int(os.environ.get("JOBS", str(max(1, (os.cpu_count() or 4) - 1))))
TIMEOUT = int(os.environ.get("TIMEOUT", "15"))

ORDER = {"ok": 0, "timeout": 1, "error": 2, "fail": 3}
COUNTED = ("ok", "fail", "error", "timeout")


def rts_bin():
    b = os.environ.get("RTS_BIN") or str(ROOT / "target" / "release" / "rts")
    if not os.access(b, os.X_OK):
        b2 = str(ROOT / "target" / "release" / "rts.exe")
        if os.access(b2, os.X_OK):
            return b2
        sys.exit("sem binário: %s (cargo build --release, ou RTS_BIN=...)" % b)
    return b


def slice_of(files):
    """A amostra e a fatia, nesta ordem, sobre uma lista JÁ ordenada."""
    if STRIDE > 1:
        files = files[::STRIDE]
    if SHARD:
        i, n = (int(x) for x in SHARD.split("/"))
        if not 1 <= i <= n:
            sys.exit("SHARD=i/n com 1 <= i <= n")
        files = files[i - 1::n]
    return files


def run_program(program, workdir, name, binary):
    """Corre um programa já montado. Devolve (returncode, saída junta).

    Escrito AO LADO do ficheiro de origem porque um teste que importa caminhos
    relativos resolve-os a partir do lugar dele; um diretório temporário mediria
    a nossa escolha de diretório em vez do teste.
    """
    tmp = Path(workdir) / ("__rts_suite_%d_%s" % (os.getpid(), name))
    try:
        tmp.write_text(program, encoding="utf-8")
        try:
            p = subprocess.run(
                [binary, "run", str(tmp)],
                capture_output=True, text=True, timeout=TIMEOUT, cwd=str(workdir),
                env={**os.environ, "NO_COLOR": "1", "FORCE_COLOR": "0"},
            )
        except subprocess.TimeoutExpired:
            return (None, "")
    finally:
        tmp.unlink(missing_ok=True)
    return (p.returncode, (p.stdout or "") + (p.stderr or ""))


def first_line(out):
    return " ".join(out.split())[:200]


def worse(a, b):
    return a if ORDER[a[0]] >= ORDER[b[0]] else b


# As causas, e não as falhas. Uma mensagem repetida 300 vezes é um nome que
# falta, não 300 problemas — e é a única parte do relatório que se usa para
# decidir o que fazer a seguir. O que esta lista apaga da mensagem é o que varia
# SEM mudar a causa: o número, o caminho, e os valores que o `assert` interpolou.
NOISE = [
    (re.compile(r"«[^»]*»"), "«…»"),
    (re.compile(r"/\S*?/[^\s'\"]*"), "<caminho>"),
    (re.compile(r"\b\d+\b"), "N"),
]
ERRNAME = re.compile(
    r"(MjsUnitAssertionError|TypeError|RangeError|SyntaxError|"
    r"ReferenceError|error: \w+|Error)\b.*")


def cause_of(detail):
    m = ERRNAME.search(detail)
    d = m.group(0) if m else detail
    for rx, rep in NOISE:
        d = rx.sub(rep, d)
    return d.strip()[:110]


def run_all(files, one, label):
    print("%d ficheiros, %d trabalhos, stride %d%s" % (
        len(files), JOBS, STRIDE, (", fatia " + SHARD) if SHARD else ""), flush=True)
    rows_path = Path(os.environ.get("ROWS", ROOT / label / "rows.tsv"))
    rows_path.parent.mkdir(parents=True, exist_ok=True)
    rows = []
    with rows_path.open("w", encoding="utf-8") as fh, ProcessPoolExecutor(JOBS) as ex:
        for i, row in enumerate(ex.map(one, files, chunksize=8), 1):
            rows.append(row)
            fh.write("\t".join(row) + "\n")
            if i % 200 == 0:
                fh.flush()
                print("  %d/%d" % (i, len(files)), file=sys.stderr, flush=True)
    return rows


def merge(paths):
    """Junta as linhas de várias fatias.

    Uma percentagem por fatia não é uma percentagem de nada — o denominador dela
    é a fatia — portanto o número só existe aqui. Deduplicado por caminho porque
    um `re-run failed jobs` traz a mesma fatia outra vez.
    """
    rows, seen = [], set()
    for p in paths:
        for line in Path(p).read_text(encoding="utf-8").splitlines():
            row = tuple((line.split("\t", 3) + ["", "", "", ""])[:4])
            if row[1] in seen:
                continue
            seen.add(row[1])
            rows.append(row)
    print("%d ficheiros de %d fatias" % (len(rows), len(paths)))
    return rows


def share(d):
    den = sum(d[k] for k in COUNTED)
    return (100.0 * d["ok"] / den) if den else 0.0


def report(rows, label):
    """A tabela por grupo, as causas e o JSON. Não escreve no README: um nome de
    suíte com uma percentagem ao lado lê-se como um resultado DELA, e é a
    condição de não-endosso que `THIRD-PARTY-NOTICES.md` guarda. O número vive no
    relatório, que é onde se planeia trabalho a partir dele.
    """
    by, tot = {}, dict.fromkeys(COUNTED + ("skipped",), 0)
    for group, _, st, _ in rows:
        by.setdefault(group, dict.fromkeys(COUNTED + ("skipped",), 0))[st] += 1
        tot[st] += 1

    print("\n%-42s %6s %6s %6s %6s %6s  %s" % ("grupo", "ok", "fail", "erro", "t/o", "skip", "%"))
    for g in sorted(by, key=lambda g: -by[g]["ok"]):
        d = by[g]
        print("%-42s %6d %6d %6d %6d %6d  %5.1f%%" % (
            g, d["ok"], d["fail"], d["error"], d["timeout"], d["skipped"], share(d)))

    causes = {}
    for _, _, st, detail in rows:
        if st in ("fail", "error") and detail:
            c = cause_of(detail)
            causes[c] = causes.get(c, 0) + 1
    top = sorted(causes.items(), key=lambda kv: -kv[1])[:15]
    if top:
        print("\nas causas mais frequentes — uma mensagem repetida é UM defeito\n")
        for msg, n in top:
            print("%6d  %s" % (n, msg))

    den = sum(tot[k] for k in COUNTED)
    print("\nTOTAL %d/%d = %.1f%%  (%d skipped, fora do denominador)" % (
        tot["ok"], den, share(tot), tot["skipped"]))

    rep = Path(os.environ.get("REPORT", ROOT / label / "report.json"))
    if str(rep) != "/dev/null":
        rep.parent.mkdir(parents=True, exist_ok=True)
        rep.write_text(json.dumps({
            "stride": STRIDE, "total": tot, "passed": tot["ok"], "denominator": den,
            "share": round(share(tot), 2), "by_group": by,
            "causes": [{"n": n, "message": m} for m, n in top],
        }, indent=2), encoding="utf-8")
        print("relatório: %s" % rep)

    return tot, den
