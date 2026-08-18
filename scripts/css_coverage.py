#!/usr/bin/env python3
"""Coverage of the CSS engine against a real stylesheet.

WHAT IT ANSWERS: of the distinct declaration properties a real sheet uses, how
many does `crates/rts-dom/src/style/parse.rs` recognise by name, and which of
the unrecognised ones cost the most (occurrence count).

WHY IT READS parse.rs AND NOT A HAND LIST: a hand list is a second source of
"what is supported" and would drift the first time a property lands. The set of
supported names is derived from the match arms of `apply_decl`, which is the
only place a CSS name becomes a field.

IT IS A LOWER BOUND. Some names are matched by a PREDICATE and not by a literal
(`_ if borders::is_longhand(&prop)` covers the twelve `border-<side>-<width|
style|color>` longhands), and a literal scan cannot see those. The count is
"names spelled out in the dispatch", never "names the parser accepts".

WHAT IT DOES NOT SAY: recognising a name is not implementing it. A property may
parse and never be read by `layout.rs` (`font-style` is the standing example).
This script measures the PARSER's surface only; the layout side is audited by
hand in `docs/ui/css-support.md`.

Usage:  python scripts/css_coverage.py pagina.css [more.css ...]
"""

import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PARSE_RS = ROOT / "crates" / "rts-dom" / "src" / "style" / "parse.rs"

# The name→field dispatch is the `match prop { ... }` inside `parse_inline_block`.
# The scan is bounded to that function: value keywords ("block" | "flow-root")
# live in `parse_display` and the helpers below it, and would otherwise be
# counted as if they were property names.
ARM = re.compile(r'^\s*("(?:[a-z-]+)"(?:\s*\|\s*"[a-z-]+")*)\s*=>', re.M)


def supported() -> set[str]:
    src = PARSE_RS.read_text(encoding="utf-8")
    start = src.index("pub fn parse_inline_block")
    end = src.index("\nfn split_important", start)
    names: set[str] = set()
    for m in ARM.finditer(src[start:end]):
        names.update(re.findall(r'"([a-z-]+)"', m.group(1)))
    return names


def declarations(css: str) -> Counter:
    """Property names of every declaration in the sheet.

    Comments are stripped first; then only the text inside `{ }` is scanned, so
    selectors (`a:hover`) and at-rule preludes (`@media (min-width: 1px)`) do
    not contribute a false `hover`/`min-width`.
    """
    css = re.sub(r"/\*.*?\*/", "", css, flags=re.S)
    out: Counter = Counter()
    depth = 0
    buf: list[str] = []
    for ch in css:
        if ch == "{":
            depth += 1
            buf = []
        elif ch == "}":
            depth -= 1
            buf = []
        elif depth > 0:
            buf.append(ch)
            if ch == ";":
                decl = "".join(buf)
                buf = []
                name = decl.split(":", 1)[0].strip().lower()
                if re.fullmatch(r"-{0,2}[a-z][a-z0-9-]*", name or ""):
                    out[name] += 1
    return out


def main() -> int:
    files = [Path(a) for a in sys.argv[1:]]
    if not files:
        print(__doc__)
        return 2
    sup = supported()
    total: Counter = Counter()
    for f in files:
        total += declarations(f.read_text(encoding="utf-8", errors="replace"))

    # Custom properties are handled GENERICALLY (parse.rs stores any `--x` raw,
    # for `var()`), so they are neither a gap nor a credit — they are counted
    # apart. Vendor prefixes are counted apart for the same reason in reverse:
    # no engine is expected to implement `-webkit-*`, so leaving them in the
    # denominator would depress the share for something nobody wants.
    custom = [p for p in total if p.startswith("--")]
    vendor = [p for p in total if p.startswith("-") and not p.startswith("--")]
    std = [p for p in total if not p.startswith("-")]
    have = [p for p in std if p in sup]
    occ_std = sum(total[p] for p in std)
    occ_have = sum(total[p] for p in have)

    print(f"# folhas: {', '.join(str(f) for f in files)}")
    print(f"# nomes reconhecidos por parse.rs: {len(sup)}")
    print(f"propriedades distintas usadas   : {len(total)}")
    print(f"  padrao (nem custom nem vendor): {len(std)}")
    print(f"    reconhecidas                : {len(have)}")
    print(f"  custom (--x, generico)        : {len(custom)}")
    print(f"  vendor (-webkit-/-moz-)       : {len(vendor)}")
    print(f"ocorrencias de prop. padrao     : {occ_std}")
    print(f"  cobertas                      : {occ_have} ({occ_have * 100 // max(occ_std, 1)}%)")
    print()
    print(f"{'ocorr':>6}  {'temos':<6} propriedade")
    for prop, n in total.most_common():
        if prop.startswith("--"):
            continue
        print(f"{n:>6}  {'sim' if prop in sup else 'NAO':<6} {prop}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
