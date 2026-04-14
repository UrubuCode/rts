import { io } from "rts";

function main(): void {
  // 1. array simples de numeros
  const nums = JSON.parse("[1,2,3]");
  io.print(JSON.stringify(nums));
  io.print(nums.length);

  // 2. roundtrip preserva array
  const original = "[10,20,30]";
  const roundtrip = JSON.stringify(JSON.parse(original));
  io.print(roundtrip);
  io.print(roundtrip == original);

  // 3. array vazio
  io.print(JSON.stringify(JSON.parse("[]")));

  // 4. array de strings
  io.print(JSON.stringify(JSON.parse('["a","b","c"]')));

  // 5. array de booleans e null
  io.print(JSON.stringify(JSON.parse("[true,false,null]")));

  // 6. objeto dentro de array
  const mixed = JSON.parse('[{"name":"alice","age":30},{"name":"bob","age":25}]');
  io.print(JSON.stringify(mixed));
  io.print(mixed.length);
  io.print(mixed[0].name);
  io.print(mixed[1].age);

  // 7. array dentro de objeto dentro de array
  const deep = JSON.parse('[{"tags":["rust","ts"]},{"tags":["wasm"]}]');
  io.print(JSON.stringify(deep));
  io.print(deep[0].tags.length);
  io.print(deep[1].tags[0]);

  // 8. array aninhado (array de arrays)
  const nested = JSON.parse("[[1,2],[3,4],[5]]");
  io.print(JSON.stringify(nested));
  io.print(nested.length);
  io.print(nested[0].length);
  io.print(JSON.stringify(nested[1]));

  // 9. objeto com multiplos arrays
  const obj = JSON.parse('{"x":[1,2],"y":[3,4],"z":"hello"}');
  io.print(JSON.stringify(obj));
  io.print(obj.x.length);
  io.print(obj.y[1]);
  io.print(obj.z);

  // 10. array com tipos mistos
  const mix = JSON.parse('[1,"two",true,null,{"k":"v"},[9]]');
  io.print(JSON.stringify(mix));
  io.print(mix.length);
}
