// Cross-runtime: String concat
const str1 = "hello";
const str2 = " ";
const str3 = "world";

console.log("concat=" + str1.concat(str2, str3));
console.log("concat_empty=" + str1.concat(""));
console.log("concat_multi=" + "a".concat("b", "c", "d"));

// Compare with +
console.log("plus=" + (str1 + str2 + str3));

// With numbers
console.log("with_num=" + "value:".concat(String(42)));
