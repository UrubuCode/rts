// Cross-runtime: static blocks interleave with static field initializers.
const seen: string[] = [];
class Sequence {
  static a = (seen.push("a"), 1);
  static { seen.push("block1:" + this.a); this.a += 2; }
  static b = (seen.push("b"), this.a * 2);
  static { seen.push("block2:" + this.b); }
}
console.log(Sequence.a, Sequence.b);
console.log(seen.join("|"));

