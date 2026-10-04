// The transitive case: a module imported for a VALUE which itself imports a
// module only for its effects.
import "./_claude_side_effect_runs";
import { order } from "./_claude_side_effect_registry";

order.push("mid");
export const mark = 1;
