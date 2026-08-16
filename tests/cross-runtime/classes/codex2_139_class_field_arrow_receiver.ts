// Cross-runtime: an arrow stored in an instance field captures that instance.
class Button {
  count = 0;
  click = () => ++this.count;
}
const b = new Button();
const click = b.click;
console.log(click(), click(), b.count);
console.log(click.call({ count: 100 }), b.count);

