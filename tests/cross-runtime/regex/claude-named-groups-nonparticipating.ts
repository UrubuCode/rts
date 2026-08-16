// Cross-runtime: a named group that does NOT participate in the match is still a
// key on the groups object, valued undefined — and `$<name>` for it expands to
// the empty string rather than to "undefined". Also pins that `groups` is a
// null-prototype object present only when the pattern declares a name, and that
// an UNKNOWN $<name> is silently empty when named groups exist at all.
// 81 and 404 only use groups that always match.

const alt = /(?<p>a)|(?<q>b)/;

function dumpGroups(m: any): string {
  if (!m) return "null";
  const g = m.groups;
  const keys = Object.keys(g);
  return keys.map((k) => k + "=" + String(g[k])).join(",") + " #" + keys.length;
}

// --- both keys exist whichever branch won ---
console.log("hit-p=" + dumpGroups(alt.exec("a")));
console.log("hit-q=" + dumpGroups(alt.exec("b")));
console.log("in-p=" + ("p" in (alt.exec("b") as any).groups));
console.log("in-q=" + ("q" in (alt.exec("a") as any).groups));
console.log("hasOwn=" + Object.prototype.hasOwnProperty.call((alt.exec("a") as any).groups, "q"));

// --- the numbered slots agree ---
const mb: any = alt.exec("b");
console.log("numbered=" + mb.length + "|" + String(mb[1]) + "|" + String(mb[2]));
console.log("json=" + JSON.stringify(mb));

// --- groups has a null prototype, so no inherited keys leak in ---
const g: any = (alt.exec("a") as any).groups;
console.log("proto=" + String(Object.getPrototypeOf(g)));
console.log("tostring-absent=" + (typeof g.toString));
console.log("tag=" + Object.prototype.toString.call(g));
console.log("enumerable=" + Object.getOwnPropertyDescriptor(mb, "groups")?.enumerable);

// --- groups is undefined when the pattern names nothing ---
console.log("unnamed=" + String((/(a)/.exec("a") as any).groups));
console.log("unnamed-in=" + ("groups" in (/(a)/.exec("a") as any)));

// --- $<name> of a non-participating group expands to "" ---
console.log("repl-both=" + "b".replace(alt, "[$<p>][$<q>]"));
console.log("repl-both-a=" + "a".replace(alt, "[$<p>][$<q>]"));
console.log("repl-unknown=" + "a".replace(alt, "[$<zz>]"));
console.log("repl-numbered=" + "b".replace(alt, "[$1][$2]"));
console.log("repl-amp=" + "b".replace(alt, "[$&]"));

// --- with NO named group in the pattern, $<...> stays literal ---
console.log("literal-named=" + "a".replace(/a/, "[$<p>]"));
console.log("literal-named-cap=" + "a".replace(/(a)/, "[$<p>]"));

// --- a replacer function receives the same undefined-valued key ---
console.log(
  "fn-groups=" +
    "b".replace(alt, (...args: any[]) => {
      const gr = args[args.length - 1];
      return String(gr.p) + "/" + String(gr.q) + "/" + Object.keys(gr).join("+");
    }),
);

// --- optional named group, quantified to zero ---
const opt = /x(?<n>\d+)?y/;
console.log("opt-hit=" + dumpGroups(opt.exec("x12y")));
console.log("opt-miss=" + dumpGroups(opt.exec("xy")));
console.log("opt-repl=" + "xy".replace(opt, "[$<n>]"));

// --- a named group inside a NEGATIVE lookahead never participates ---
const nla = /a(?!(?<z>b))/;
console.log("nla=" + dumpGroups(nla.exec("ac")));

// --- a group behind a failed alternative resets between attempts ---
const reset = /(?:(?<u>a)x)?(?<v>a)/;
console.log("reset=" + dumpGroups(reset.exec("a")));
console.log("reset-hit=" + dumpGroups(reset.exec("ax")));

// --- matchAll keeps the same shape on every entry ---
console.log(
  "matchall=" +
    [..."ab".matchAll(/(?<p>a)|(?<q>b)/g)]
      .map((m: any) => String(m.groups.p) + ">" + String(m.groups.q))
      .join(" "),
);

// --- the d flag mirrors it: indices.groups has the key, valued undefined ---
const d: any = /(?<p>a)|(?<q>b)/d.exec("b");
console.log("d-groups-keys=" + Object.keys(d.indices.groups).join(","));
console.log("d-groups-p=" + String(d.indices.groups.p));
console.log("d-groups-q=" + JSON.stringify(d.indices.groups.q));
console.log("d-indices=" + JSON.stringify(d.indices));
console.log("d-groups-proto=" + String(Object.getPrototypeOf(d.indices.groups)));
console.log("d-unnamed=" + String((/(a)/d.exec("a") as any).indices.groups));
