// Cross-runtime: the `v` flag is not only a set-expression grammar, it is a
// STRICTER one — a whole table of punctuators becomes a SyntaxError inside a
// class where `u` accepted them bare, and every DOUBLE punctuator is reserved.
// claude-v-flag-set-operations pins what v CAN express; nothing pins what it
// REFUSES, which is the half a parser gets wrong silently.

function syn(src: string, flags: string): string {
  try {
    new RegExp(src, flags);
    return "ok";
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

function both(src: string): string {
  return "u=" + syn(src, "u") + " v=" + syn(src, "v");
}

function t(src: string, flags: string, subject: string): string {
  try {
    return String(new RegExp(src, flags).test(subject));
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- the ClassSetSyntaxCharacter set: bare inside a v class is a SyntaxError ---
console.log("lparen=" + both("[(]"));
console.log("rparen=" + both("[)]"));
console.log("lbrace=" + both("[{]"));
console.log("rbrace=" + both("[}]"));
console.log("lbracket=" + both("[[]"));
console.log("slash=" + both("[/]"));
console.log("pipe=" + both("[|]"));
console.log("minus=" + both("[-]"));

// --- and each of them is fine ESCAPED under v ---
console.log("esc-lparen=" + t("^[\\(]$", "v", "("));
console.log("esc-lbrace=" + t("^[\\{]$", "v", "{"));
console.log("esc-slash=" + t("^[\\/]$", "v", "/"));
console.log("esc-pipe=" + t("^[\\|]$", "v", "|"));
console.log("esc-minus=" + t("^[\\-]$", "v", "-"));
console.log("esc-lbracket=" + t("^[\\[]$", "v", "["));

// --- a range keeps `-` legal in its infix position ---
console.log("range=" + t("^[a-c]$", "v", "b"));
console.log("range-out=" + t("^[a-c]$", "v", "d"));

// --- DOUBLE punctuators are reserved wholesale, even ones with no meaning ---
console.log("bangbang=" + both("[!!]"));
console.log("hashhash=" + both("[##]"));
console.log("dollardollar=" + both("[$$]"));
console.log("percentpercent=" + both("[%%]"));
console.log("starstar=" + both("[**]"));
console.log("plusplus=" + both("[++]"));
console.log("commacomma=" + both("[,,]"));
console.log("dotdot=" + both("[..]"));
console.log("coloncolon=" + both("[::]"));
console.log("semisemi=" + both("[;;]"));
console.log("ltlt=" + both("[<<]"));
console.log("eqeq=" + both("[==]"));
console.log("gtgt=" + both("[>>]"));
console.log("questquest=" + both("[??]"));
console.log("atat=" + both("[@@]"));
console.log("tildetilde=" + both("[~~]"));

// --- but a SINGLE one of the same character is an ordinary member ---
console.log("single-bang=" + t("^[!]$", "v", "!"));
console.log("single-star=" + t("^[*]$", "v", "*"));
console.log("single-at=" + t("^[@]$", "v", "@"));
console.log("single-tilde=" + t("^[~]$", "v", "~"));

// --- `^^` is NOT a reserved double: the first ^ is the negation marker ---
console.log("caretcaret-source=" + syn("[^^]", "v"));
console.log("caretcaret-match=" + t("[^^]", "v", "a") + "/" + t("[^^]", "v", "^"));

// --- `&&` is the intersection operator, so a lone member `a` beside it is not a member ---
console.log("amp-intersect=" + t("^[a&&b]$", "v", "a"));
console.log("amp-intersect-real=" + t("^[[ab]&&[bc]]$", "v", "b"));
console.log("single-amp=" + t("^[&]$", "v", "&"));

// --- the same three sources under `u`, where they are plain members ---
console.log("u-bangbang=" + t("^[!!]$", "u", "!"));
console.log("u-amp=" + t("^[a&&b]$", "u", "&"));
console.log("u-lone-dash=" + t("^[-]$", "u", "-"));

// --- nesting is a v-only grammar; under u the inner bracket ends the class ---
console.log("nested-v=" + t("^[[a-z][0-9]]$", "v", "5"));
console.log("nested-u=" + syn("[[a-z][0-9]]", "u"));

// --- a negated class may not contain a multi-character string ---
console.log("neg-strings=" + syn("[^\\q{ab}]", "v"));
console.log("pos-strings=" + t("^[\\q{ab}]$", "v", "ab"));

// --- flags and accessors on a v regex ---
console.log("flags=" + new RegExp("a", "v").flags);
console.log("uv-together=" + syn("a", "uv"));
console.log("vv=" + syn("a", "vv"));
