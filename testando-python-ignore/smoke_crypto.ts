const u = crypto.random_uuid();
console.log("uuid-len:" + u.length);
console.log("uuid-dashes:" + (u.split("-").length === 5));
const b = new Blob(["hello"]);
console.log("blob-size:" + b.size);
