// Cross-runtime: a Blob's size is in BYTES after UTF-8 encoding (not in string
// length), its type is lower-cased and rejected when it is not printable ASCII,
// and slice() takes Array-style negative indices with its own content type.

(async function (): Promise<void> {
  // size counts encoded bytes, not code units.
  console.log("ascii=" + new Blob(["abc"]).size);
  console.log("latin=" + new Blob(["é"]).size);
  console.log("euro=" + new Blob(["€"]).size);
  console.log("astral=" + new Blob(["\u{1F600}"]).size);
  console.log("lone_surrogate=" + new Blob(["\uD800"]).size);
  console.log("empty=" + new Blob().size + "," + new Blob([]).size + "," + new Blob([""]).size);

  // Parts may be strings, buffers, views and other Blobs, in any mix.
  const mixed = new Blob(["ab", new Uint8Array([67, 68]), new Blob(["ef"]), new ArrayBuffer(2)]);
  console.log("mixed_size=" + mixed.size);
  console.log("mixed_text=" + JSON.stringify(await mixed.text()));
  const fromView = new Uint8Array([1, 2, 3, 4]).subarray(1, 3);
  console.log("view_window=" + new Blob([fromView]).size);
  console.log("dataview_part=" + new Blob([new DataView(new ArrayBuffer(3))]).size);

  // type is lower-cased and kept verbatim otherwise.
  console.log("type_default=" + JSON.stringify(new Blob(["x"]).type));
  console.log("type_lowered=" + new Blob(["x"], { type: "TEXT/Plain;Charset=UTF-8" }).type);
  console.log("type_spaces=" + JSON.stringify(new Blob(["x"], { type: " text/plain " }).type));
  console.log("type_nonascii=" + JSON.stringify(new Blob(["x"], { type: "tëxt/plain" }).type));

  // slice(): Array-style bounds, and a third argument that sets the type.
  const src = new Blob(["abcdefghij"]);
  console.log("slice_all=" + (await src.slice().text()));
  console.log("slice_from=" + (await src.slice(3).text()));
  console.log("slice_range=" + (await src.slice(2, 5).text()));
  console.log("slice_negative=" + (await src.slice(-3).text()));
  console.log("slice_both_negative=" + (await src.slice(-4, -2).text()));
  console.log("slice_reversed=" + JSON.stringify(await src.slice(5, 2).text()) + " size=" + src.slice(5, 2).size);
  console.log("slice_past_end=" + (await src.slice(8, 99).text()));
  console.log("slice_far_negative=" + (await src.slice(-99, 2).text()));
  console.log("slice_type=" + JSON.stringify(src.slice(0, 1, "X/Y").type) + " parent=" + JSON.stringify(src.type));
  console.log("slice_of_slice=" + (await src.slice(2, 8).slice(1, 3).text()));
  console.log("slice_is_new=" + (src.slice() !== src) + " kind=" + src.slice().constructor.name);

  // A slice cuts BYTES, so it can land in the middle of a character.
  const multi = new Blob(["€€"]);
  console.log("multi_size=" + multi.size);
  const cut = multi.slice(0, 4);
  console.log("cut_size=" + cut.size);
  const cutText = await cut.text();
  const codes: string[] = [];
  for (const ch of cutText) codes.push((ch.codePointAt(0) as number).toString(16));
  console.log("cut_codepoints=" + codes.join(","));

  // The async readers.
  const reader = new Blob(["héllo"]);
  console.log("text=" + (await reader.text()));
  const ab = await reader.arrayBuffer();
  console.log("arrayBuffer=" + ab.byteLength + " kind=" + ab.constructor.name + " bytes=" + Array.from(new Uint8Array(ab)).join(","));
  console.log("bytes_method=" + (typeof reader.bytes === "function"));
  const bytes = await reader.bytes();
  console.log("bytes=" + bytes.constructor.name + " " + Array.from(bytes).join(","));
  console.log("read_twice=" + (await reader.text()) + "|" + (await reader.text()));
  console.log("empty_text=" + JSON.stringify(await new Blob().text()));

  // A Blob built from another Blob copies the bytes.
  const typed = new Blob(["xy"], { type: "a/b" });
  const wrapped = new Blob([typed, "z"]);
  console.log("wrapped_size=" + wrapped.size + " text=" + (await wrapped.text()));

  // Non-Blob, non-buffer parts go through ToString.
  console.log("number_part=" + (await new Blob([1234 as any]).text()));
  try {
    console.log("non_iterable=" + new Blob({} as any).size);
  } catch (e: any) {
    console.log("non_iterable=" + e.constructor.name);
  }
  console.log("string_not_array=" + (function (): string {
    try {
      return String(new Blob("abc" as any).size);
    } catch (e: any) {
      return e.constructor.name;
    }
  })());

  // Line endings: the default leaves them alone.
  console.log("endings_default=" + JSON.stringify(await new Blob(["a\r\nb\nc"]).text()));
  console.log("endings_transparent=" + JSON.stringify(await new Blob(["a\r\nb\nc"], { endings: "transparent" }).text()));

  // Identity.
  console.log("tag=" + Object.prototype.toString.call(new Blob()));
  console.log("ctor_name=" + new Blob().constructor.name);
  console.log("instanceof=" + (new Blob() instanceof Blob));
  console.log("size_is_getter=" + (typeof (Object.getOwnPropertyDescriptor(Blob.prototype, "size") as any).get));
  console.log("no_own_size=" + Object.prototype.hasOwnProperty.call(new Blob(), "size"));
})();
