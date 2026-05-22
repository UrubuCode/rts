// Cross-runtime: JSON.stringify / JSON.parse basicos.
console.log("st_obj=" + JSON.stringify({ a: 1, b: "hi" }));
console.log("st_arr=" + JSON.stringify([1, 2, 3]));
console.log("st_bool=" + JSON.stringify({ x: true, y: false }));
console.log("st_null=" + JSON.stringify({ z: null }));
console.log("st_nested=" + JSON.stringify({ a: { b: { c: 1 } } }));
console.log("st_empty_obj=" + JSON.stringify({}));
console.log("st_empty_arr=" + JSON.stringify([]));
console.log("st_str=" + JSON.stringify("hi"));
console.log("st_num=" + JSON.stringify(42));
console.log("st_true=" + JSON.stringify(true));

const arr: any = JSON.parse("[10,20,30]");
console.log("parse_arr_len=" + arr.length);
console.log("parse_arr_0=" + arr[0]);

const obj: any = JSON.parse('{"name":"alice","age":30}');
console.log("parse_obj_name=" + obj.name);
console.log("parse_obj_age=" + obj.age);
