---
description: Redundant Val conversion — avoid unnecessary conversions to and from Val
sidebar_position: 11
---

# `redundant_val_conversion`

| Default Severity | Category     |
| ---------------- | ------------ |
| `warn`           | Compute      |

## What it does

Flags redundant or unnecessary conversions to and from Soroban `Val` types.

## Why is this bad

Unnecessary type conversions introduce avoidable instruction overhead and bloat the generated Wasm code.

## Example

**Bad** — redundant conversions:

```rust
// ❌ Triggers: redundant_val_conversion
let val: Val = x.into();
let back: u32 = val.try_into().unwrap();
```

**Good** — use direct types without redundant conversions:

```rust
// ✅ Fixed: direct usage
let x_val = x;
```

## Suggested Fix

Remove unnecessary `.into()` or `Val` conversion wrappers when working with native SDK types.

## Cost Impact

- **CPU instructions:** Redundant type conversions add unnecessary Wasm instruction executions, wasting CPU budget.
