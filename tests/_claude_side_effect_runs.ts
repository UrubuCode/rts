// A module whose whole purpose is its top-level statements: it exports NOTHING.
//
// That is the shape `module_publish` gives no namespace to, and the reason
// `import "./x"` cannot be compiled as a namespace read with the answer
// dropped — both namespace-reading entry points throw for a module with none.
import { add, order, runs } from "./_claude_side_effect_registry";

order.push("side");
runs.n = runs.n + 1;
add("alfa");
add("beta");
