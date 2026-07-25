pub mod soroban_sdk {
    pub struct Env;
    impl Clone for Env {
        fn clone(&self) -> Self { Env }
    }
    impl Env {
        pub fn storage(&self) -> storage::Storage {
            storage::Storage
        }
        pub fn ledger(&self) -> ledger::Ledger {
            ledger::Ledger
        }
        pub fn host(&self) -> host::Host {
            host::Host
        }
    }

    pub mod storage {
        pub struct Storage;
        impl Storage {
            pub fn instance(&self) -> Instance { Instance }
            pub fn persistent(&self) -> Persistent { Persistent }
            pub fn temporary(&self) -> Temporary { Temporary }
        }

        pub struct Instance;
        impl Instance {
            pub fn get<K, V>(&self, _k: &K) -> Option<V> { None }
            pub fn set<K, V>(&self, _k: &K, _v: &V) {}
            pub fn has<K>(&self, _k: &K) -> bool { false }
        }

        pub struct Persistent;
        impl Persistent {
            pub fn get<K, V>(&self, _k: &K) -> Option<V> { None }
            pub fn set<K, V>(&self, _k: &K, _v: &V) {}
            pub fn has<K>(&self, _k: &K) -> bool { false }
        }

        pub struct Temporary;
        impl Temporary {
            pub fn get<K, V>(&self, _k: &K) -> Option<V> { None }
            pub fn set<K, V>(&self, _k: &K, _v: &V) {}
            pub fn has<K>(&self, _k: &K) -> bool { false }
        }
    }

    pub mod ledger {
        pub struct Ledger;
        impl Ledger {
            pub fn sequence(&self) -> u32 { 0 }
        }
    }

    pub mod host {
        pub struct Host;
        impl Host {
            pub fn invoke_contract(&self) {}
            pub fn invoke_static(&self) {}
            pub fn budget_cloned(&self) {}
        }
    }

    pub struct Vec<T>(core::marker::PhantomData<T>);
    impl<T> Vec<T> {
        pub fn len(&self) -> u32 { 0 }
        pub fn push(&self, _val: &T) {}
        pub fn pop(&self) -> Option<T> { None }
        pub fn get(&self, _i: u32) -> Option<T> { None }
        pub fn remove(&self, _i: u32) -> T { todo!() }
    }

    pub struct Map<K, V>(core::marker::PhantomData<(K, V)>);
    impl<K, V> Map<K, V> {
        pub fn len(&self) -> u32 { 0 }
        pub fn set(&self, _k: &K, _v: &V) {}
        pub fn get(&self, _k: &K) -> Option<V> { None }
        pub fn insert(&self, _k: &K, _v: &V) {}
        pub fn remove(&self, _k: &K) {}
    }

    pub struct Set<T>(core::marker::PhantomData<T>);
    impl<T> Set<T> {
        pub fn len(&self) -> u32 { 0 }
        pub fn insert(&self, _val: &T) {}
        pub fn remove(&self, _val: &T) {}
    }
}

use soroban_sdk::Env;

// =======================================================================
// soroban_storage_in_loop — Fixtures
// =======================================================================

fn bad_storage_in_for_loop(env: Env) {
    for i in 0..10 {
        env.storage().instance().set(&i, &1); // Should Warn
    }
}

fn bad_storage_in_while_loop(env: Env) {
    let mut i = 0;
    while i < 10 {
        let _: Option<i32> = env.storage().persistent().get(&i); // Should Warn
        i += 1;
    }
}

fn bad_storage_in_loop_loop(env: Env) {
    loop {
        if env.storage().temporary().has(&1) { // Should Warn
            break;
        }
    }
}

fn good_storage_outside_loop(env: Env) {
    env.storage().instance().set(&1, &1); // Good
}

#[allow(soroban_storage_in_loop)]
fn allowed_storage_in_loop(env: Env) {
    for i in 0..10 {
        env.storage().instance().set(&i, &1); // Good (allowed)
    }
}

// =======================================================================
// redundant_env_clone — Fixtures
// =======================================================================

fn bad_clone_env(env: Env) {
    let _cloned = env.clone(); // Should Warn
}

fn good_no_clone_needed(env: Env) {
    let _ref = &env; // Good — no clone, just a reference
}

#[allow(redundant_env_clone)]
fn allowed_clone_env(env: Env) {
    let _cloned = env.clone(); // Good (allowed)
}

// =======================================================================
// unnecessary_host_function_call — Fixtures
// =======================================================================

fn bad_host_call_in_loop(env: Env) {
    for _ in 0..10 {
        let _seq = env.ledger().sequence(); // Should Warn
    }
}

fn good_host_call_outside_loop(env: Env) {
    let seq = env.ledger().sequence(); // Good — called once before the loop
    for _ in 0..10 {
        let _seq = seq;
    }
}

#[allow(unnecessary_host_function_call)]
fn allowed_host_call_in_loop(env: Env) {
    for _ in 0..10 {
        let _seq = env.ledger().sequence(); // Good (allowed)
    }
}

// =======================================================================
// collection_len_in_loop_condition — Fixtures
// =======================================================================

use soroban_sdk::{Vec, Map, Set};

fn bad_while_len_no_mutation(env: Env) {
    let vec: Vec<i32> = Vec();
    let mut i = 0;
    while i < vec.len() { // Should Warn
        let _x = vec.get(i);
        i += 1;
    }
}

fn bad_for_range_len_no_mutation(env: Env) {
    let vec: Vec<i32> = Vec();
    for _ in 0..vec.len() { // Should Warn
        let _x = vec.get(0);
    }
}

fn bad_map_len_in_while(env: Env) {
    let map: Map<u32, u32> = Map();
    let mut i = 0;
    while i < map.len() { // Should Warn
        let _x = map.get(&i);
        i += 1;
    }
}

fn bad_set_len_in_for(env: Env) {
    let set: Set<u32> = Set();
    for _ in 0..set.len() { // Should Warn
        let _x = set.remove(&0u32);
    }
}

fn good_while_len_with_mutation(env: Env) {
    let vec: Vec<i32> = Vec();
    let mut i = 0;
    while i < vec.len() { // Good — vec mutated in body
        vec.push(&i);
        i += 1;
    }
}

fn good_for_range_len_with_mutation(env: Env) {
    let vec: Vec<i32> = Vec();
    for _ in 0..vec.len() { // Good — vec mutated in body
        vec.push(&42i32);
    }
}

fn good_len_outside_loop(env: Env) {
    let vec: Vec<i32> = Vec();
    let _len = vec.len(); // Good — not in a loop
}

fn good_len_on_non_collection(env: Env) {
    let _len = 42u32; // Good — not a Soroban collection
}

#[allow(collection_len_in_loop_condition)]
fn allowed_len_in_loop(env: Env) {
    let vec: Vec<i32> = Vec();
    let mut i = 0;
    while i < vec.len() { // Good (allowed)
        i += 1;
    }
}

fn main() {}
