// Cross-runtime: control flow flattening via switch+state machine (js-confuser output).
// Original: sequential steps A→B→C→D→E flattened into switch dispatch.

function _0xf1(input: number): number {
  let _s = "3|1|4|0|2";
  const _order = _s.split("|").map(Number);
  let _i = 0;
  let result = 0;
  let tmp = 0;

  while (_i < _order.length) {
    switch (_order[_i++]) {
      case 0: result = tmp * 2; continue;
      case 1: tmp = input + 10; continue;
      case 2: result = result - 1; continue;
      case 3: result = 0; tmp = 0; continue;
      case 4: result = tmp + result; continue;
    }
  }
  return result;
}
// order executes: 3(reset), 1(tmp=input+10), 4(result=tmp+result=input+10), 0(result*=2), 2(result-=1)
// for input=5: tmp=15, result=0+15=15, result=30, result=29
console.log(_0xf1(5));   // 29
console.log(_0xf1(0));   // 19  (tmp=10, result=10, *2=20, -1=19)
console.log(_0xf1(10));  // 39  (tmp=20, result=20, *2=40, -1=39)

// nested flattened with fake dead branches
function _0xf2(x: number, y: number): string {
  let _state = 0;
  let out = "";
  while (true) {
    switch (_state) {
      case 0: out = ""; _state = (x > 0) ? 1 : 3; break;
      case 1: out += "pos"; _state = (y > 0) ? 2 : 4; break;
      case 2: out += "+pos"; _state = 5; break;
      case 3: out += "non-pos"; _state = (y > 0) ? 4 : 5; break;
      case 4: out += "+non-pos"; _state = 5; break;
      case 5: return out;
    }
  }
}
console.log(_0xf2(1, 1));
console.log(_0xf2(1, -1));
console.log(_0xf2(-1, 1));
console.log(_0xf2(-1, -1));
