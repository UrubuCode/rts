// Cross-runtime: UnicodeSets intersection under v flag composes character properties.
const consonants = new RegExp("[\\p{ASCII}&&\\p{Letter}&&[^AEIOUaeiou]]+", "gv");
console.log(JSON.stringify("abc XYZ élan 123".match(consonants)));
console.log(consonants.unicodeSets, consonants.flags);

