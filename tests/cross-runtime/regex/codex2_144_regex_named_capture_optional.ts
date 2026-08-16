// Cross-runtime: absent optional named captures appear as undefined.
const re = /(?<word>[a-z]+)(?:-(?<num>\d+))?/;
const a = re.exec("item-42")!;
const b = re.exec("plain")!;
console.log(a.groups!.word, a.groups!.num);
console.log(b.groups!.word, b.groups!.num);

