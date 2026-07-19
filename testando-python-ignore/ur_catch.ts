Promise.reject(new Error("caught-y")).catch((e: any) => { console.log("caught:" + e.message); });
