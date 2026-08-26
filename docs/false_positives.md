# Handling False Positives

Static analysis tools occasionally flag code that is intentionally written the way it is. This guide explains how to recognize, suppress, evaluate, and track false positives in `soroban-cost-linter`.

## What is a False Positive?

A false positive is a lint warning that fires on code that does not actually contain the problem the lint is designed to catch, or where the flagged cost is intentional, unavoidable, or inherent to the contract's business logic.

For example, `soroban_storage_in_loop` warns when a storage operation appears inside a loop body. In most code this is an expensive anti-pattern, but if you are intentionally writing different keys on each iteration (e.g., writing a batch of entries), the warning is a false positive — the code is correct, and the cost is inherent to the operation.

---

## Real-World Corpus Baseline & Tracking

The project runs continuous regression and triage checks against real-world Soroban contracts in `tests/corpus/` via `cargo-cost-lint/tests/real_world_corpus.rs`. The findings are recorded in `tests/corpus/baseline.json`.

### Current Corpus Baseline Statistics

| Metric | Count | Percentage |
|---|---:|---:|
| **Total Findings** | 93 | 100.0% |
| **True Positives (TP)** | 17 | 18.3% |
| **False Positives (FP)** | 76 | 81.7% |

### New Cross-Contract Call Corpus Contracts Analysis

- **`cross_contract_call_simple`**: Makes a single external cross-contract call (`env.invoke_contract`) outside of a loop to settle a transfer. Emits **0 findings**. This is **correct** (no loop-based contract calls or other anti-patterns). 
- **`cross_contract_call_loop`**: Batches multiple cross-contract calls inside a `while` loop iterating over recipients. Emits **1 finding** (`contract_call_in_loop`). This is a **false positive / intentional batch dispatch pattern**; the contract is designed to batch-settle transfers across multiple accounts in a single transaction, where cross-contract invocation per item is required by the business logic.

---

## Known False Positive Patterns by Lint

### `contract_call_in_loop`

Flags cross-contract calls (`env.invoke_contract`) inside loop bodies.

- **Batch Dispatches:** When performing batch distributions or multi-recipient settlements through token or external contracts, looping over items and invoking the contract per item is correct and intentional. Suppress with `#[allow(contract_call_in_loop)]` if needed.
