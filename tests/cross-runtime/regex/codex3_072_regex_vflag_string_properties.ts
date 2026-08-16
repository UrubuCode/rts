// Cross-runtime: v-mode string properties match multi-code-point emoji sequences.
const re = new RegExp("\\p{RGI_Emoji_Flag_Sequence}", "gv");
const input = "A🇧🇷B🇺🇸C";
console.log(JSON.stringify(input.match(re)));
console.log([...input.matchAll(re)].map((m) => m.index).join(","));

