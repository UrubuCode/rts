// Cross-runtime: String trim variants
const str = "  hello world  ";
console.log("trim=" + str.trim());
console.log("trimStart=" + str.trimStart());
console.log("trimEnd=" + str.trimEnd());

const tabs = "\t\thello\t\t";
console.log("tabs=" + tabs.trim());

const newlines = "\n\nhello\n\n";
console.log("newlines=" + newlines.trim());

const mixed = " \t\n hello \n\t ";
console.log("mixed=" + mixed.trim());
