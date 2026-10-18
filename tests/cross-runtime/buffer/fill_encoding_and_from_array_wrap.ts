// Buffer#fill argument forms (encoding in offset/end slot, bounds errors, empty/invalid patterns) and Buffer.from(array) ToUint8 modulo wrap.
const t = (f: () => any) => {
  try {
    const v = f();
    console.log(Buffer.isBuffer(v) ? "buf:" + v.toString("hex") : String(v));
  } catch (e: any) {
    console.log(e.name, e.code);
  }
};
t(()=>Buffer.alloc(3).fill(1,0,10));
t(()=>Buffer.alloc(3).fill(1,-1));
t(()=>Buffer.alloc(3).fill(1,4));
t(()=>Buffer.alloc(3).fill(1,"a"));
t(()=>Buffer.alloc(3).fill(1,0,"a"));
t(()=>Buffer.alloc(3).fill("zz","hex"));
t(()=>Buffer.alloc(3).fill(Buffer.alloc(0)));
t(()=>Buffer.alloc(3).fill("a","nope"));
t(()=>Buffer.alloc(3).fill("a",0,2,"nope"));
t(()=>Buffer.alloc(3).fill("a",0,1.5));
t(()=>Buffer.alloc(3).fill("a",1.5));
t(()=>Buffer.alloc(3).fill("a",NaN));
t(()=>Buffer.alloc(3).fill("a",0,NaN));
t(()=>Buffer.alloc(3).fill("a",Infinity));
t(()=>Buffer.alloc(3).fill("a",0,undefined,"hex"));
t(()=>Buffer.alloc(3).fill("a",undefined,undefined,"hex"));
t(()=>Buffer.alloc(3).fill("6162",undefined,undefined,"hex").toString("hex"));
t(()=>Buffer.alloc(3).fill(1,0,2,"nope").toString("hex"));
t(()=>Buffer.alloc(4).fill("é",1,3).toString("hex"));
t(()=>Buffer.alloc(4).fill("é","latin1").toString("hex"));
t(()=>Buffer.alloc(4).fill({}).toString("hex"));
t(()=>Buffer.alloc(4).fill(null).toString("hex"));
t(()=>Buffer.alloc(4).fill(undefined).toString("hex"));
t(()=>Buffer.alloc(4).fill("a",3,2).toString("hex"));
t(()=>Buffer.from([1,257,-1,3.7,"5",NaN,300,-300,2**32+7]).toString("hex"));
t(()=>Buffer.alloc(8).fill("abc").toString("hex"));
t(()=>Buffer.alloc(6).fill("abc",1,5).toString("hex"));
t(()=>Buffer.alloc(5).fill("6162","hex").toString("hex"));
t(()=>Buffer.alloc(6).fill("6162",1,"hex").toString("hex"));
t(()=>Buffer.alloc(6).fill("6162",1,4,"hex").toString("hex"));
t(()=>Buffer.alloc(6).fill("ab","utf16le").toString("hex"));
t(()=>Buffer.alloc(6).fill("aGk=","base64").toString("hex"));
t(()=>Buffer.alloc(3,7).fill("").toString("hex"));
t(()=>Buffer.alloc(5).fill(Buffer.from([1,2])).toString("hex"));
t(()=>Buffer.alloc(4).fill(0x101,1).toString("hex"));
t(()=>Buffer.alloc(3).fill(-1).toString("hex"));
