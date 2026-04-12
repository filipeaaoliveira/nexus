use nexus::runtime_support::run_blocking_compatible;

#[test]
fn run_blocking_compatible_runs_without_tokio_runtime() {
    let value = run_blocking_compatible(|| 42);

    assert_eq!(value, 42);
}

#[test]
fn run_blocking_compatible_does_not_panic_on_current_thread_runtime() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime");

    let value = runtime.block_on(async { run_blocking_compatible(|| 7) });

    assert_eq!(value, 7);
}

#[test]
fn run_blocking_compatible_runs_on_multi_thread_runtime() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("multi-thread runtime");

    let value = runtime.block_on(async { run_blocking_compatible(|| 11) });

    assert_eq!(value, 11);
}
