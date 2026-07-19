async function f() { try { await Promise.reject(new Error("caught-z")); } catch (e: any) { console.log("await-caught:" + e.message); } }
f();
