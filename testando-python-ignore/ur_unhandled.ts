async function boom() { throw new Error("unhandled-x"); }
boom();
console.log("done");
