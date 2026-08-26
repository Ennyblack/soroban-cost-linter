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
| **Total Findings** | 96 | 100.0% |
| **True Positives (TP)** | 17 | 17.7% |
| **False Positives (FP)** | 79 | 82.3% |

### New Cross-Contract Corpus Contracts Triage

1. **`cross_contract_call_outside_loop`**: Makes a single cross-contract transfer via `env.invoke_contract` outside of any loop. Result: **0 findings**. Correctly avoids flagging non-loop contract calls.
2. **`cross_contract_batch_settlement`**: Performs a batch token transfer dispatch inside a `while` loop over recipient and amount collections. Result: `contract_call_in_loop` (1 finding, correct true/intentional positive for batch settlement), `soroban_storage_in_loop` (1 finding, false positive for collection index reads), and `loop_invariant_storage_access` (2 findings, false positives for vector handle access).

---

## Known False Positive Patterns by Lint

Refer to codebase history and documentation for specific lint suppression guidelines.
