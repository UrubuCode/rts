// toFixed: banker's? Nao - round-half-to-even-ish do double; casos famosos
console.log((0.1).toFixed(1));     // "0.1"
console.log((1.005).toFixed(2));   // "1.00" (1.005 nao e' exato)
console.log((2.005).toFixed(2));   // "2.00"
console.log((0.5).toFixed(0));     // "1" ou "0"? -> "1" em V8
console.log((1.5).toFixed(0));     // "2"
console.log((2.5).toFixed(0));     // "3" ou "2"? -> "3"
console.log((0.000001).toFixed(2)); // "0.00"
console.log((123.456).toFixed(2)); // "123.46"
console.log((-1.5).toFixed(0));    // "-2"
console.log((8.575).toFixed(2));   // "8.57" (8.575 < 8.575 real)
console.log((1.255).toFixed(2));   // "1.25"
console.log((0).toFixed(3));       // "0.000"
console.log((1e21).toFixed(2));    // "1e+21"