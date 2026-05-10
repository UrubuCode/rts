// Cross-runtime: equality operators (loose vs strict).
console.log("null==undefined=" + (null == undefined));
console.log("undefined==null=" + (undefined == null));
console.log("null==null=" + (null == null));
console.log("undefined==undefined=" + (undefined == undefined));
console.log("null===undefined=" + (null === (undefined as any)));
console.log("0==false=" + (0 == (false as any)));
console.log("1==true=" + (1 == (true as any)));
console.log("0===false=" + (0 === (false as any)));
console.log("1=='1'=" + (1 == ("1" as any)));
console.log("1==='1'=" + (1 === ("1" as any)));
console.log("NaN==NaN=" + (NaN == NaN));
console.log("NaN===NaN=" + (NaN === NaN));
