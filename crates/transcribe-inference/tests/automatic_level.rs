//! Model-backed input-rate contracts for the shared automatic-level path.
use std::{path::PathBuf, sync::atomic::AtomicBool};
use transcribe_audio::MonoPcm;
use transcribe_core::{AppConfig, NoiseReduction};
use transcribe_inference::audio_frontend::{process_completed, FrontendCache};

#[test]
#[ignore = "requires pinned frontend models"]
fn automatic_level_preserves_device_rate_samples_across_packet_schedules() {
    let root = PathBuf::from(std::env::var_os("TRANSCRIBE_SMOKE_MODELS").expect("models"));
    let cancelled = AtomicBool::new(false);
    let mut cache = FrontendCache::default();
    let mut config = AppConfig::default();
    config.audio_frontend.auto_level_enabled = true;
    config.audio_frontend.vad_enabled = true;
    for denoiser in [
        NoiseReduction::Off,
        NoiseReduction::Dpdfnet4,
        NoiseReduction::Dpdfnet8,
    ] {
        config.audio_frontend.noise_reduction = denoiser;
        for rate in [16000, 44100, 48000] {
            let length = 2 * rate as usize + 17;
            let input = MonoPcm::new(
                (0..length)
                    .map(|i| {
                        let scale = if i < length / 2 { 0.0001 } else { 0.7 };
                        scale * (i as f32 * 300.0 * std::f32::consts::TAU / rate as f32).sin()
                    })
                    .collect(),
                rate,
            )
            .unwrap();
            let (expected, stats) =
                process_completed(&mut cache, &input, &config, &root, &cancelled).unwrap();
            let mut session = cache.start(&config, &root, &cancelled).unwrap();
            let mut actual = Vec::new();
            let mut offset = 0;
            for size in [1, 441, 17, 960, 37, 2048].into_iter().cycle() {
                if offset == length {
                    break;
                }
                let end = (offset + size).min(length);
                actual.extend(
                    session
                        .push(&input.samples()[offset..end], rate, &cancelled)
                        .unwrap(),
                );
                offset = end;
            }
            actual.extend(session.finish(&cancelled).unwrap());
            let wanted = (length as f64 * 16000.0 / rate as f64).round() as usize;
            assert_eq!(actual.len(), wanted);
            assert_eq!(
                actual,
                expected.samples(),
                "device-rate packetization changed PCM"
            );
            assert_eq!(session.metrics().speech_intervals, stats.speech_intervals);
            assert_eq!(
                session.metrics().automatic_level.as_ref().unwrap().samples,
                wanted
            );
            assert!(
                session
                    .metrics()
                    .automatic_level
                    .as_ref()
                    .unwrap()
                    .output_peak
                    <= 0.950001
            );
        }
    }
}
