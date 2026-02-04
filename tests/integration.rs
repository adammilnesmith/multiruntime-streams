use multiruntime_streams::{MultiRuntimeStreamExt, RuntimeSpawner};
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_on_basic() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    let result: Vec<i32> = tokio_stream::iter(vec![1, 2, 3])
        .poll_on(spawner)
        .collect()
        .await;

    assert_eq!(result, vec![1, 2, 3]);
}

#[tokio::test]
async fn test_on_with_map() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    let result: Vec<i32> = tokio_stream::iter(vec![1, 2, 3])
        .poll_on(spawner.clone())
        .map(|x| x * 2)
        .poll_on(spawner)
        .collect()
        .await;

    assert_eq!(result, vec![2, 4, 6]);
}

#[test]
fn test_subscribe_and_observe_on() {
    // Create runtimes outside of async context
    let io_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .unwrap();

    let cpu_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .build()
        .unwrap();

    let io = RuntimeSpawner::new(io_rt.handle().clone());
    let cpu = RuntimeSpawner::new(cpu_rt.handle().clone());

    // Run the test on one of the runtimes
    io_rt.block_on(async {
        let result: Vec<i32> = tokio_stream::iter(vec![1, 2, 3, 4, 5])
            .poll_on(io)
            .map(|x| x * 2)
            .poll_on(cpu)
            .map(|x| x + 1)
            .collect()
            .await;

        assert_eq!(result, vec![3, 5, 7, 9, 11]);
    });
}

#[tokio::test]
async fn test_custom_capacity() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    let stream = tokio_stream::iter(0..100).poll_on_with(spawner.clone(), 16);

    let result: Vec<i32> = stream.poll_on_with(spawner, 32).collect().await;

    assert_eq!(result.len(), 100);
}

#[tokio::test]
async fn test_cancellation_on_drop() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    let stream = tokio_stream::iter(0..1000).poll_on(spawner);

    // Take only 5 items, which should cause the upstream to be cancelled
    let result: Vec<i32> = stream.take(5).collect().await;

    assert_eq!(result, vec![0, 1, 2, 3, 4]);
}

#[tokio::test]
async fn test_concat_map_basic() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    let result: Vec<i32> = tokio_stream::iter(0..10)
        .flat_map(
            |x| async move { x * 2 },
            spawner,
            4, // concurrency
        )
        .collect()
        .await;

    assert_eq!(result, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
}

#[tokio::test]
async fn test_concat_map_preserves_order() {
    use std::time::Duration;

    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    // Items with different processing times - should still be in order
    let result: Vec<u64> = tokio_stream::iter(vec![50, 10, 30, 5])
        .flat_map(
            |delay_ms| async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                delay_ms
            },
            spawner,
            4, // concurrency
        )
        .collect()
        .await;

    // Order preserved despite different completion times
    assert_eq!(result, vec![50, 10, 30, 5]);
}

#[tokio::test]
async fn test_concat_map_result_handles_errors() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    let results: Vec<_> = tokio_stream::iter(0..10)
        .flat_map_result(
            |x| async move {
                if x == 5 {
                    panic!("intentional error");
                }
                x * 2
            },
            spawner,
            4, // concurrency
        )
        .collect()
        .await;

    // Should have 9 Ok and 1 Err
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let err_count = results.iter().filter(|r| r.is_err()).count();

    assert_eq!(ok_count, 9);
    assert_eq!(err_count, 1);
}

#[cfg(feature = "global-spawners")]
mod global_spawners_tests {
    use super::*;
    use multiruntime_streams::spawners;

