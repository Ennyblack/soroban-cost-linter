# Lint Reference

This section provides detailed documentation for all lints supported by `soroban-cost-linter`.

## Storage Operations

| Lint                                                                  | Default Severity | Catches                                    |
| --------------------------------------------------------------------- | ---------------- | ------------------------------------------ |
| [`soroban_storage_in_loop`](soroban_storage_in_loop.md)               | `warn`           | Storage reads/writes inside loop bodies    |

## CPU/Compute

| Lint                                                                  | Default Severity | Catches                                    |
| --------------------------------------------------------------------- | ---------------- | ------------------------------------------ |
| [`unnecessary_host_function_call`](unnecessary_host_function_call.md) | `warn`           | Redundant host function calls inside loops |
| [`host_in_loop`](host_in_loop.md)                                     | `warn`           | Host object usage inside loop bodies       |

## Memory

| Lint                                                                  | Default Severity | Catches                                    |
| --------------------------------------------------------------------- | ---------------- | ------------------------------------------ |
| [`redundant_env_clone`](redundant_env_clone.md)                       | `warn`           | Unnecessary `.clone()` calls on `Env`      |

## Lint inventory schema

The CLI can emit a versioned inventory of all registered lints via `cargo cost-lint --list-lints --format json`. The payload contains:

- `version`: inventory schema version (`1.0`)
- `schema`: the schema documentation URL
- `lints`: an array of entries containing `name`, `default_level`, `description`, `category`, and `documentation_url`

{% hint style="info" %}
Severities can be adjusted per-workspace via `budget.toml` — see the [Integration Guide](../integration.md).
{% endhint %}
