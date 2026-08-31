use mcl_cli::bench;

#[tokio::test]
async fn bench_mock_mode_runs_all_scenarios() {
    let report = bench::run_mock_benchmark(None, 1)
        .await
        .expect("mock benchmark should succeed");
    assert_eq!(report.total_turns, 20);
    assert_eq!(report.correct_signals, 20);
    assert_eq!(report.malformed_signals, 0);
    assert_eq!(report.missing_signals, 0);
    assert_eq!(report.hallucinated_signals, 0);
}

#[tokio::test]
async fn bench_mock_mode_produces_report() {
    let report = bench::run_mock_benchmark(Some(10), 3)
        .await
        .expect("mock benchmark should succeed");
    assert_eq!(report.total_turns, 30);
    assert_eq!(report.correct_signals, 30);
    assert_eq!(report.model, "mock");
    assert!((report.accuracy() - 100.0).abs() < f64::EPSILON);
}
