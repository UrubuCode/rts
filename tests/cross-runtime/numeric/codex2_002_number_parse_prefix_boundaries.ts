// Cross-runtime: numeric parsers stop at the first invalid digit.
console.log([parseInt("10102", 2), parseInt("0xfg", 16), parseInt("077", 8)].join("|"));
console.log([parseFloat("12.5px"), parseFloat("1e2rest"), parseFloat(".75!")].join("|"));