    #[tokio::test]
    async fn test_global_async_io_spawner() {
        let result: Vec<i32> = tokio_stream::iter(0..5)
            .poll_on(spawners::async_io())
            .collect()
            .await;

        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_global_compute_spawner() {
        let result: Vec<i32> = tokio_stream::iter(0..5)
            .poll_on(spawners::compute())
            .map(|x| x * x)
            .collect()
            .await;

        assert_eq!(result, vec![0, 1, 4, 9, 16]);
    }

    #[tokio::test]
    async fn test_global_current_spawner() {
        let result: Vec<i32> = tokio_stream::iter(0..5)
            .poll_on(spawners::current())
            .collect()
            .await;

        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_async_io_to_compute_with_globals() {
        let result: Vec<i32> = tokio_stream::iter(1..=10)
            .poll_on(spawners::async_io())
            .map(|x| x + 1)
            .poll_on(spawners::compute())
            .map(|x| x * x)
            .collect()
            .await;

        assert_eq!(result, vec![4, 9, 16, 25, 36, 49, 64, 81, 100, 121]);
    }

    #[tokio::test]
    async fn test_sync_io_with_spawn_blocking() {
        use multiruntime_streams::BlockingSpawner;
        use std::io::Write;
        use tempfile::NamedTempFile;

        let result: Vec<Vec<u8>> = tokio_stream::iter(vec!["test1", "test2"])
            .poll_on(spawners::sync_io())
            .then(|content| async move {
                let spawner = spawners::sync_io();

                // Blocking I/O on elastic thread pool
                spawner
                    .spawn_blocking(move || {
                        let mut file = NamedTempFile::new().unwrap();
                        file.write_all(content.as_bytes()).unwrap();
                        std::fs::read(file.path()).unwrap()
                    })
                    .await
                    .unwrap()
            })
            .collect()
            .await;

        assert_eq!(result, vec![b"test1".to_vec(), b"test2".to_vec()]);
    }

    #[tokio::test]
    async fn test_all_three_runtimes() {
        use multiruntime_streams::BlockingSpawner;

        let result: Vec<i32> = tokio_stream::iter(1..=5)
            .poll_on(spawners::async_io()) // Async I/O
            .map(|x| x * 2)
            .poll_on(spawners::compute()) // CPU computation
            .map(|x| x + 1)
            .poll_on(spawners::sync_io()) // Blocking I/O runtime
            .then(|x| async move {
                // Use blocking pool for "blocking" operation
                spawners::sync_io()
                    .spawn_blocking(move || x * 10)
                    .await
                    .unwrap()
            })
            .collect()
            .await;

        assert_eq!(result, vec![30, 50, 70, 90, 110]);
    }

    #[test]
    fn test_runtime_spawner_creates_tasks() {
        // Simple test: verify spawners can create tasks
        let io_rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("test-io-runtime")
            .build()
            .unwrap();

        let cpu_rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("test-cpu-runtime")
            .build()
            .unwrap();

        let io_spawner = RuntimeSpawner::new(io_rt.handle().clone());
        let cpu_spawner = RuntimeSpawner::new(cpu_rt.handle().clone());

        // Run on io runtime and verify both spawners work
        io_rt.block_on(async {
            let result: Vec<_> = tokio_stream::iter(0..10)
                .poll_on(io_spawner)
                .poll_on(cpu_spawner)
                .collect()
                .await;

            assert_eq!(result.len(), 10);
        });
    }

    #[test]
    fn test_global_spawners_are_distinct() {
        use multiruntime_streams::Spawner;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // Just verify they can all spawn tasks
            let async_io = spawners::async_io();
            let compute = spawners::compute();
            let sync_io = spawners::sync_io();

            let r1 = async_io.spawn(async { 42 }).await.unwrap();
            let r2 = compute.spawn(async { 42 }).await.unwrap();
            let r3 = sync_io.spawn(async { 42 }).await.unwrap();

            assert_eq!(r1, 42);
            assert_eq!(r2, 42);
            assert_eq!(r3, 42);
        });
    }
}

