// Cross-runtime: classes with constructor, fields, methods, extends and accessors.
class Box {
  value: string;

  constructor(value: string) {
    this.value = value;
  }

  label(): string {
    return "box:" + this.value;
  }

  get size(): number {
    return this.value.length;
  }

  set rename(next: string) {
    this.value = next;
  }
}

class GiftBox extends Box {
  kind: string;

  constructor(value: string, kind: string) {
    super(value);
    this.kind = kind;
  }

  label(): string {
    return super.label() + ":" + this.kind;
  }
}

const a = new Box("toy");
console.log("a_label=" + a.label());
console.log("a_size=" + a.size);
a.rename = "drone";
console.log("a_next=" + a.label());

const b = new GiftBox("book", "gift");
console.log("b_label=" + b.label());
console.log("is_box=" + (b instanceof Box));
console.log("is_gift=" + (b instanceof GiftBox));
