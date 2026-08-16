// Cross-runtime: global exec advances lastIndex and resets it after failure.
const re = /a./g;
console.log(JSON.stringify(re.exec("a1 a2")), re.lastIndex);
console.log(JSON.stringify(re.exec("a1 a2")), re.lastIndex);
console.log(re.exec("a1 a2"), re.lastIndex);

