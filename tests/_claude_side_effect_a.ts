// Side-effect imports NEST: this one is reached by a side-effect import and
// makes one of its own, so a fix that only works at the entry file is caught.
import "./_claude_side_effect_adep";
import { order } from "./_claude_side_effect_registry";

order.push("a");
