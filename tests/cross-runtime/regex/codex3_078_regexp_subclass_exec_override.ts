// Cross-runtime: String matching dispatches through an overridden RegExp exec method.
const seen: string[] = [];
class Traced extends RegExp {
  exec(input: string) {
    seen.push(this.lastIndex + ":" + input.length);
    return super.exec(input);
  }
}
const re = new Traced("a", "g");
console.log(JSON.stringify("a-a".match(re)));
console.log(seen.join("|"));

