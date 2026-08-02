//! `rts-value-probe` — measure BEFORE implementing.
//!
//! Prices the representation RTS actually uses for each primordial against the
//! plausible alternatives, on the same workload, with every variant removing
//! exactly ONE thing from the one before it:
//!
//! | kernel | primordial | question |
//! |---|---|---|
//! | A | Object (field read) | what does a property read cost, and why |
//! | B | Object (construction) | what does `new` cost |
//! | C | Number / any value | is the NaN-box itself worth redesigning |
//! | ARR | Array | element read/write, boxed vs packed elements |
//! | OBJ | Object (dictionary) | `IndexMap` vs shape+slot on the same read |
//! | BOOL | Boolean | `if (x)` on a Tagged value |
//! | INT | Number (integer) | tagged int32 arithmetic |
//! | STR | String | concat/append and `===` |
//!
//! Read `README.md` for what this does NOT prove.

mod bench;
mod emit;
mod harness;
mod poly;
mod rt;
mod slab;

fn main() {
    println!(
        "rts-value-probe — sizeof(Slot) = {} bytes",
        size_of::<slab::Slot>()
    );
    if cfg!(debug_assertions) {
        eprintln!(
            "\n  !! DEBUG BUILD — these numbers are meaningless.\n     Run: cargo run --release -p rts-value-probe\n"
        );
    }
    println!();

    bench::objects::kernel_a();
    bench::objects::kernel_b();
    bench::types::kernel_arr();
    bench::types::kernel_obj();
    bench::types::kernel_prim();
    bench::strings::kernel_string_append();
    bench::strings::kernel_string_eq();
    bench::repr::kernel_c();
    bench::ops::kernel_ops();
    bench::structs::kernel_struct();
    bench::visible::kernel_visible();
    bench::arch::kernel_arch();
    bench::stdlib::kernel_stdlib();
    bench::closing::kernel_closing();
    bench::final_two::kernel_final();
    bench::complexity::kernel_complexity();
    bench::backend::kernel_backend_gap();
    bench::irladder::kernel_ir_ladder();
    bench::symarch::kernel_sym_arch();
    bench::stdlibshape::kernel_stdlib_shape();
    bench::compiletime::kernel_compile_time();
    bench::numwidth::kernel_num_width();
}
