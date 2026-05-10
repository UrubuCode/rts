// Cross-runtime: bitwise operators on positive and negative values.
console.log("and=" + (6 & 3));
console.log("or=" + (6 | 3));
console.log("xor=" + (6 ^ 3));
console.log("not0=" + (~0));
console.log("lsh=" + (5 << 2));
console.log("rsh=" + (-8 >> 1));
console.log("trunc=" + (~~3.7));
console.log("hi_bit=" + (1 << 31));
console.log("mix=" + ((-5 & 3) + ":" + (-5 | 3)));
