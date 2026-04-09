import { i32, io } from "rts";

const start_time: i32 = 0; // Placeholder for timing

function heavy_computation(): i32 {
    let result: i32 = 0;
    let i: i32 = 0;

    while (i < 1000000) {
        result = result + (i % 1000);
        i = i + 1;
    }

    return result;
}

let result: i32 = heavy_computation();
io.print("Computation result: " + result);