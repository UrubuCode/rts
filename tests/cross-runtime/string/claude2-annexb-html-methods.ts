// Cross-runtime: the Annex B "HTML methods" (anchor, link, fontcolor, big, sub,
// …) are still normative-optional-but-required-for-browsers, and every engine
// here has them. Their one interesting rule is CreateHTML's escaping: it
// replaces `"` in the ATTRIBUTE VALUE with &quot; and escapes NOTHING else — not
// `<`, not `&` — so they are not a sanitiser. Nothing in the corpus touches them.

function attempt(f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

const S: any = "x";

// --- the tag-only methods: one tag name each, no attribute ---
console.log("big=" + S.big());
console.log("blink=" + S.blink());
console.log("bold=" + S.bold());
console.log("fixed=" + S.fixed());
console.log("italics=" + S.italics());
console.log("small=" + S.small());
console.log("strike=" + S.strike());
console.log("sub=" + S.sub());
console.log("sup=" + S.sup());

// --- the attribute methods: name="value" ---
console.log("anchor=" + S.anchor("n"));
console.log("link=" + S.link("u"));
console.log("fontcolor=" + S.fontcolor("red"));
console.log("fontsize=" + S.fontsize(3));

// --- CreateHTML escapes only the double quote, and only in the value ---
console.log("quote-in-value=" + S.anchor('a"b'));
console.log("quote-twice=" + S.anchor('"a"'));
console.log("lt-in-value=" + S.link("<b>"));
console.log("amp-in-value=" + S.link("a&b"));
console.log("single-quote-in-value=" + S.link("a'b"));
console.log("newline-in-value=" + JSON.stringify(S.link("a\nb")));
console.log("quote-in-body=" + ('a"b' as any).anchor("n"));
console.log("lt-in-body=" + ("<b>" as any).bold());
console.log("amp-in-body=" + ("a&b" as any).big());

// --- both the receiver and the attribute value are ToString'd ---
console.log("num-receiver=" + String.prototype.big.call(12));
console.log("num-value=" + S.fontsize(12));
console.log("null-value=" + S.link(null));
console.log("undefined-value=" + S.link(undefined));
console.log("no-arg=" + S.link());
console.log("object-value=" + S.link({ toString: () => "o" }));
console.log("array-value=" + S.link(["a", "b"]));
console.log("symbol-value=" + attempt(() => S.link(Symbol("s"))));
console.log("throwing-value=" + attempt(() => S.link({ toString: () => { throw new RangeError("x"); } })));

// --- the receiver is coerced with RequireObjectCoercible, so null throws ---
console.log("null-this=" + attempt(() => String.prototype.big.call(null)));
console.log("undefined-this=" + attempt(() => String.prototype.anchor.call(undefined, "n")));
console.log("boxed-this=" + String.prototype.bold.call(new String("y")));
console.log("array-this=" + String.prototype.italics.call(["a", "b"]));

// --- and the value is coerced AFTER the receiver ---
const order: string[] = [];
console.log("order-result=" + attempt(() => String.prototype.link.call(
  { toString: () => { order.push("this"); return "b"; } },
  { toString: () => { order.push("value"); return "v"; } },
)));
console.log("order=" + order.join(","));

// --- function shapes: the attribute ones take 1 argument, the others 0 ---
const names: string[] = ["big", "blink", "bold", "fixed", "italics", "small", "strike", "sub", "sup", "anchor", "link", "fontcolor", "fontsize"];
for (let i = 0; i < names.length; i++) {
  const f: any = (String.prototype as any)[names[i]];
  console.log("fn-" + names[i] + "=" + typeof f + "/" + f.length + "/" + f.name);
}
const d: any = Object.getOwnPropertyDescriptor(String.prototype, "anchor");
console.log("desc=" + d.writable + "/" + d.enumerable + "/" + d.configurable);
console.log("enumerable-in-for-in=" + (function () {
  let found = false;
  for (const k in String.prototype) if (k === "anchor") found = true;
  return found;
})());

// --- they are plain string builders: the result is a normal string ---
console.log("nested=" + (S.bold() as string).italics());
console.log("length=" + S.anchor("n").length);
console.log("concat=" + (S.sub() + S.sup()));
console.log("astral-body=" + ("\u{1F600}" as any).bold().length);
