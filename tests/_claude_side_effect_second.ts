// The third importer of the same side-effect module, and the written-empty
// spelling of the import: `import {} from "m"` evaluates the module too.
import {} from "./_claude_side_effect_runs";
import { order } from "./_claude_side_effect_registry";

order.push("second");
export const two = 2;
