// Cross-runtime: typeof on TDZ binding throws.
{
  try {
    console.log(typeof x);
  } catch (e: any) {
    console.log(e.constructor.name);
  }
  let x = 1;
  console.log(x);
}
