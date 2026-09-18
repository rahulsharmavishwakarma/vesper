//! Compare the existing hash primitive with its supported CPU backend.
//! Files are verified completely on every call; there is no verification cache.
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: audio-integrity-probe FILE EXPECTED_BYTES EXPECTED_SHA256".into());
    }
    let expected_bytes: u64 = args[1].parse()?;
    let mut reader = File::open(&args[0])?;
    let cancelled = AtomicBool::new(false);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    let mut count = 0u64;
    let started = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let size = reader.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        count += size as u64;
        if count > expected_bytes {
            return Err("file exceeds expected size".into());
        }
        hasher.update(&buffer[..size]);
    }
    let actual = format!("{:x}", hasher.finalize());
    let milliseconds = started.elapsed().as_secs_f64() * 1000.0;
    if count != expected_bytes || actual != args[2] {
        return Err("integrity failure".into());
    }
    #[cfg(target_arch = "aarch64")]
    let cpu_sha2 = std::arch::is_aarch64_feature_detected!("sha2");
    #[cfg(not(target_arch = "aarch64"))]
    let cpu_sha2 = false;
    println!("{{\"bytes\":{count},\"sha256\":\"{actual}\",\"milliseconds\":{milliseconds},\"hardwareFeatureEnabled\":{},\"aarch64Sha2Detected\":{cpu_sha2},\"integrityVerified\":true}}", cfg!(feature = "hardware"));
    Ok(())
}
