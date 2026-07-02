// Iterator helpers (#306) — the rts-shared stdlib global (NOT primordial).
// Level A: `Iterator.from(arrayLike)` snapshots the elements into a cursor
// wrapper; `toArray()` DRAINS from the cursor (a second call yields `[]`,
// matching the one-shot iterator-helper semantics the corpus checks). The
// lazy protocol (`next()`) and the transform helpers (`map`/`filter`/`take`)
// are a later increment.
//
// Prelude naming rule: locals carry the `__it` prefix.

class Iterator {
  #items: any[] = [];
  #i: number = 0;

  static from(__itSrc: any): any {
    const __itOut = new Iterator();
    for (let __itK = 0; __itK < __itSrc.length; __itK++) {
      __itOut.#items.push(__itSrc[__itK]);
    }
    return __itOut;
  }

  toArray(): any[] {
    const __itOut: any[] = [];
    while (this.#i < this.#items.length) {
      __itOut.push(this.#items[this.#i]);
      this.#i = this.#i + 1;
    }
    return __itOut;
  }
}
