// Cross-runtime: classic algorithms (binary search, sort variants, graph BFS/DFS).

// Binary search
function binarySearch(arr: number[], target: number): number {
  let lo = 0, hi = arr.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (arr[mid] === target) return mid;
    if (arr[mid] < target) lo = mid + 1; else hi = mid - 1;
  }
  return -1;
}
const sorted = [1,3,5,7,9,11,13,15,17,19];
console.log(binarySearch(sorted, 7));   // 3
console.log(binarySearch(sorted, 1));   // 0
console.log(binarySearch(sorted, 19));  // 9
console.log(binarySearch(sorted, 6));   // -1

// Quicksort (in-place)
function quicksort(arr: number[], lo = 0, hi = arr.length - 1): void {
  if (lo >= hi) return;
  const pivot = arr[hi];
  let i = lo;
  for (let j = lo; j < hi; j++) if (arr[j] <= pivot) { [arr[i], arr[j]] = [arr[j], arr[i]]; i++; }
  [arr[i], arr[hi]] = [arr[hi], arr[i]];
  quicksort(arr, lo, i - 1);
  quicksort(arr, i + 1, hi);
}
const q = [5,3,8,1,9,2,7,4,6];
quicksort(q);
console.log(q.join(","));

// BFS on adjacency list
function bfs(graph: Map<number, number[]>, start: number): number[] {
  const visited = new Set<number>();
  const queue = [start];
  const order: number[] = [];
  visited.add(start);
  while (queue.length) {
    const node = queue.shift()!;
    order.push(node);
    for (const nb of (graph.get(node) ?? [])) {
      if (!visited.has(nb)) { visited.add(nb); queue.push(nb); }
    }
  }
  return order;
}
const graph = new Map<number, number[]>([
  [1, [2, 3]], [2, [4, 5]], [3, [6]], [4, []], [5, []], [6, []]
]);
console.log(bfs(graph, 1).join(","));  // 1,2,3,4,5,6

// DFS (iterative)
function dfs(graph: Map<number, number[]>, start: number): number[] {
  const visited = new Set<number>();
  const stack = [start];
  const order: number[] = [];
  while (stack.length) {
    const node = stack.pop()!;
    if (visited.has(node)) continue;
    visited.add(node);
    order.push(node);
    const neighbors = [...(graph.get(node) ?? [])].reverse();
    for (const nb of neighbors) if (!visited.has(nb)) stack.push(nb);
  }
  return order;
}
console.log(dfs(graph, 1).join(","));  // 1,2,4,5,3,6

// LCS (dynamic programming)
function lcs(a: string, b: string): number {
  const m = a.length, n = b.length;
  const dp = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = 1; i <= m; i++)
    for (let j = 1; j <= n; j++)
      dp[i][j] = a[i-1] === b[j-1] ? dp[i-1][j-1] + 1 : Math.max(dp[i-1][j], dp[i][j-1]);
  return dp[m][n];
}
console.log(lcs("ABCBDAB", "BDCAB"));  // 4
