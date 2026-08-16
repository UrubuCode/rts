// Cross-runtime: RegExp exposes normalized flag and boolean property values.
const re = new RegExp("a.b", "mis");
console.log(re.source, re.flags);
console.log(re.global, re.ignoreCase, re.multiline, re.dotAll, re.unicode, re.sticky);

