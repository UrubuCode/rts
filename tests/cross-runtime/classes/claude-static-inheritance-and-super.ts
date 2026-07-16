// Cross-runtime: static methods inherited by subclasses + super in static context.
class Shape {
  static count: number = 0;
  static kind(): string {
    return "shape";
  }
  static describe(): string {
    return "kind=" + this.kind();
  }
  static make(n: number): string {
    return this.kind() + ":" + n;
  }
  static bump(): number {
    this.count = this.count + 1;
    return this.count;
  }
}

class Circle extends Shape {
  static kind(): string {
    return "circle";
  }
  static describe(): string {
    return "circle<" + super.describe() + ">";
  }
}

class SmallCircle extends Circle {
  static kind(): string {
    return "small-" + super.kind();
  }
  static describe(): string {
    return "small<" + super.describe() + ">";
  }
}

console.log("shape_kind=" + Shape.kind());
console.log("circle_kind=" + Circle.kind());
console.log("small_kind=" + SmallCircle.kind());

console.log("shape_describe=" + Shape.describe());
console.log("circle_describe=" + Circle.describe());
console.log("small_describe=" + SmallCircle.describe());

// inherited static reached through the subclass: `this` is the subclass ctor
console.log("circle_make=" + Circle.make(3));
console.log("small_make=" + SmallCircle.make(4));

// static field: subclass reads through the chain until it writes its own
console.log("shape_count0=" + Shape.count);
console.log("circle_count0=" + Circle.count);
console.log("circle_bump=" + Circle.bump());
console.log("circle_count1=" + Circle.count);
console.log("shape_count1=" + Shape.count);
console.log("circle_own=" + Object.prototype.hasOwnProperty.call(Circle, "count"));

console.log("shape_bump=" + Shape.bump());
console.log("shape_count2=" + Shape.count);
console.log("circle_count2=" + Circle.count);

// static method borrowed with an explicit receiver
console.log("borrowed_describe=" + Shape.describe.call(Circle));
