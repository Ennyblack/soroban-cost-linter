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
// discarded_storage_read — Fixtures
// =======================================================================

fn bad_instance_get_discarded(env: Env) {
    let _key: i32 = 1;
    env.storage().instance().get::<i32, i32>(&_key); // Should Warn
}

fn bad_persistent_get_discarded(env: Env) {
    let _key: i32 = 1;
    env.storage().persistent().get::<i32, i32>(&_key); // Should Warn
}

fn bad_temporary_get_discarded(env: Env) {
    let _key: i32 = 1;
    env.storage().temporary().get::<i32, i32>(&_key); // Should Warn
}

fn bad_instance_get_wildcard(env: Env) {
    let _key: i32 = 1;
    let _ = env.storage().instance().get::<i32, i32>(&_key); // Should Warn
}

fn bad_persistent_get_wildcard(env: Env) {
    let _key: i32 = 1;
    let _ = env.storage().persistent().get::<i32, i32>(&_key); // Should Warn
}

fn bad_temporary_get_wildcard(env: Env) {
    let _key: i32 = 1;
    let _ = env.storage().temporary().get::<i32, i32>(&_key); // Should Warn
}

fn good_has_check(env: Env) {
    let _key: i32 = 1;
    let _exists = env.storage().instance().has(&_key); // Good — has is intentional
}

fn good_get_used_in_if_let(env: Env) {
    let _key: i32 = 1;
    if let Some(_val) = env.storage().instance().get::<i32, i32>(&_key) {
        // Good — result is consumed
    }
}

fn good_get_result_used(env: Env) {
    let _key: i32 = 1;
    let val: Option<i32> = env.storage().persistent().get(&_key);
    let _ = val; // Good — result is bound and read
}

fn good_get_is_some(env: Env) {
    let _key: i32 = 1;
    if env.storage().instance().get::<i32, i32>(&_key).is_some() {
        // Good — used to prove existence
    }
}

#[allow(discarded_storage_read)]
fn allowed_instance_get_discarded(env: Env) {
    let _key: i32 = 1;
    env.storage().instance().get::<i32, i32>(&_key); // Good (allowed)
}

#[allow(discarded_storage_read)]
fn allowed_instance_get_wildcard(env: Env) {
    let _key: i32 = 1;
    let _ = env.storage().instance().get::<i32, i32>(&_key); // Good (allowed)
}

fn main() {}
