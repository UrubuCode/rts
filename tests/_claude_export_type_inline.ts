// `import { value, type T }` plus `export type { T }` — the two inline
// spellings in one file, which is the shape the defect was reported in.
import { marker, type Cfg } from "./_claude_export_type_base";

export type { Cfg };

export function use(c: Cfg): string {
  return marker + ":" + String(c.a);
}
