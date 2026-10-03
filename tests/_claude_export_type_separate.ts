// `import type { T }` as its own declaration, then `export type { T }`.
import type { Cfg } from "./_claude_export_type_base";

export type { Cfg };

export function use(c: Cfg): number {
  return c.a;
}
