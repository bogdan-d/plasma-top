# async-fn-in-trait

> Use native `async fn` in traits (stable 1.75) instead of the `async_trait` macro

## Why It Matters

Since Rust 1.75, you can write `async fn` directly inside trait definitions (AFIT — async functions in traits). This eliminates the `#[async_trait]` proc-macro dependency and removes the hidden `Box<dyn Future>` allocation it inserts on every call. Fewer allocations, no macro expansion overhead, and no extra crate to audit. However, native async fn in traits carries two precise caveats you must understand before migrating.

## Bad

```rust
// requires async_trait crate; boxes every future on the heap
use async_trait::async_trait;

#[async_trait]
trait Repo {
    async fn get(&self, id: u64) -> anyhow::Result<String>;
    async fn save(&self, value: String) -> anyhow::Result<()>;
}

struct PgRepo;

#[async_trait]
impl Repo for PgRepo {
    async fn get(&self, id: u64) -> anyhow::Result<String> {
        Ok(format!("row-{id}"))
    }

    async fn save(&self, value: String) -> anyhow::Result<()> {
        let _ = value;
        Ok(())
    }
}
```

## Good

```rust
// native async fn in traits — no macro, no boxing
trait Repo {
    async fn get(&self, id: u64) -> anyhow::Result<String>;
    async fn save(&self, value: String) -> anyhow::Result<()>;
}

struct PgRepo;

impl Repo for PgRepo {
    async fn get(&self, id: u64) -> anyhow::Result<String> {
        Ok(format!("row-{id}"))
    }

    async fn save(&self, value: String) -> anyhow::Result<()> {
        let _ = value;
        Ok(())
    }
}
```

## Caveats

**Caveat 1 — not dyn-compatible.** Native async fn in traits is not yet dyn-compatible. You cannot write `Box<dyn Repo>` with the definition above. For dynamic dispatch, either keep `#[async_trait]` (it boxes the future), or write a dyn-compatible method that returns a boxed future explicitly:

```rust
use std::{future::Future, pin::Pin};

trait DynRepo: Send + Sync {
    fn get(&self, id: u64) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + '_>>;
}

impl DynRepo for PgRepo {
    fn get(&self, id: u64) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + '_>> {
        Box::pin(async move { Ok(format!("row-{id}")) })
    }
}

fn make_repo() -> Box<dyn DynRepo> {
    Box::new(PgRepo)
}
```

`trait-variant` remains useful for generating a `Send`-bounded trait for static dispatch; it still returns an opaque `impl Future` and cannot be used as `Box<dyn Trait>`:

```rust
#[trait_variant::make(RepoSend: Send)]
trait Repo {
    async fn get(&self, id: u64) -> anyhow::Result<String>;
}

// Use `RepoSend` through generics or `impl RepoSend`, not `dyn RepoSend`.
```

**Caveat 2 — futures are not `Send` by default.** On a multi-threaded Tokio runtime, spawned tasks require `Send` futures. The auto-generated future from a native `async fn` in a trait captures `&self` but does not promise `Send`. If you need `Send`, either:

- Use `#[trait_variant::make(TraitNameSend: Send)]` from the `trait-variant` crate to generate a `Send`-bounded variant.
- Bound the return type explicitly: `fn get(&self, id: u64) -> impl Future<Output = anyhow::Result<String>> + Send`.

```rust
// explicit Send bound on the return future
trait Repo {
    fn get(&self, id: u64) -> impl Future<Output = anyhow::Result<String>> + Send;
}
```

## When to Use Each Approach

| Scenario | Recommended approach |
|---|---|
| Static dispatch only (generics, `impl Trait`) | Native `async fn` in trait |
| Need `dyn Trait` | `#[async_trait]` or a trait method returning `Pin<Box<dyn Future<...>>>` |
| Multi-threaded Tokio, spawned tasks | `trait-variant` `Send` variant or explicit `+ Send` |
| Single-threaded runtime / `LocalSet` | Native `async fn` in trait (no `Send` needed) |

## See Also

- [anti-type-erasure](anti-type-erasure.md) - prefer `impl Trait` over `Box<dyn Trait>` when possible
- [async-async-fn-bounds](async-async-fn-bounds.md) - use `AsyncFn` bounds for higher-order async functions
- [async-tokio-runtime](async-tokio-runtime.md) - use Tokio for production async runtime
