# `redundant_env_clone`

**Default Severity:** `warn`

**Target Resource:** [CPU — memory allocation, copy, and host object dispatch](../cost_rationale.md#per-lint-resource-summary)

## What it does

Detects unnecessary `.clone()` calls on the Soroban `Env` object.

## Why is this bad?

{% hint style="danger" %}
The Soroban `Env` object is designed to be highly lightweight and is typically passed by value or reference. Cloning it forces `MemAlloc` and `MemCpy` operations followed by a `VisitObject` of the new handle — all unnecessary CPU cycles that the network charges for. See the [Cost Rationale — Metered Resources](../cost_rationale.md#1-cpu-instructions) for the cost types involved.
{% endhint %}

## Cost impact

Cloning `Env` incurs `MemAlloc` and `MemCpy` operations followed by a `VisitObject` of the new handle — unnecessary CPU cycles charged to the transaction. `Env` is designed to be passed by value or reference without cloning.

Measured with `Env::default()` in the [`cost_benchmarks`](https://github.com/Tollcraft/soroban-cost-linter/tree/main/cost_benchmarks) crate (`cargo test -- --nocapture`):

| Pattern | Iterations | CPU instructions (delta) | Memory bytes (delta) |
| --- | --- | --- | --- |
| `env.clone()` (bad) | 100 | *run `cargo test -- --nocapture` in `cost_benchmarks/`* | *run `cargo test -- --nocapture` in `cost_benchmarks/`* |
| `&env` (good) | 100 | *run `cargo test -- --nocapture` in `cost_benchmarks/`* | *run `cargo test -- --nocapture` in `cost_benchmarks/`* |

{% hint style="info" %}
The cost of an individual `env.clone()` is small, but in hot paths (e.g., contract entry points called thousands of times across a protocol's lifetime) the cumulative overhead is real and avoidable.
{% endhint %}

### How to reproduce

```bash
cd cost_benchmarks
cargo test bench_env_clone_vs_reuse -- --nocapture
```

## Example

```rust
// ❌ Bad: Env is lightweight — no clone needed
let my_env = env.clone();
```

## Suggested Fix

{% hint style="success" %}
Pass `env` directly by value or reference without calling `.clone()`.
{% endhint %}
