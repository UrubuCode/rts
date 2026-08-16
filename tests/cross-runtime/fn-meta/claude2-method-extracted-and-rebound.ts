// Cross-runtime: a method is a plain function stored on an object — extracting
// it loses the receiver, and the ways of getting one back (bind, a field arrow,
// call/apply, a wrapper) each have a different identity and cost.

class Account {
  label: string;
  balance: number;
  // A field arrow is created per instance and captures `this` at construction.
  boundShow: () => string;

  constructor(label: string, balance: number) {
    this.label = label;
    this.balance = balance;
    this.boundShow = (): string => this.label + ":" + this.balance;
  }

  show(): string {
    return this.label + ":" + this.balance;
  }

  add(n: number): string {
    this.balance += n;
    return this.show();
  }

  static describe(): string {
    return "static:" + (this === Account ? "class" : String(this));
  }
}

const acct = new Account("A", 10);

// 1) Called as a method it works; extracted it does not, because a class body
//    is strict and the receiver stays undefined.
console.log("as_method=" + acct.show());
const extracted = acct.show;
function callExtracted(fn: any, ...args: any[]): string {
  try {
    return "ok:" + fn(...args);
  } catch (e) {
    return "threw:" + (e as any).constructor.name;
  }
}
console.log("extracted=" + callExtracted(extracted));

// 2) The extracted function is the SAME object as the one on the prototype.
console.log("same_function=" + (extracted === Account.prototype.show));
console.log("not_own_property=" + Object.prototype.hasOwnProperty.call(acct, "show"));

// 3) Handing it back a receiver restores it, three ways.
console.log("via_call=" + extracted.call(acct));
console.log("via_apply=" + extracted.apply(acct, []));
console.log("via_bind=" + extracted.bind(acct)());

// 4) The field arrow needs no receiver and survives extraction.
const arrowExtracted = acct.boundShow;
console.log("field_arrow=" + arrowExtracted());
console.log("field_arrow_is_own=" + Object.prototype.hasOwnProperty.call(acct, "boundShow"));
console.log("field_arrow_per_instance=" + (acct.boundShow !== new Account("B", 0).boundShow));

// 5) Two binds of the same method are two different functions, so removing a
//    listener registered with an inline bind would fail.
const boundOnce = acct.show.bind(acct);
const boundTwice = acct.show.bind(acct);
console.log("bind_identity=" + (boundOnce !== boundTwice) + "|same_result=" +
  (boundOnce() === boundTwice()));

// 6) A bound method as a callback keeps its receiver through the iteration.
const accounts = [new Account("X", 1), new Account("Y", 2)];
console.log("callback_bound=" + accounts.map((a) => a.show()).join(","));
console.log("callback_extracted=" + accounts.map((a) => callExtracted(a.show)).join(","));
console.log("callback_method_ref=" + accounts.map(Account.prototype.show.call.bind(Account.prototype.show)).join(","));

// 7) `thisArg` of a built-in iteration method supplies the receiver.
const collected: string[] = [];
[1, 2].forEach(function (n: number): void {
  const self: any = this;
  collected.push(self.label + n);
}, acct);
console.log("this_arg=" + collected.join(","));

// 8) An arrow ignores that `thisArg` — it has no `this` of its own to set.
const arrowCollected: string[] = [];
[1, 2].forEach((n: number): void => {
  arrowCollected.push(String(n));
}, acct);
console.log("arrow_ignores_thisarg=" + arrowCollected.join(","));

// 9) Re-binding a mutating method: the bound version writes through to the
//    original instance.
const boundAdd = acct.add.bind(acct);
console.log("bound_mutation=" + boundAdd(5) + "|" + boundAdd(5));
console.log("instance_after=" + acct.show());

// 10) Borrowing the method for a foreign object with the same shape.
console.log("foreign_receiver=" + extracted.call({ label: "foreign", balance: 99 }));

// 11) A method taken off the PROTOTYPE and given to another object as its own.
const adopted: any = { label: "adopted", balance: 3, show: Account.prototype.show };
console.log("adopted_method=" + adopted.show());
console.log("adopted_is_same_fn=" + (adopted.show === Account.prototype.show));

// 12) A static method extracted loses the class as receiver.
const staticExtracted = Account.describe;
console.log("static_as_method=" + Account.describe());
console.log("static_extracted=" + callExtracted(staticExtracted));
console.log("static_rebound=" + staticExtracted.call(Account));

// 13) A getter is not extracted by reading the property — the read runs it.
class Computed {
  get twice(): number {
    return 21 * 2;
  }
}
const computed = new Computed();
console.log("getter_read=" + computed.twice);
const getterFn: any = (Object.getOwnPropertyDescriptor(Computed.prototype, "twice") as any).get;
console.log("getter_extracted_type=" + typeof getterFn + "|" + JSON.stringify(getterFn.name));
console.log("getter_called_with_receiver=" + getterFn.call(computed));

// 14) A wrapper closure is a third way, and it re-reads the property each call.
let live: any = acct;
const wrapper = (): string => live.show();
console.log("wrapper_first=" + wrapper());
live = new Account("Z", 7);
console.log("wrapper_after_swap=" + wrapper());
console.log("bound_after_swap=" + boundOnce());

// 15) The bound function's metadata records the target.
console.log("bound_meta=" + JSON.stringify(boundOnce.name) + "/" + boundOnce.length +
  "|target=" + JSON.stringify(acct.show.name) + "/" + acct.show.length);

// 16) A method assigned onto an instance shadows the prototype's, and deleting
//     it uncovers the original.
const shadowed = new Account("S", 1);
(shadowed as any).show = function (): string { return "own-version"; };
console.log("shadowed=" + shadowed.show());
delete (shadowed as any).show;
console.log("after_delete=" + shadowed.show());

// 17) `Reflect.apply` supplies the receiver without touching the function.
console.log("reflect_apply=" + Reflect.apply(Account.prototype.show, acct, []));
