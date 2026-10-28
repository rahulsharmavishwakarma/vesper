//! Per-app workspace detection coverage recorded during real dictations (only
//! while **Detect workspaces automatically** is on). Reads app identifiers and
//! counts; the table holds no paths, hosts, titles or text.
//! Usage: `workspace_detection_report DATASET_DIR [--clear]` → JSON on stdout.
use serde_json::json;
use vesper_data::DatasetStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .ok_or("usage: workspace_detection_report DATASET_DIR [--clear]")?;
    let store = DatasetStore::open(root.into())?;
    if args.next().as_deref() == Some("--clear") {
        store.clear_detection_coverage()?;
    }
    let percent = |part: u32, whole: u32| {
        if whole == 0 {
            0.0
        } else {
            (1000.0 * f64::from(part) / f64::from(whole)).round() / 10.0
        }
    };
    let apps = store
        .detection_coverage()?
        .into_iter()
        .map(|c| {
            let found = c.file + c.folder + c.web;
            json!({
                "application": c.application,
                "dictations": c.dictations,
                "detectedPercent": percent(found, c.dictations),
                "file": c.file, "folder": c.folder, "web": c.web, "none": c.none,
                "readMs": {"under1": c.latency[0], "under5": c.latency[1],
                           "under30": c.latency[2], "slower": c.latency[3]},
            })
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "apps": apps }))?
    );
    Ok(())
}
