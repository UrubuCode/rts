// Cross-runtime compatibility: RegExp indices flag /d.
const match = /a(b)(c)/d.exec("zabc");
console.log("indices=" + JSON.stringify(match?.indices));
