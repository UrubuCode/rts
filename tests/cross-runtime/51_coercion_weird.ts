// Cross-runtime compatibility: weird coercion corners.
console.log("plus_arr=" + ([] + []));
console.log("plus_obj=" + ({} + ""));
console.log("num_empty=" + Number(""));
console.log("num_space=" + Number("   "));
console.log("num_null=" + Number(null));
console.log("num_true=" + Number(true));
console.log("str_arr=" + String([1, null, 3]));
console.log("eq_a=" + ([] == ""));
console.log("eq_b=" + ([0] == 0));
console.log("eq_c=" + ("" == 0));
console.log("eq_d=" + (false == "0"));
