"""O que é comum a qualquer corpus de outro motor corrido contra o rts.

Três arneses fazem a mesma coisa em volta de uma pergunta diferente: escolher os
ficheiros, dividi-los por máquinas, correr um processo por ficheiro, agregar as
causas e reescrever um bloco do README. O que muda entre eles é apenas **como se
constrói o programa a partir do ficheiro** e **como se lê o resultado** — e é só
isso que cada arnês escreve.

Isto existe porque a alternativa estava a começar: o segundo arnês seria uma
cópia de 250 linhas do primeiro, e a partir daí uma correção num deles é uma
correção que o outro não tem. É a mesma regra que o CLAUDE.md aplica a um
símbolo do runtime — uma fonte, várias vistas.
"""

import datetime
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
# sozinho são 9% do test262 e a máquina que o apanhasse decidia o tempo de todas
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
    r"(Test262Error|MjsUnitAssertionError|TypeError|RangeError|SyntaxError|"
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


def report(rows, label, readme=None):
    """A tabela por grupo, as causas, o JSON — e o bloco do README se pedido."""
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

    if readme and os.environ.get("UPDATE_README") == "1":
        # A contagem entra aqui e não no rodapé que cada arnês escreve, porque o
        # `--merge` não sabe o `STRIDE` com que as linhas foram medidas: o
        # ambiente da máquina que junta não é o da que correu. Um rodapé a dizer
        # "corpus inteiro" sobre uma amostra é o instrumento a afirmar o que não
        # sabe, e é o mesmo erro que o checkout incompleto — em texto.
        readme = dict(readme)
        readme["footer"] += " · %d ficheiros medidos" % (den + tot["skipped"])
        update_readme(tot, den, share(tot), by, top, **readme)
    return tot, den


def bar(pct, segs=20):
    filled = int(round(pct / (100.0 / segs)))
    return "▰" * filled + "▱" * (segs - filled)


def badge_color(pct):
    """De um NÚMERO e nunca da percentagem já formatada: `"5.0" >= 95` compara
    com coerção e sairia certa por acidente. A régua do Node tem a mesma nota."""
    return ("red" if pct < 30 else "orange" if pct < 50 else "yellow" if pct < 70
            else "yellowgreen" if pct < 85 else "green" if pct < 95 else "brightgreen")


def update_readme(tot, den, pct, by, causes, marker, badge_label, badge_href,
                  heading, intro, group_label, footer, rows_label="ficheiros",
                  ok_label="A norma ficou satisfeita"):
    """Reescreve o bloco em vez de o deixar escrever à mão.

    A régua cross-runtime já pagou o preço da alternativa: uma cópia do número no
    CLAUDE.md ficou obsoleta duas vezes, a segunda por dois pontos e meio. Um
    bloco gerado não pode discordar da sua fonte.
    """
    # Recusar um zero é a guarda que o badge de paridade aprendeu à sua custa: um
    # 0% quase nunca é o motor, é o instrumento — uma lib a faltar no runner e o
    # corpus inteiro falha por igual. Publicá-lo apagava o último número real.
    if tot["ok"] == 0:
        sys.exit("recusado: ok=0 — o instrumento, não o motor. O README fica como estava")

    # Uma barra por grupo, e não uma tabela: é a forma do bloco de CSS/DOM, e a
    # razão dela é a mesma — o total sozinho não diz ONDE está o trabalho, e uma
    # coluna de barras alinhadas responde a isso de relance. Ordenado por
    # tamanho do grupo porque um 0% sobre três ficheiros não é a mesma notícia
    # que um 0% sobre duzentos.
    top = sorted(by.items(), key=lambda kv: -(sum(kv[1].values()) - kv[1]["skipped"]))[:16]
    grouprows = "\n".join(
        "  [%s] %5.1f%%   %-9s %s" % (
            bar(share(d)), share(d),
            "%d/%d" % (d["ok"], sum(d.values()) - d["skipped"]), g)
        for g, d in top)
    causerows = "\n".join("| %d | `%s` |" % (n, m.replace("|", "\\|")) for m, n in causes[:10])

    block = """<!-- %(m)s_STATS_START -->
%(heading)s

%(intro)s

```
[%(bar)s] %(pct).1f%%   %(ok)d/%(den)d %(rows_label)s passando
```

| Metric | Value |
|---|---|
| **Conformidade** | **%(pct).1f%%** (%(ok)d/%(den)d) |
| ✅ %(ok_label)s | %(ok)d |
| ❌ Resposta errada | %(fail)d |
| 💥 Exceção não apanhada | %(error)d |
| ⏱️ Não terminou | %(timeout)d |
| ➖ Fora da conta | %(skipped)d |

### %(group_label)s

O total sozinho não diz onde está o trabalho: não distingue uma área que este
motor faz bem de uma que não tenta. Os dezasseis maiores grupos, por número de
ficheiros:

```
%(grouprows)s
```

**As causas mais frequentes** — uma mensagem repetida é **um** defeito, não N:

| Ficheiros | Mensagem |
|---|---|
%(causerows)s

_%(footer)s · %(hoje)s_

<!-- %(m)s_STATS_END -->""" % {
        "m": marker, "heading": heading, "intro": intro,
        "bar": bar(pct), "pct": pct, "ok": tot["ok"], "den": den,
        "rows_label": rows_label, "ok_label": ok_label, "fail": tot["fail"], "error": tot["error"],
        "timeout": tot["timeout"], "skipped": tot["skipped"],
        "group_label": group_label, "grouprows": grouprows, "causerows": causerows,
        "footer": footer, "hoje": datetime.date.today().isoformat(),
    }

    badge = ("<!-- %s_BADGE_START -->\n"
             "[![%s](https://img.shields.io/badge/%s-%.1f%%25-%s?style=flat-square)](%s)\n"
             "<!-- %s_BADGE_END -->" % (
                 marker, badge_label, badge_label.replace(" ", "%20").replace("(", "%28")
                 .replace(")", "%29").replace("-", "--"),
                 pct, badge_color(pct), badge_href, marker))

    path = ROOT / "README.md"
    txt = path.read_text(encoding="utf-8")
    if "<!-- %s_STATS_START -->" % marker not in txt:
        sys.exit("README.md não tem os marcadores %s" % marker)
    for start, end, new in ((marker + "_BADGE_START", marker + "_BADGE_END", badge),
                            (marker + "_STATS_START", marker + "_STATS_END", block)):
        txt = re.sub("<!-- %s -->.*?<!-- %s -->" % (start, end), lambda _: new, txt, flags=re.S)
    path.write_text(txt, encoding="utf-8")
    print("README.md: bloco %s reescrito" % marker)
