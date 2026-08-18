#!/usr/bin/env python3
"""Our CSS surface against the canonical property list of a real browser.

FERRAMENTA DE ANALISE LOCAL — NAO faz parte da build nem de nenhum gate.

Le a lista canonica de propriedades do Blink (`css_properties.json5`) de uma
ARVORE LOCAL do Chromium que NAO pertence a este repositorio, e cruza-a com:

  * os nomes que o nosso parser reconhece (derivados de
    `crates/rts-dom/src/style/parse.rs`, via `css_coverage.supported()`);
  * a contagem de uso numa folha real (via `css_coverage.declarations()`).

O ficheiro do Chromium e' apenas LIDO. Nada dele e' copiado para dentro deste
repositorio: o que sai daqui sao NOMES de propriedades CSS (que sao a norma
publica do W3C, nao codigo) e contagens nossas. A conclusao escrita vive em
`docs/ui/css-support.md` e e' texto nosso.

O caminho NUNCA esta' fixo no codigo. Ordem: 1.o argumento, ou a variavel de
ambiente BLINK_CSS_JSON5. Sem nenhum dos dois, o script explica-se e sai — este
repositorio tem de continuar a funcionar numa maquina que nao tenha o Chromium.

Uso:
    python scripts/css_blink_gap.py <caminho/css_properties.json5> [folha.css ...]
    BLINK_CSS_JSON5=... python scripts/css_blink_gap.py -- pagina.css
"""

import os
import re
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from css_coverage import declarations, supported  # noqa: E402

FIELD = re.compile(r"^\s*([a-z_]+):\s*(.*?),?\s*$", re.M)


def top_level_objects(body: str):
    """Os objetos de PRIMEIRO nivel de `data: [...]`, por contagem de chavetas.

    A primeira versao disto partia por indentacao (`^    {` ate `^    },`) e
    ENGOLIA entradas: bastava um objeto aninhado para o fecho nao casar, e as
    entradas seguintes iam todas para dentro da mesma — 682 propriedades em vez
    de 915, com `width` e `margin-top` a aparecerem como "nao existem no Blink".
    Um erro que se ve porque o resultado era absurdo; um mais subtil teria
    passado. Contar chavetas (fora de strings e de comentarios `//`) nao tem
    esse modo de falha.
    """
    depth = 0
    start = 0
    in_str = None
    i = 0
    while i < len(body):
        ch = body[i]
        if in_str:
            if ch == "\\":
                i += 2
                continue
            if ch == in_str:
                in_str = None
        elif ch in "\"'":
            in_str = ch
        elif ch == "/" and body[i : i + 2] == "//":
            i = body.find("\n", i)
            if i < 0:
                break
            continue
        elif ch == "{":
            if depth == 0:
                start = i + 1
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                yield body[start:i]
            elif depth < 0:
                return
        elif ch == "]" and depth == 0:
            return
        i += 1


def blink_properties(path: Path) -> dict[str, dict]:
    src = path.read_text(encoding="utf-8", errors="replace")
    body = src[src.index("\n  data: [") + len("\n  data: ["):]
    out: dict[str, dict] = {}
    for obj in top_level_objects(body):
        # PRIMEIRA ocorrencia de cada campo, nao a ultima: os campos de um
        # objeto ANINHADO estao dentro do mesmo texto e um deles COLIDE — o
        # `logical_property_group: { name: "size" }` de `width` fazia a
        # propriedade chamar-se "size", e `width`, `height` e `margin-top`
        # apareciam como "nao existem no Blink". Absurdo visivel; e o motivo de
        # a lista de "nomes nossos fora do Blink" estar impressa.
        fields: dict[str, str] = {}
        for k, v in FIELD.findall(obj):
            fields.setdefault(k, v)
        name = fields.get("name", "").strip().strip('"')
        if not name:
            continue
        out[name] = {
            "inherited": fields.get("inherited", "false").startswith("true"),
            "runtime_flag": fields.get("runtime_flag", "").strip('"'),
            "alias_for": fields.get("alias_for", "").strip('"'),
            "longhands": fields.get("longhands", ""),
            # `is_property: false` marca um DESCRITOR de at-rule (`src` do
            # @font-face), que nao e' uma propriedade e nao conta como buraco.
            "is_property": not fields.get("is_property", "true").startswith("false"),
            "is_descriptor": fields.get("is_descriptor", "false").startswith("true"),
        }
    return out


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--"]
    path = None
    if args and args[0].endswith(".json5"):
        path = Path(args.pop(0))
    elif os.environ.get("BLINK_CSS_JSON5"):
        path = Path(os.environ["BLINK_CSS_JSON5"])
    if path is None or not path.is_file():
        print(
            "css_blink_gap: preciso do `css_properties.json5` do Blink.\n"
            "  Passe o caminho como 1.o argumento ou em BLINK_CSS_JSON5.\n"
            "  E' uma arvore LOCAL do Chromium, externa a este repositorio;\n"
            "  este script e' de analise e nada aqui depende dele.\n"
            f"  (tentei: {path})",
            file=sys.stderr,
        )
        return 2

    blink = blink_properties(path)
    ours = supported()

    real = {n: d for n, d in blink.items() if d["is_property"] and not d["alias_for"]}
    stable = {n: d for n, d in real.items() if not d["runtime_flag"]}
    prefixed = {n for n in stable if n.startswith("-")}
    web = {n: d for n, d in stable.items() if n not in prefixed}

    print(f"# fonte: {path}")
    print(f"entradas no ficheiro            : {len(blink)}")
    print(f"  propriedades (nao descritores): {len([n for n in blink if blink[n]['is_property']])}")
    print(f"  sem alias                     : {len(real)}")
    print(f"  sem runtime_flag (estaveis)   : {len(stable)}")
    print(f"  estaveis e nao prefixadas     : {len(web)}")
    print()
    have = sorted(n for n in web if n in ours)
    print(f"NOS reconhecemos                : {len(have)} de {len(web)} ({len(have)*100//len(web)}%)")
    extra = sorted(n for n in ours if n not in blink)
    if extra:
        print(f"  nomes nossos fora do Blink    : {', '.join(extra)}")

    if not args:
        return 0

    total: Counter = Counter()
    for f in args:
        total += declarations(Path(f).read_text(encoding="utf-8", errors="replace"))
    used = {p: n for p, n in total.items() if p in web}
    miss = sorted(((n, p) for p, n in used.items() if p not in ours), reverse=True)
    print()
    print(f"# folhas: {', '.join(args)}")
    print(f"propriedades do Blink usadas    : {len(used)} de {len(web)}")
    print(f"  reconhecidas por nos          : {len(used) - len(miss)}")
    print(f"  em falta                      : {len(miss)}")
    print()
    print(f"{'ocorr':>6}  {'herda':<6} propriedade")
    for n, p in miss:
        print(f"{n:>6}  {'sim' if web[p]['inherited'] else '':<6} {p}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
