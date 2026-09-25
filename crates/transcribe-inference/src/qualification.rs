//! Resource observations for explicit installed-model qualification only.

pub(crate) fn rss_kib() -> u64 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("process RSS observation requires native process access");
    assert!(output.status.success(), "process RSS observation failed");
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .expect("process RSS in KiB")
}

pub(crate) fn assert_bounded_growth(checkpoints: &[u64]) {
    assert_eq!(checkpoints.len(), 10, "one RSS checkpoint per ten requests");
    let first = checkpoints[0];
    let peak = checkpoints.iter().copied().max().unwrap();
    println!(
        "resident_rss={}",
        serde_json::json!({"checkpointEveryRequests":10,"rssKiB":checkpoints,
            "growthKiB":peak.saturating_sub(first),"limitKiB":128 * 1024})
    );
    assert!(
        peak.saturating_sub(first) <= 128 * 1024,
        "post-warmup process RSS growth exceeded the predeclared 128 MiB ceiling"
    );
}
