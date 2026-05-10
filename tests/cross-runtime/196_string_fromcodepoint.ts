// Cross-runtime: String.fromCodePoint
console.log("ABC=" + String.fromCodePoint(65, 66, 67));
console.log("hello=" + String.fromCodePoint(104, 101, 108, 108, 111));

// Emoji
console.log("emoji=" + String.fromCodePoint(0x1F600));
console.log("multi=" + String.fromCodePoint(0x1F600, 65, 0x1F601));

// Compare with fromCharCode
console.log("charCode=" + String.fromCharCode(0x1F600));
