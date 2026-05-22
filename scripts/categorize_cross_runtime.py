#!/usr/bin/env python3
"""
Categoriza fixtures cross-runtime em subpastas seguindo a mesma logica do
`.github/workflows/cross-runtime.yml` (function categorize).

Uso:
  python scripts/categorize_cross_runtime.py --dry-run  # mostra plano
  python scripts/categorize_cross_runtime.py            # aplica via git mv

Cada fixture vai para `tests/cross-runtime/<categoria>/<nome>.ts`. O CI e
o runner local (que enumera `tests/cross-runtime/*.ts`) precisam ser
atualizados para descer recursivamente — feito num passo separado.
"""
from __future__ import annotations
import argparse
import re
import subprocess
import sys
from pathlib import Path


def categorize(name: str) -> str:
    n = name.lower()
    rules = [
        (r"(stream|compression|textdecoder_stream|textencoder)", "streams"),
        (r"(arraybuffer|dataview|typedarray|atomics|sharedarraybuffer)", "typed-buffers"),
        (r"(blob|file|formdata|headers|request|response)", "web-api"),
        (r"(abort_?controller|abort_?signal|event_?target|message_?channel|microtask|queuemicrotask)", "events-async"),
        (r"(weak_?refs?|finalization_?registry)", "gc-weak"),
        (r"(structured_?clone|domexception|aggregate_?error|modern_helpers|property_order)", "misc-platform"),
        (r"intl", "intl"),
        (r"regex", "regex"),
        (r"url", "url"),
        (r"(encodeuri|decodeuri|text_encoding|subtle_digest|encode_decode)", "encoding"),
        (r"bigint", "bigint"),
        (r"json", "json"),
        (r"(generator|yield|yieldstar)", "generators"),
        (r"(iterator_helpers|iterator_toarray)", "iteration"),
        (r"(promise|await|promiseall|allsettled|withresolvers)", "promises"),
        (r"(class|extends|super_|private_field|private_method|static_block|new_target|computed_class)", "classes"),
        (r"(instanceof|typeof|error)", "classes-errors"),
        (r"console", "console"),
        (r"(function_meta|function_bind|function_call_apply|function_name_length|arrow_rest_params|arguments_object|new_target|closure)", "fn-meta"),
        (r"(template|tagged_template|destructur|spread|labeled_break|delete_operator|void_operator|comma_operator|this_strict|hashbang|import_meta|numeric_separator|nullish_assignment|logical_assignment|optional_catch|exponentiation)", "syntax"),
        (r"(coercion|number_format|nan_negative_zero|bitwise|number_tostring|number_constructor|number_valueof|number_parseint|number_parsefloat|number_isfinite|number_isinteger|number_issafeinteger|parseint|parsefloat|isfinite|isnan|tofixed|toexponential|toprecision)", "numeric"),
        (r"math", "math"),
        (r"date", "date"),
        (r"boolean", "boolean"),
        (r"array|findlast|findlastindex|flatmap|toreversed|tosorted|tospliced", "array"),
        (r"(object|proxy|reflect)", "object-meta"),
        (r"symbol", "symbol"),
        (r"(map|set_operations|set_ops|map_set|groupby|map_iter|map_foreach)", "collections"),
        (r"string|replaceall|matchall|substring|substr", "string"),
        (r"(dynamic_import|setimmediate|setinterval)", "misc-platform"),
        # (organizacao por pasta) Catch-alls posteriores ao regex do CI.
        (r"(number_methods|number_edges)", "numeric"),
        (r"(logical|equality|nullish|control_flow)", "syntax"),
    ]
    for pat, cat in rules:
        if re.search(pat, n):
            return cat
    return "other"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true", help="lista sem mover")
    parser.add_argument("--root", default="tests/cross-runtime", help="diretorio raiz das fixtures")
    args = parser.parse_args()

    root = Path(args.root)
    if not root.exists():
        print(f"erro: {root} nao existe", file=sys.stderr)
        return 1

    # Coleta arquivos .ts top-level (ignora subpastas existentes).
    files = [f for f in sorted(root.iterdir()) if f.is_file() and f.suffix == ".ts"]
    if not files:
        print("nada para mover (fixtures ja' organizadas?)")
        return 0

    plan: dict[str, list[Path]] = {}
    for f in files:
        cat = categorize(f.stem)
        plan.setdefault(cat, []).append(f)

    # Imprime distribuicao.
    print("=== Plano de categorizacao ===")
    for cat in sorted(plan, key=lambda c: -len(plan[c])):
        print(f"  {cat:<20} {len(plan[cat])}")
    total = sum(len(v) for v in plan.values())
    print(f"  total                {total}")
    print()

    if args.dry_run:
        print("dry-run: nenhum arquivo movido.")
        return 0

    # Aplica via git mv (mantem historico).
    for cat, paths in plan.items():
        target_dir = root / cat
        target_dir.mkdir(exist_ok=True)
        for src in paths:
            dst = target_dir / src.name
            cmd = ["git", "mv", str(src), str(dst)]
            r = subprocess.run(cmd, capture_output=True, text=True)
            if r.returncode != 0:
                print(f"warn: git mv falhou para {src} -> {dst}: {r.stderr.strip()}", file=sys.stderr)
                # Fallback: rename simples (sem historico).
                src.rename(dst)
    print(f"movidos {total} arquivos em {len(plan)} categorias.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
