// Cross-runtime: at() vs charAt() vs [] — the THREE disagree on how an index
// is coerced and what an out-of-range index yields.
//   at()     -> truncates, accepts negatives, undefined out of range
//   charAt() -> truncates, no negatives, EMPTY STRING out of range
//   []       -> canonical-numeric-string key only, undefined out of range

const s = "hello";

function row(label: string, i: any): void {
  console.log(
    label +
      " at=" + String(s.at(i)) +
      " charAt=[" + s.charAt(i) + "]" +
      " bracket=" + String((s as any)[i]),
  );
}

// --- plain in-range integers ---
row("i=0", 0);
row("i=4", 4);

// --- out of range: undefined vs "" vs undefined ---
row("i=5", 5);
row("i=99", 99);

// --- negatives: only at() counts from the end ---
row("i=-1", -1);
row("i=-5", -5);
row("i=-6", -6);
row("i=-0", -0);

// --- fractional indices: at/charAt truncate toward zero, [] does not match a slot ---
row("i=1.5", 1.5);
row("i=0.9", 0.9);
row("i=-1.5", -1.5);
row("i=4.99", 4.99);

// --- non-numeric / special values: at & charAt coerce to 0, [] misses ---
row("i=undefined", undefined);
row("i=null", null);
row("i=NaN", NaN);
row("i=true", true);
row("i=false", false);
row("i='2'", "2");
row("i=''", "");

// --- infinities ---
row("i=Infinity", Infinity);
row("i=-Infinity", -Infinity);

// --- [] is a property lookup: only the canonical form hits ---
console.log("bracket-'1'=" + String((s as any)["1"]));
console.log("bracket-'01'=" + String((s as any)["01"]));
console.log("bracket-'1.0'=" + String((s as any)["1.0"]));
console.log("bracket-' 1'=" + String((s as any)[" 1"]));
console.log("bracket-'-0'=" + String((s as any)["-0"]));
console.log("bracket-length=" + String((s as any)["length"]));

// --- charAt with no argument = index 0; at with no argument = index 0 ---
console.log("charAt-noarg=" + s.charAt());
console.log("at-noarg=" + String((s.at as any)()));

// --- 'in' sees index slots as real own properties ---
console.log("0-in=" + (0 in Object(s)));
console.log("5-in=" + (5 in Object(s)));

// --- empty string: every access misses ---
console.log("empty-at=" + String("".at(0)));
console.log("empty-charAt=[" + "".charAt(0) + "]");
console.log("empty-bracket=" + String(("" as any)[0]));

// --- all three index by code UNIT, tearing a surrogate pair ---
const emoji = "a\u{1F600}";
console.log("emoji-at1=" + emoji.at(1).charCodeAt(0).toString(16));
console.log("emoji-charAt1=" + emoji.charAt(1).charCodeAt(0).toString(16));
console.log("emoji-bracket1=" + (emoji as any)[1].charCodeAt(0).toString(16));
