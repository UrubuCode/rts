// Cross-runtime: replaceAll handles literal dollar substitutions globally.
console.log("a.b.a.b".replaceAll(".", "$&$&"));
console.log("xxxx".replaceAll("xx", "[$$]"));
console.log("abc".replaceAll("", "-"));

