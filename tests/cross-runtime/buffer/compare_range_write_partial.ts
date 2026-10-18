const t=(f:()=>unknown)=>{try{console.log(f())}catch(e:any){console.log(e.code,e.message)}};
const A=Buffer.from('ABC'),B=Buffer.from('BCD');
t(()=>A.compare(B,0,1,1,2));
t(()=>A.compare(B,1,3,0,2));
t(()=>A.compare(B,3,3,3,3));
t(()=>A.compare(B,0,0,0,3));
t(()=>A.compare(B,2,1));
t(()=>A.compare(B,0,3,2,1));
t(()=>A.compare(B,-1));
t(()=>A.compare(B,4));
t(()=>A.compare(B,0,4));
t(()=>A.compare(B,0,3,4));
t(()=>A.compare(B,0,3,0,4));
t(()=>A.compare(B,0,-1));
t(()=>A.compare(B,undefined,undefined,1));
t(()=>A.compare(B,1.5));
t(()=>A.compare(B,'1'));
t(()=>A.compare(B,0,3,0,undefined));
t(()=>A.compare(B,0,undefined,1,2));
t(()=>A.compare(B,3,2));
t(()=>A.compare(B,0,3,3,2));
t(()=>A.compare(B,NaN));
t(()=>A.compare(B,0,2,0,1));
for (const n of [0,1,2,3,4,5]) { const b=Buffer.alloc(n); const r=b.write('€'); console.log(n,r,b.toString('hex')); }
{const b=Buffer.alloc(2);console.log(b.write('a€'),b.toString('hex'));}
{const b=Buffer.alloc(4);console.log(b.write('a€€'),b.toString('hex'));}
{const b=Buffer.alloc(6);console.log(b.write('x😀',1),b.toString('hex'));}
{const b=Buffer.alloc(8);console.log(b.write('😀😀',0,6),b.toString('hex'));}
{const b=Buffer.alloc(8);console.log(b.write('é€',0,2,'utf8'),b.toString('hex'));}
{const b=Buffer.alloc(8);console.log(b.utf8Write('€€',0,5),b.toString('hex'));}
{const b=Buffer.alloc(3);console.log(b.write('\ud800ab'),b.toString('hex'));}
{const b=Buffer.alloc(2);console.log(b.write('\ud800'),b.toString('hex'));}
t(()=>A.compare(B,0,3,-1));
t(()=>A.compare(B,0,3,0.5));
t(()=>A.compare(B,0,3,'1'));
t(()=>A.compare(B,0,'1'));
t(()=>A.compare(B,0,3,0,1.5));
t(()=>A.compare(B,0,3,0,NaN));
t(()=>A.compare(B,null));
t(()=>A.compare(B,0,null));
t(()=>A.compare(B,Infinity));
t(()=>A.compare(B,2**53));
t(()=>A.compare(B,0,3,2**53));
t(()=>A.compare(B,-1,-1));
t(()=>A.compare(B,0,3,0,0));
t(()=>A.compare(Buffer.from('ABC')));t(()=>A.compare(5 as any));
