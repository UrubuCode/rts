// The classes `class_layout_modules.test.ts` constructs, declared in another
// module on purpose: a class with a layout is constructed across the boundary.
export class Shape {
  kind = "shape";
  id: number;
  constructor(id: number) { this.id = id; }
  label(): string { return this.kind + this.id; }
}
export class Rect extends Shape {
  w: number; h: number;
  constructor(id: number, w: number, h: number) { super(id); this.w = w; this.h = h; }
  area(): number { return this.w * this.h; }
  grow(k: number): void { this.w *= k; this.h *= k; }
}
export function madeHere(n: number): Rect { return new Rect(n, n, n); }
