// Cross-runtime: escapes de string no JSON.stringify.
// --- escapes basicos com atalho proprio
console.log("quote=" + JSON.stringify("a\"b"));
console.log("backslash=" + JSON.stringify("a\\b"));
console.log("newline=" + JSON.stringify("a\nb"));
console.log("cr=" + JSON.stringify("a\rb"));
console.log("tab=" + JSON.stringify("a\tb"));
console.log("backspace=" + JSON.stringify("a\bb"));
console.log("formfeed=" + JSON.stringify("a\fb"));

// --- controles sem atalho viram \u00XX
console.log("nul=" + JSON.stringify("a\u0000b"));
console.log("soh=" + JSON.stringify("a\u0001b"));
console.log("vtab=" + JSON.stringify("a\u000bb"));
console.log("esc=" + JSON.stringify("a\u001bb"));
console.log("us=" + JSON.stringify("a\u001fb"));
console.log("del=" + JSON.stringify("a\u007fb"));
console.log("space=" + JSON.stringify("a b"));

// --- NAO escapados
console.log("slash=" + JSON.stringify("a/b"));
console.log("apostrophe=" + JSON.stringify("a'b"));
console.log("accent=" + JSON.stringify("café"));
console.log("cjk=" + JSON.stringify("日本"));

// --- lone surrogates viram escape (well-formed stringify)
console.log("lone_high=" + JSON.stringify("\ud800"));
console.log("lone_low=" + JSON.stringify("\udfff"));
console.log("lone_mid=" + JSON.stringify("a\ud834b"));
const emoji = "\ud83d\ude00";
console.log("pair=" + JSON.stringify(emoji));
console.log("pair_len=" + JSON.stringify(emoji).length);
console.log("reversed_pair=" + JSON.stringify("\ude00\ud83d"));

// --- chaves tambem sao escapadas
console.log("key_escape=" + JSON.stringify({ "a\nb": 1 }));
console.log("key_quote=" + JSON.stringify({ "a\"b": 1 }));
console.log("key_ctrl=" + JSON.stringify({ "a\u0001b": 1 }));

// --- round-trip
const s = "a\u0000b \"q\" \\ \n end";
console.log("roundtrip=" + (JSON.parse(JSON.stringify(s)) === s));
console.log("empty=" + JSON.stringify(""));
