// Cross-runtime compatibility: DataView float read/write.
const view = new DataView(new ArrayBuffer(16));
view.setFloat64(0, Math.PI, true);
view.setFloat32(8, -1.5, false);
console.log("f64=" + view.getFloat64(0, true).toFixed(6));
console.log("f32=" + view.getFloat32(8, false).toFixed(1));
