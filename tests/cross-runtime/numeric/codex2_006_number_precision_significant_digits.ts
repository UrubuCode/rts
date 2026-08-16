// Cross-runtime: toPrecision switches notation while preserving significant digits.
console.log([(123.456).toPrecision(4), (0.00123456).toPrecision(3)].join("|"));
console.log([(9.99).toPrecision(2), (1000).toPrecision(2)].join("|"));