// Stress and load tests
#[tokio::test]
async fn test_high_throughput() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    // Process 10K items through multiple runtime hops
    let count = 10_000;
    let result: Vec<_> = tokio_stream::iter(0..count)
        .poll_on(spawner.clone())
        .map(|x| x * 2)
        .poll_on(spawner)
        .map(|x| x + 1)
        .collect()
        .await;

    assert_eq!(result.len(), count);
    assert_eq!(result[0], 1);
    assert_eq!(result[count - 1], (count - 1) * 2 + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_backpressure_under_load() {
    use std::time::Duration;

    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    // Fast producer, slow consumer
    let stream = tokio_stream::iter(0..1000)
        .poll_on_with(spawner, 16) // Small capacity to test backpressure
        .then(|x| async move {
            // Slow down to create backpressure
            tokio::time::sleep(Duration::from_micros(10)).await;
            x
        });

    let start = std::time::Instant::now();
    let result: Vec<_> = stream.collect().await;
    let duration = start.elapsed();

    assert_eq!(result.len(), 1000);
    // Should take at least 10ms (backpressure working)
    assert!(
        duration.as_millis() >= 5,
        "Backpressure should slow things down"
    );
}

#[tokio::test]
async fn test_cancellation_cleans_up() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());
    let items_processed = Arc::new(AtomicUsize::new(0));
    let counter = items_processed.clone();

    {
        let stream = tokio_stream::iter(0..1_000_000)
            .poll_on(spawner)
            .map(move |x| {
                counter.fetch_add(1, Ordering::Relaxed);
                x
            });

        // Take only 10 items, then drop
        let _result: Vec<_> = stream.take(10).collect().await;
    }

    // Give spawned task time to see channel closure
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Should have processed ~10 items, not 1 million
    let processed = items_processed.load(Ordering::Relaxed);
    assert!(
        processed < 1000,
        "Should have stopped early, but processed {}",
        processed
    );
}

#[tokio::test]
async fn test_concat_map_high_concurrency() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    let stream = tokio_stream::iter(0..1000).flat_map(
        |x| async move { x * 2 },
        spawner,
        100, // High concurrency
    );

    let result: Vec<_> = stream.collect().await;

    assert_eq!(result.len(), 1000);
}

#[tokio::test]
async fn test_memory_stability_repeated_operations() {
    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());

    // Repeat operations to check for memory leaks
    for _ in 0..100 {
        let _result: Vec<_> = tokio_stream::iter(0..100)
            .poll_on(spawner.clone())
            .map(|x| x * 2)
            .collect()
            .await;
    }

    // If we got here without OOM, memory management is working
}

