#![allow(unused)]
#![warn(redundant_val_conversion)]

use soroban_sdk::{Env, Val, IntoVal, TryFromVal};

fn flag_same_type(env: Env, num: u32) {
    // Should warn: redundant conversion to the same type
    let _ = num.into_val(&env); // But wait, trait IntoVal in stub returns `u32` for `num`? 
    // Yes, we implemented `IntoVal<Env, u32>` for `u32`, so `num.into_val` could resolve to that, but the compiler might complain about ambiguity without type annotations.
}

fn flag_round_trip(env: Env, num: u32) {
    // Should warn: redundant round-trip conversion
    let val: Val = num.into_val(&env);
    // actually inline:
    let _ = u32::try_from_val(&env, &num.into_val(&env));
}

fn good_valid_conversion(env: Env, num: u32) {
    // Good: u32 -> Val
    let _val: Val = num.into_val(&env);
}

#[allow(redundant_val_conversion)]
fn allowed_round_trip(env: Env, num: u32) {
    // Good (allowed)
    let _ = u32::try_from_val(&env, &num.into_val(&env));
}

fn main() {}
