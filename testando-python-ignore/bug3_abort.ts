const ac = new AbortController();
console.log("before:"+ac.signal.aborted);
ac.abort();
console.log("after:"+ac.signal.aborted);
