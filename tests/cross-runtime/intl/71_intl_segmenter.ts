// Cross-runtime compatibility: Intl.Segmenter.
const seg = new Intl.Segmenter("en", { granularity: "word" });
const out = Array.from(seg.segment("hi there"))
  .map((x: any) => x.segment + ":" + x.isWordLike)
  .join("|");
console.log("segments=" + out);