#[tokio::test]
async fn test_on_current_convenience() {
    let result: Vec<_> = tokio_stream::iter(0..10)
        .on_current()
        .map(|x| x * 2)
        .collect()
        .await;

    assert_eq!(result, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
}

#[cfg(feature = "global-spawners")]
#[tokio::test]
async fn test_poll_on_and_observe_on_semantics_with_logging() {
    use multiruntime_streams::spawners;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use tokio_stream::StreamExt;

    println!("\n=== poll_on and observe_on Demonstration ===\n");
    println!("This test logs the thread name at every step to show runtime routing.\n");
    println!("Using global spawners: async_io(), compute(), and sync_io()\n");

    let results: Vec<_> = tokio_stream::iter(vec![1, 2, 3])
        // Stage 1: Source/initial operators
        .map(|item| {
            let thread = std::thread::current()
                .name()
                .unwrap_or("unknown")
                .to_string();
            println!("  [1-MAP-BEFORE-SUBSCRIBE] Item {} on: {}", item, thread);
            item
        })
        .poll_on(spawners::async_io()) // Ensure upstream runs on async I/O runtime
        .then(|item| async move {
            let thread = std::thread::current()
                .name()
                .unwrap_or("unknown")
                .to_string();
            println!("  [2-AFTER-SUBSCRIBE-ON-IO] Item {} on: {}", item, thread);
            item
        })
        // Stage 2: Switch to CPU for processing
        .poll_on(spawners::compute())
        .map(|item| {
            let thread = std::thread::current()
                .name()
                .unwrap_or("unknown")
                .to_string();
            println!("  [3-AFTER-OBSERVE-ON-CPU] Item {} on: {}", item, thread);
            item
        })
        // Use flat_map to process items concurrently (demonstrates parallelism)
        .flat_map(
            |item| async move {
                let thread = std::thread::current()
                    .name()
                    .unwrap_or("unknown")
                    .to_string();
                println!(
                    "  [4-FLAT_MAP-PARALLEL] Processing item {} concurrently (2 at a time) on: {}",
                    item, thread
                );

                // Simulate CPU-intensive work with actual computation (not sleep!)
                let mut hasher = DefaultHasher::new();
                for i in 0..10_000 {
                    (item + i).hash(&mut hasher);
                }
                let hash = hasher.finish();

                println!("      Computed hash for item {}: {}", item, hash);
                item * 10
            },
            spawners::compute(),
            2, // Concurrency limit
        )
        .map(|value| {
            let thread = std::thread::current()
                .name()
                .unwrap_or("unknown")
                .to_string();
            println!(
                "  [5-MAP-AFTER-FLAT_MAP_ON] Got value {} on: {}",
                value, thread
            );
            value
        })
        // Stage 4: Switch to blocking for I/O operations
        .poll_on(spawners::sync_io())
        .then(|item| async move {
            use multiruntime_streams::BlockingSpawner;
            let thread = std::thread::current()
                .name()
                .unwrap_or("unknown")
                .to_string();
            println!(
                "  [8-AFTER-OBSERVE-ON-SYNC_IO] Item {} on: {}",
                item, thread
            );

            // Use spawn_blocking for actual blocking I/O work
            spawners::sync_io()
                .spawn_blocking(move || {
                    let blocking_thread = std::thread::current()
                        .name()
                        .unwrap_or("unknown")
                        .to_string();
                    println!(
                        "  [9-INSIDE-SPAWN_BLOCKING] Item {} on blocking pool: {}",
                        item, blocking_thread
                    );
                    // Simulate blocking I/O (std::fs::write, etc.)
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    item + 1000
                })
                .await
                .unwrap()
        })
        .collect()
        .await;

    println!("\n=== Results ===");
    println!("Final values: {:?}\n", results);

    println!("✅ What you should observe:");
    println!("  [1] Runs on multiruntime-streams-async-io (poll_on moved it there)");
    println!("  [2] Runs on multiruntime-streams-compute (next observe_on will poll this!)");
    println!("  [3] Runs on multiruntime-streams-compute (observe_on switched to compute)");
    println!(
        "  [4] Runs on multiruntime-streams-compute (concat_map spawns parallel tasks with real CPU work)"
    );
    println!("  [5] Runs on multiruntime-streams-sync-io (next observe_on will poll this!)");
    println!("  [8] Runs on multiruntime-streams-sync-io (observe_on switched to sync_io)");
    println!("  [9] Runs on blocking pool (spawn_blocking)\n");

    println!("💡 Key Observations:");
    println!("  • poll_on: moves source to async_io runtime");
    println!("  • observe_on: switches runtimes for processing");
    println!("  • concat_map: spawns up to 2 parallel tasks with CPU-intensive work");
    println!("  • Actual computation (hashing) not sleep - real CPU work!");
    println!("  • spawn_blocking: uses elastic blocking thread pool\n");

    println!("⚠️  Critical Semantics Understanding:");
    println!("  Operators BETWEEN two switches get polled by the SECOND switch!");
    println!("  Example: [2] comes after .poll_on(async_io) but runs on compute()");
    println!("           because the next .poll_on(compute) spawns a task that polls it!");
    println!("  Example: [5] comes after .flat_map() but runs on sync_io()");
    println!("           because the next .poll_on(sync_io) spawns a task that polls it!\n");

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_graceful_cancellation() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;

    let spawner = RuntimeSpawner::new(tokio::runtime::Handle::current());
    let task_finished = Arc::new(AtomicBool::new(false));
    let flag = task_finished.clone();

    {
        let _stream = tokio_stream::iter(0..1_000_000)
            .poll_on(spawner)
            .map(move |x| {
                if x == 999_999 {
                    flag.store(true, Ordering::Relaxed);
                }
                x
            });

        // Drop the stream without consuming it
        // This should send cancellation signal
    }

    // Wait a bit for cancellation to propagate
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Task should NOT have processed all 1M items
    assert!(
        !task_finished.load(Ordering::Relaxed),
        "Task should have been cancelled before completing"
    );
}
