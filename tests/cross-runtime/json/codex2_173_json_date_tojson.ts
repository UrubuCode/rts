// Cross-runtime: Date values serialize through their ISO toJSON representation.
const value = { when: new Date("2020-02-03T04:05:06.789Z") };
console.log(JSON.stringify(value));
console.log(value.when.toJSON());

