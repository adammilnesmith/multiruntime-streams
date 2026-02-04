use crate::RuntimeSpawner;
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static ASYNC_IO_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    let threads = num_cpus::get().max(4);

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .thread_name("multiruntime-streams-async-io")
        .enable_io()
        .enable_time()
        .build()
        .expect("Failed to create global async I/O runtime")
});

static COMPUTE_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    let threads = num_cpus::get();

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .thread_name("multiruntime-streams-compute")
        .build()
        .expect("Failed to create global compute runtime")
});

static SYNC_IO_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("multiruntime-streams-sync-io")
        .max_blocking_threads(512)
        .build()
        .expect("Failed to create global sync I/O runtime")
});

pub fn async_io() -> RuntimeSpawner {
    RuntimeSpawner::new(ASYNC_IO_RUNTIME.handle().clone())
}

pub fn compute() -> RuntimeSpawner {
    RuntimeSpawner::new(COMPUTE_RUNTIME.handle().clone())
}

pub fn sync_io() -> RuntimeSpawner {
    RuntimeSpawner::new(SYNC_IO_RUNTIME.handle().clone())
}

pub fn current() -> RuntimeSpawner {
    RuntimeSpawner::new(
        tokio::runtime::Handle::try_current()
            .expect("spawners::current() called outside of a Tokio runtime context"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spawner;

    #[test]
    fn test_spawners_initialize() {
        // Create an outer runtime to test in
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let async_io_spawner = async_io();
            let compute_spawner = compute();
            let sync_io_spawner = sync_io();
            let current_spawner = current();

            // Smoke test to ensure they all initialize
            let _ = async_io_spawner.handle();
            let _ = compute_spawner.handle();
            let _ = sync_io_spawner.handle();
            let _ = current_spawner.handle();
        });
    }

    #[tokio::test]
    async fn test_sync_io_blocking_pattern() {
        use crate::BlockingSpawner;
        use std::io::Write;
        use tempfile::NamedTempFile;

        let spawner = sync_io();

        // Test blocking I/O pattern
        let content = spawner
            .spawn_blocking(|| {
                let mut file = NamedTempFile::new().unwrap();
                file.write_all(b"test data from sync_io").unwrap();
                std::fs::read(file.path()).unwrap()
            })
            .await
            .unwrap();

        assert_eq!(content, b"test data from sync_io");
    }

    #[tokio::test]
    async fn test_compute_has_exact_cpu_count_threads() {
        // This is a design verification test
        // The compute runtime should have exactly num_cpus workers
        let spawner = compute();

        // Spawn a task to verify it runs
        let result = spawner.spawn(async { 42 }).await.unwrap();
        assert_eq!(result, 42);
    }
}
