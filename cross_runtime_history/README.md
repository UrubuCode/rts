# Cross-runtime parity — histórico

Snapshots semanais (segunda 6h UTC) gerados pelo workflow
`.github/workflows/cross-runtime.yml`.

## Estrutura

- `index.json` — lista cronológica `{ date, pct, pass, total_valid, ... }`
  para dashboards consumirem.
- `YYYY-MM-DD.json` — snapshot detalhado da run daquela semana
  (summary + lista de divergências com status).

Não tem outputs completos (Bun/Node/RTS) — o snapshot atual fica em
`cross_runtime_report.json` na raiz. History é só sumário + nomes para
mostrar tendência ao longo do tempo.

## Consumo externo

Dashboards podem fetchar:
```
https://raw.githubusercontent.com/UrubuCode/rts/main/cross_runtime_history/index.json
https://raw.githubusercontent.com/UrubuCode/rts/main/cross_runtime_history/2026-05-10.json
```
