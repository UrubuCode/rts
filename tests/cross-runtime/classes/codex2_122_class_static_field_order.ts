// Cross-runtime: static fields initialize in source order with class receiver access.
class Counter {
  static first = 2;
  static second = this.first * 3;
  static third = Counter.second + 1;
}
console.log(Counter.first, Counter.second, Counter.third);
console.log(Object.keys(Counter).join(","));

