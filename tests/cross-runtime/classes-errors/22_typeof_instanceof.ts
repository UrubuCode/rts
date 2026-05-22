// Cross-runtime: typeof and instanceof.
class BaseThing {}
class SubThing extends BaseThing {}

function fn() {
  return 1;
}

console.log("t_num=" + typeof 1);
console.log("t_str=" + typeof "x");
console.log("t_bool=" + typeof true);
console.log("t_undef=" + typeof undefined);
console.log("t_fn=" + typeof fn);
console.log("t_obj=" + typeof { a: 1 });
console.log("t_arr=" + typeof [1, 2]);
console.log("t_null=" + typeof null);

const item = new SubThing();
console.log("i_sub=" + (item instanceof SubThing));
console.log("i_base=" + (item instanceof BaseThing));
console.log("i_obj=" + (item instanceof Object));
