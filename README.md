# multiruntime-streams

⚠️ **Experimental** - This library is in early development and the API may change.

A Rust library for processing streams across multiple Tokio runtimes, enabling fine-grained control over where your async work executes.

## Overview

`multiruntime-streams` extends Rust's async streams with operators that let you route stream processing across different Tokio runtimes. Think of it as RxJava's `subscribeOn`/`observeOn` for Rust streams, but with the power to separate I/O, CPU-bound work, and blocking operations onto dedicated thread pools.

## Why?

In complex async applications, different types of work have different resource requirements:

- **Async I/O** (network, async file operations) - needs many lightweight workers
- **CPU-bound computation** (parsing, serialization, compression) - benefits from exactly `num_cpus` threads
- **Blocking I/O** (`std::fs`, legacy sync libraries) - requires elastic blocking thread pools

Running all work on a single runtime leads to thread pool contention and poor performance. This library lets you route work to specialized runtimes optimally.

## Key Concepts

### 1. **Spawners**

A `Spawner` is an abstraction over a Tokio runtime handle that can spawn tasks:

```rust
pub trait Spawner: Clone + Send + Sync + 'static {
    fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;
}
```

Create spawners from any Tokio runtime:
```rust
let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());
```

### 2. **Stream Operators**

#### `poll_on(spawner)`

Routes stream polling to a different runtime. Similar to RxJava's `subscribeOn` - it moves the upstream source and operators to the specified runtime.

```rust
use multiruntime_streams::MultiRuntimeStreamExt;

tokio_stream::iter(0..100)
    .poll_on(io_spawner)        // Source runs on I/O runtime
    .map(|x| fetch_data(x))     // This runs on I/O runtime too
    .poll_on(compute_spawner)   // Switch to compute runtime
    .map(|data| process(data))  // CPU work on compute runtime
    .collect()
    .await
```

#### `flat_map(mapper, spawner, concurrency)`

Processes stream items concurrently with bounded parallelism. Each item is mapped through an async function that runs on the specified runtime.

```rust
tokio_stream::iter(urls)
    .flat_map(
        |url| async move { fetch(url).await },
        io_spawner,
        10  // Process 10 URLs concurrently
    )
    .collect()
    .await
```

Order is preserved - results come out in the same order as inputs, even though processing happens concurrently.

#### `flat_map_result(mapper, spawner, concurrency)`

Like `flat_map` but returns `Result<T, JoinError>` instead of panicking on task failure. Use when you need to handle errors gracefully.

### 3. **Global Spawners** (feature: `global-spawners`)

Pre-configured, lazily-initialized runtimes for common workload patterns:

```rust
use multiruntime_streams::spawners;

// Async I/O runtime (many workers, I/O + time enabled)
let async_io = spawners::async_io();

// CPU-bound compute runtime (exactly num_cpus workers)
let compute = spawners::compute();

// Blocking I/O runtime (small worker pool + large elastic blocking pool)
let sync_io = spawners::sync_io();

// Current runtime (whatever context you're in)
let current = spawners::current();
```

These runtimes are shared across your application and optimized for their specific workloads.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
multiruntime-streams = "0.0.1"
```

### Features

- `tokio` (default) - Tokio runtime support
- `global-spawners` (default) - Pre-configured global runtimes
- `local` - Local runtime support
- `tracing` - Tracing integration

## Usage Examples

### Basic Multi-Runtime Pipeline

```rust
use multiruntime_streams::{MultiRuntimeStreamExt, spawners};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    let results: Vec<_> = tokio_stream::iter(1..=10)
        .poll_on(spawners::async_io())      // Fetch on I/O runtime
        .map(|id| fetch_user(id))
        .poll_on(spawners::compute())       // Process on CPU runtime
        .map(|user| expensive_transform(user))
        .collect()
        .await;
}
```

### Concurrent Processing with `flat_map`

```rust
use multiruntime_streams::{MultiRuntimeStreamExt, spawners};
use tokio_stream::StreamExt;

// Process 100 URLs with concurrency of 20
let responses: Vec<_> = tokio_stream::iter(urls)
    .flat_map(
        |url| async move {
            reqwest::get(&url).await?.text().await
        },
        spawners::async_io(),
        20  // 20 concurrent requests
    )
    .collect()
    .await;
```


## How It Works

### `poll_on` Mechanics

When you call `.poll_on(spawner)`:

1. A background task is spawned on the target runtime
2. Stream items are sent through an `mpsc` channel
3. The background task polls the upstream and sends items forward
4. Backpressure is maintained through bounded channels
5. Cancellation is handled via a oneshot signal on drop

This means operators **between** two `poll_on` calls run on the **downstream** runtime:

```rust
stream
    .map(|x| x + 1)           // Runs on current runtime
    .poll_on(runtime_a)       // Spawns task on runtime_a
    .map(|x| x * 2)           // Runs on runtime_b! (polled by next poll_on)
    .poll_on(runtime_b)       // Spawns task on runtime_b
    .map(|x| x - 1)           // Runs on runtime_b
```

### `flat_map` Mechanics

`flat_map` uses `futures_util::StreamExt::buffered` under the hood:

1. Maps each stream item through your async function
2. Each function call is spawned as a task on the target runtime
3. Up to `concurrency` tasks run in parallel
4. Results are buffered and yielded in order
5. Task failures panic by default (use `flat_map_result` to handle errors)

## Performance Guidelines

### Runtime Selection

- **`async_io()`** - Network requests, async file I/O, timers
- **`compute()`** - JSON parsing, compression, encryption, pure computation
- **`sync_io()`** - `std::fs` operations, blocking libraries, FFI

### Concurrency Tuning

- **I/O-bound work**: High concurrency (50-200+) is fine
- **CPU-bound work**: Set concurrency to `num_cpus` or slightly higher
- **Blocking I/O**: Match your expected I/O parallelism (files to read concurrently)

### Channel Capacity

Default capacity is 256. Adjust with `poll_on_with(spawner, capacity)`:
- **Small capacity** (16-64): Better latency, more backpressure
- **Large capacity** (256-1024): Better throughput, more buffering

## Limitations

1. **Order preservation**: `flat_map` maintains order, which may limit throughput for highly variable task durations
2. **Tokio only**: Currently only supports Tokio runtimes

## Comparison to Other Approaches

| Approach | Pros | Cons |
|----------|------|------|
| Single runtime | Simple | Poor isolation, thread contention |
| Manual task spawning | Full control | Verbose, error-prone |
| `tokio::select!` | Fine-grained | Doesn't solve runtime separation |
| **multiruntime-streams** | Declarative, composable | Requires explicit runtime management |
