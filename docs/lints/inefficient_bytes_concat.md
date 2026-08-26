---
description: Inefficient bytes concatenation — avoid repeated per-iteration concatenations that waste CPU and memory
sidebar_position: 6
---

# `inefficient_bytes_concat`

| Default Severity | Category     |
| ---------------- | ------------ |
| `warn`           | Compute      |

## What it does

Flags repeated concatenation of `Bytes` or `BytesN` objects inside loop bodies using operator-based concatenation.

## Why is this bad

Concatenating bytes repeatedly inside a loop allocates new host objects and copies backing buffers on every iteration, causing quadratic performance degradation and excessive CPU consumption.

## Example

**Bad** — concatenating bytes inside a loop:

```rust
// ❌ Triggers: inefficient_bytes_concat
let mut result = Bytes::new(&env);
for item in &items {
    result = result + item;
}
```

**Good** — preallocate or accumulate using a vector or buffer before converting to host bytes:

```rust
// ✅ Fixed: collect/accumulate outside the loop
let mut buffer = Vec::new();
for item in &items {
    buffer.extend_from_slice(item.to_alloc_vec());
}
let result = Bytes::from_slice(&env, &buffer);
```

## Suggested Fix

Avoid performing operator-based `Bytes` concatenation inside loops. Instead, accumulate data in a native vector or buffer and create the host `Bytes` object once after the loop.

## Cost Impact

- **CPU instructions & Memory allocations:** Each per-iteration concatenation allocates a new host object and copies bytes, scaling quadratically with loop iterations and draining the transaction CPU/memory budget.
