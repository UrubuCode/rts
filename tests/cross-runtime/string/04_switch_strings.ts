// Cross-runtime: switch statement com strings.
function color(s: string): number {
  switch (s) {
    case "red": return 1;
    case "green": return 2;
    case "blue": return 3;
    default: return 0;
  }
}

function priority(s: string): string {
  switch (s) {
    case "high":
    case "urgent":
      return "important";
    case "low":
      return "minor";
    default:
      return "normal";
  }
}

console.log("color(red)=" + color("red"));
console.log("color(green)=" + color("green"));
console.log("color(yellow)=" + color("yellow"));
console.log("priority(high)=" + priority("high"));
console.log("priority(urgent)=" + priority("urgent"));
console.log("priority(low)=" + priority("low"));
console.log("priority(med)=" + priority("med"));
