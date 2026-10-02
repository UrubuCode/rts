// A CommonJS module imported with ESM syntax — the half of the symmetry that
// must keep working. `require` became live in `eec029f9c`; this file is here so
// that making a named `import` live does not take the CommonJS side with it.

exports.cv = 7;
exports.cf = function () {
    return 8;
};
