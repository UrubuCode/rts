import { io, collections } from "rts";

const nums = [10, 20, 30];
io.print(`vec_len=${collections.vec_len(nums)}`);
io.print(`nums[0]=${nums[0]}`);
io.print(`nums[2]=${nums[2]}`);

const point = { x: 1, y: 2 };
io.print(`point.x=${point.x} point.y=${point.y}`);
io.print(`point["y"]=${point["y"]}`);

const x = 7;
const y = 8;
const sh = { x, y };
io.print(`sh.x=${sh.x} sh.y=${sh.y}`);

const matrix = [[1, 2], [3, 4]];
const row0 = matrix[0];
const row1 = matrix[1];
io.print(`matrix[0][0]=${row0[0]} matrix[1][1]=${row1[1]}`);

const objs = [{ v: 100 }, { v: 200 }];
io.print(`objs[0].v=${objs[0].v}`);
io.print(`objs[1].v=${objs[1].v}`);

const data = { items: [11, 22, 33] };
const items = data.items;
io.print(`data.items[1]=${items[1]}`);
