import { bool, i32, io, str } from "rts";

const LCG_MOD: i32 = 2147483647;
const LIMB_BASE: i32 = 1000000;

let limb2: i32 = 0;
let limb1: i32 = 0;
let limb0: i32 = 1;

let arithmetic_score: i32 = 0;
let prime_score: i32 = 0;
let bigint_like_score: i32 = 0;
let final_score: i32 = 0;

function emit(message: str): void {
    io.print(message);
}

function arithmetic_stress(rounds: i32): void {
    let acc: i32 = 123456;
    let i: i32 = 0;

    while (i < rounds) {
        acc = (acc * 1664525 + 1013904223) % LCG_MOD;

        if ((i % 7) === 0) {
            acc = (acc + i) % LCG_MOD;
        } else if ((i % 11) === 0) {
            acc = (acc - i) % LCG_MOD;
        } else {
            acc = (acc + (i % 97) * (i % 89)) % LCG_MOD;
        }

        if (acc < 0) {
            acc = acc + LCG_MOD;
        }

        i = i + 1;
    }

    arithmetic_score = acc;
}

function count_primes(limit: i32): void {
    let count: i32 = 0;
    let n: i32 = 2;

    while (n <= limit) {
        let d: i32 = 2;
        let prime: bool = true;

        while ((d * d) <= n) {
            if ((n % d) === 0) {
                prime = false;
                break;
            }
            d = d + 1;
        }

        if (prime) {
            count = count + 1;
        }

        n = n + 1;
    }

    prime_score = count;
}

function bigint_like_stress(rounds: i32): void {
    let i: i32 = 0;
    let checksum: i32 = 0;
    let mul: i32 = 0;
    let add: i32 = 0;
    let v0: i32 = 0;
    let n0: i32 = 0;
    let c0: i32 = 0;
    let v1: i32 = 0;
    let n1: i32 = 0;
    let c1: i32 = 0;
    let v2: i32 = 0;
    let n2: i32 = 0;

    if (rounds > 0) {
        do {
            mul = 31 + (i % 17);
            add = i % 97;

            v0 = limb0 * mul + add;
            n0 = v0 % LIMB_BASE;
            c0 = (v0 - n0) / LIMB_BASE;

            v1 = limb1 * mul + c0;
            n1 = v1 % LIMB_BASE;
            c1 = (v1 - n1) / LIMB_BASE;

            v2 = limb2 * mul + c1;
            n2 = v2 % LIMB_BASE;

            limb0 = n0;
            limb1 = n1;
            limb2 = n2;

            checksum = (checksum + n0 + n1 + n2) % LCG_MOD;
            i = i + 1;
        } while (i < rounds);
    }

    bigint_like_score = checksum;
}

function mix_scores(): void {
    const mix_a: i32 = arithmetic_score + prime_score;
    const mix_b: i32 = bigint_like_score + limb0 + limb1 + limb2;
    const mode: i32 = (limb0 + limb1 + limb2) % 3;

    switch (mode) {
        case 0:
            final_score = mix_a + mix_b;
            break;
        case 1:
            final_score = (mix_a + mix_b + 17) - 17;
            break;
        default:
            final_score = (mix_a + mix_b + 31) - 31;
            break;
    }
}

arithmetic_stress(80000);
count_primes(2500);
bigint_like_stress(120000);
mix_scores();

emit("bench-checksum:" + final_score);