/// Production-grade unit tests for queue_orchestrator module
///
/// Tests cover:
/// - Bandwidth allocation strategies
/// - Speed trend detection
/// - ETC prediction accuracy
/// - Bottleneck detection
/// - Queue efficiency scoring
/// - Real-world scenarios

#[cfg(test)]
mod queue_orchestrator_tests {
    use crate::queue_orchestrator::{QueueOrchestrator, DownloadMetrics, QueueOrchestrationState};

    #[test]
    fn test_orchestrator_creation() {
        let orch = QueueOrchestrator::new();
        assert_eq!(orch.format_bytes(1024), "1.00 KB");
        assert_eq!(orch.format_bytes(1048576), "1.00 MB");
        assert_eq!(orch.format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_register_and_unregister_downloads() {
        let orch = QueueOrchestrator::new();

        // Register multiple downloads
        assert!(orch
            .register_download("dl-1", "https://example.com/file1.bin", 1024 * 1024 * 100, 2)
            .is_ok());
        assert!(orch
            .register_download("dl-2", "https://example.com/file2.bin", 1024 * 1024 * 50, 1)
            .is_ok());
        assert!(orch
            .register_download("dl-3", "https://example.com/file3.bin", 1024 * 1024 * 25, 0)
            .is_ok());

        // Verify registration
        let metrics = orch.get_metrics(None).unwrap();
        assert_eq!(metrics.len(), 3);

        // Duplicate should fail
        assert!(orch
            .register_download("dl-1", "https://example.com/file-other.bin", 1024, 1)
            .is_err());

        // Unregister and verify removal
        assert!(orch.unregister_download("dl-2").is_ok());
        let metrics = orch.get_metrics(None).unwrap();
        assert_eq!(metrics.len(), 2);
    }

    #[test]
    fn test_progress_recording_and_metrics() {
        let orch = QueueOrchestrator::new();
        let total_bytes = 1024 * 1024 * 100; // 100 MB

        orch.register_download("dl-1", "https://example.com/file.bin", total_bytes, 1)
            .unwrap();

        // Simulate first second: 1 MB downloaded
        orch.record_progress("dl-1", 1024 * 1024, 1000).unwrap();

        let metrics = orch.get_metrics(Some("dl-1")).unwrap();
        assert_eq!(metrics.len(), 1);

        let metric = &metrics[0];
        assert_eq!(metric.bytes_downloaded, 1024 * 1024);
        assert_eq!(metric.total_bytes, total_bytes);
        assert!(metric.current_speed_bps > 0); // Should have measurable speed

        // Simulate continuing: 2 more MB in 2 more seconds
        orch.record_progress("dl-1", 1024 * 1024 * 2, 3000).unwrap();

        let metrics = orch.get_metrics(Some("dl-1")).unwrap();
        let metric = &metrics[0];
        assert_eq!(metric.bytes_downloaded, 1024 * 1024 * 3);
        assert!(metric.average_speed_bps > 0);
        assert!(metric.estimated_remaining_ms > 0);
    }

    #[test]
    fn test_priority_bandwidth_allocation() {
        let orch = QueueOrchestrator::new();

        // Register downloads at different priorities
        orch.register_download("high-1", "https://ex.com/1", 1024 * 1024 * 100, 2)
            .unwrap();
        orch.register_download("normal-1", "https://ex.com/2", 1024 * 1024 * 80, 1)
            .unwrap();
        orch.register_download("normal-2", "https://ex.com/3", 1024 * 1024 * 60, 1)
            .unwrap();
        orch.register_download("low-1", "https://ex.com/4", 1024 * 1024 * 40, 0)
            .unwrap();

        let total_bandwidth = 10 * 1024 * 1024; // 10 MB/s
        let allocation = orch.allocate_bandwidth(total_bandwidth).unwrap();

        // Verify allocation exists for all downloads
        assert_eq!(allocation.keys().len(), 4);

        // Verify high-priority gets most bandwidth
        let high_allocated = allocation.get("high-1").unwrap();
        let normal_allocated = allocation.get("normal-1").unwrap();
        let low_allocated = allocation.get("low-1").unwrap();

        // High should get roughly 50%, normal 35%, low 15%
        assert!(high_allocated > normal_allocated);
        assert!(normal_allocated > low_allocated);

        // Verify total doesn't exceed available
        let total_allocated: u64 = allocation.values().sum();
        assert!(total_allocated <= total_bandwidth);
    }

    #[test]
    fn test_blocked_downloads_handling() {
        let orch = QueueOrchestrator::new();

        orch.register_download("dep-1", "https://ex.com/1", 1024 * 1024, 1)
            .unwrap();
        orch.register_download("dep-2", "https://ex.com/2", 1024 * 1024, 1)
            .unwrap();

        // Mark second as blocked (waiting for dependency)
        orch.set_blocked("dep-2", true).unwrap();

        let metrics = orch.get_metrics(None).unwrap();
        let blocked = metrics.iter().find(|m| m.id == "dep-2").unwrap();
        assert!(blocked.is_blocked);

        // Unblock it
        orch.set_blocked("dep-2", false).unwrap();
        let metrics = orch.get_metrics(None).unwrap();
        let unblocked = metrics.iter().find(|m| m.id == "dep-2").unwrap();
        assert!(!unblocked.is_blocked);
    }

    #[test]
    fn test_etc_calculation() {
        let orch = QueueOrchestrator::new();
        let total_bytes = 1024 * 1024 * 100; // 100 MB

        orch.register_download("dl-1", "https://ex.com/file.bin", total_bytes, 1)
            .unwrap();

        // Simulate 50 MB downloaded in 50 seconds (1 MB/s)
        for i in 0..50 {
            orch.record_progress("dl-1", 1024 * 1024, 1000).unwrap();
        }

        let metrics = orch.get_metrics(Some("dl-1")).unwrap();
        let metric = &metrics[0];

        // Should be roughly 50 seconds remaining (50 MB left at 1 MB/s)
        assert!(metric.estimated_remaining_ms > 40_000); // at least 40 seconds
        assert!(metric.estimated_remaining_ms < 60_000); // at most 60 seconds
    }

    #[test]
    fn test_speed_trend_detection() {
        let orch = QueueOrchestrator::new();

        orch.register_download("dl-1", "https://ex.com/file", 1024 * 1024 * 100, 1)
            .unwrap();

        // Simulate improving speed trend
        orch.record_progress("dl-1", 100 * 1024, 1000).unwrap(); // 100 KB/s
        orch.record_progress("dl-1", 150 * 1024, 1000).unwrap(); // 150 KB/s
        orch.record_progress("dl-1", 200 * 1024, 1000).unwrap(); // 200 KB/s

        let trend = orch.get_speed_trend("dl-1").unwrap();
        assert!(trend.contains("Improving") || trend.contains("Insufficient"));

        // Test degrading trend
        orch.register_download("dl-2", "https://ex.com/file2", 1024 * 1024 * 100, 1)
            .unwrap();

        orch.record_progress("dl-2", 200 * 1024, 1000).unwrap(); // 200 KB/s
        orch.record_progress("dl-2", 150 * 1024, 1000).unwrap(); // 150 KB/s
        orch.record_progress("dl-2", 100 * 1024, 1000).unwrap(); // 100 KB/s

        let trend = orch.get_speed_trend("dl-2").unwrap();
        assert!(trend.contains("Degrading") || trend.contains("Insufficient"));
    }

    #[test]
    fn test_queue_efficiency_calculation() {
        let orch = QueueOrchestrator::new();

        // Register 3 downloads
        for i in 1..=3 {
            orch.register_download(
                &format!("dl-{}", i),
                &format!("https://ex.com/file{}.bin", i),
                1024 * 1024 * 100,
                1,
            )
            .unwrap();

            // Simulate some progress
            orch.record_progress(&format!("dl-{}", i), 1024 * 512, 1000)
                .unwrap();
        }

        // Analyze queue
        let analysis = orch.analyze_queue(5, 3, 5).unwrap();
        let state = &analysis.state;

        // Verify state fields are populated
        assert_eq!(state.total_active_downloads, 3);
        assert_eq!(state.total_queued_downloads, 5);
        assert!(state.queue_efficiency >= 0.0);
        assert!(state.queue_efficiency <= 1.0);
    }

    #[test]
    fn test_format_bytes_human_readable() {
        assert_eq!(QueueOrchestrator::format_bytes(0), "0.00 B");
        assert_eq!(QueueOrchestrator::format_bytes(1), "1.00 B");
        assert_eq!(QueueOrchestrator::format_bytes(1023), "1023.00 B");
        assert_eq!(
            QueueOrchestrator::format_bytes(1024),
            "1.00 KB"
        );
        assert_eq!(
            QueueOrchestrator::format_bytes(1024 * 1024),
            "1.00 MB"
        );
        assert_eq!(
            QueueOrchestrator::format_bytes(1024 * 1024 * 1024),
            "1.00 GB"
        );
        assert_eq!(
            QueueOrchestrator::format_bytes(1024 * 1024 * 1024 * 1024),
            "1.00 GB" // Caps at GB
        );
    }

    #[test]
    fn test_concurrent_operations() {
        use std::sync::Arc;
        use std::thread;

        let orch = Arc::new(QueueOrchestrator::new());

        // Register from multiple threads
        let mut handles = vec![];

        for i in 0..10 {
            let orch_clone = Arc::clone(&orch);
            let handle = thread::spawn(move || {
                let id = format!("concurrent-{}", i);
                let url = format!("https://ex.com/file{}.bin", i);
                assert!(orch_clone
                    .register_download(&id, &url, 1024 * 1024 * 100, (i % 3) as u8)
                    .is_ok());

                // Record some progress
                for _ in 0..5 {
                    let _ = orch_clone.record_progress(&id, 1024 * 512, 1000);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all downloads are registered
        let metrics = orch.get_metrics(None).unwrap();
        assert_eq!(metrics.len(), 10);
    }

    #[test]
    fn test_realistic_scenario() {
        let orch = QueueOrchestrator::new();

        // Set global bandwidth limit
        orch.set_global_bandwidth_limit(50 * 1024 * 1024); // 50 MB/s

        // Register downloads in different priority tiers
        let scenarios = vec![
            ("critical-backup", "https://backup.server.com/data.tar.gz", 1024 * 1024 * 5000, 2), // 5 GB, high priority
            ("normal-software", "https://software.repo.com/app.exe", 1024 * 1024 * 500, 1),      // 500 MB, normal
            ("low-docs", "https://docs.repo.com/manual.pdf", 1024 * 1024 * 100, 0),              // 100 MB, low
        ];

        for (id, url, size, priority) in scenarios {
            orch.register_download(id, url, size, priority).unwrap();
        }

        // Simulate 10 seconds of downloading
        for second in 1..=10 {
            // Critical gets more bandwidth
            orch.record_progress("critical-backup", 15 * 1024 * 1024, 1000).unwrap(); // 15 MB/s
            // Normal gets less
            orch.record_progress("normal-software", 8 * 1024 * 1024, 1000).unwrap(); // 8 MB/s
            // Low gets least
            orch.record_progress("low-docs", 2 * 1024 * 1024, 1000).unwrap(); // 2 MB/s
        }

        // Get allocation
        let allocation = orch.allocate_bandwidth(50 * 1024 * 1024).unwrap();
        assert_eq!(allocation.len(), 3);

        // Verify high-priority gets more
        let critical = allocation.get("critical-backup").unwrap();
        let normal = allocation.get("normal-software").unwrap();
        let low = allocation.get("low-docs").unwrap();

        assert!(critical > normal);
        assert!(normal > low);

        // Analyze queue
        let analysis = orch.analyze_queue(0, 3, 5).unwrap();
        assert_eq!(analysis.state.total_active_downloads, 3);

        // Critical should ETC soon, others much later
        let critical_metrics = orch.get_metrics(Some("critical-backup")).unwrap();
        let low_metrics = orch.get_metrics(Some("low-docs")).unwrap();

        // Critical should have much less time remaining due to higher priority/speed
        assert!(critical_metrics[0].estimated_remaining_ms < low_metrics[0].estimated_remaining_ms);
    }
}
