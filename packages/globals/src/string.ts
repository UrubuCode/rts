import { str } from "rts";

export class String {
  private value: str;

  constructor(value: str = "") {
    this.value = value;
  }

  toString(): str {
    return this.value;
  }

  valueOf(): str {
    return this.value;
  }

  concat(next: any): String {
    return new String(str.concat(this.value, `${next}`));
  }

  length(): number {
    return str.len(this.value);
  }
}

export type string = String;
