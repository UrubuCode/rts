const LCG_MOD: number = 2147483647;
const LIMB_BASE: number = 1000000;

let limb2: number = 0;
let limb1: number = 0;
let limb0: number = 1;

let arithmetic_score: number = 0;
let prime_score: number = 0;
let bigint_like_score: number = 0;
let final_score: number = 0;

function emit(message: string): void {
    console.log(message);
}

function arithmetic_stress(rounds: number): void {
    let acc: number = 123456;
    let i: number = 0;

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

function count_primes(limit: number): void {
    let count: number = 0;
    let n: number = 2;

    while (n <= limit) {
        let d: number = 2;
        let prime: boolean = true;

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

function bigint_like_stress(rounds: number): void {
    let i: number = 0;
    let checksum: number = 0;
    let mul: number = 0;
    let add: number = 0;
    let v0: number = 0;
    let n0: number = 0;
    let c0: number = 0;
    let v1: number = 0;
    let n1: number = 0;
    let c1: number = 0;
    let v2: number = 0;
    let n2: number = 0;

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
    const mix_a: number = arithmetic_score + prime_score;
    const mix_b: number = bigint_like_score + limb0 + limb1 + limb2;
    const mode: number = (limb0 + limb1 + limb2) % 3;

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
