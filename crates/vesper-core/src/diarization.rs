use serde::{Deserialize, Serialize};

/// Anonymous within-recording activity, never a user's identity or word alignment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeakerSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeakerTimeline {
    pub duration_ms: u64,
    pub segments: Vec<SpeakerSegment>,
    pub clipped_segments: usize,
    pub model_revision: String,
    pub runtime_revision: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
}

impl SpeakerTimeline {
    pub fn validate(&self) -> Result<(), String> {
        if self.duration_ms == 0 || self.segments.len() > 100_000 {
            return Err("Invalid speaker timeline size".into());
        }
        let mut previous = 0;
        for segment in &self.segments {
            if !(1..=8).contains(&segment.speaker)
                || segment.start_ms >= segment.end_ms
                || segment.end_ms > self.duration_ms
                || segment.start_ms < previous
            {
                return Err("Invalid speaker segment or ordering".into());
            }
            previous = segment.start_ms;
        }
        Ok(())
    }

    /// Correct only a one-frame native tail overrun (10 ms plus float tolerance).
    /// Other malformed predictions fail the stage, preserving ordinary ASR output.
    pub fn from_native(
        duration: f64,
        rows: &[(f64, f64, i32)],
        model_revision: &str,
        runtime_revision: &str,
        provider: &str,
    ) -> Result<Self, String> {
        if !duration.is_finite() || duration <= 0.0 || rows.len() > 100_000 {
            return Err("Invalid diarization duration or segment count".into());
        }
        let mut timeline = Self {
            duration_ms: (duration * 1000.0).ceil() as u64,
            segments: Vec::with_capacity(rows.len()),
            clipped_segments: 0,
            model_revision: model_revision.into(),
            runtime_revision: runtime_revision.into(),
            provider: provider.into(),
            fallback_reason: None,
        };
        for &(start, end, speaker) in rows {
            if !start.is_finite()
                || !end.is_finite()
                || start < 0.0
                || start >= end
                || start >= duration
                || end > duration + 0.010_001
                || !(1..=8).contains(&speaker)
            {
                return Err("Diarizer returned an invalid speaker interval".into());
            }
            timeline.clipped_segments += usize::from(end > duration);
            timeline.segments.push(SpeakerSegment {
                start_ms: (start * 1000.0).floor() as u64,
                end_ms: (end.min(duration) * 1000.0).ceil() as u64,
                speaker: speaker as u8,
            });
        }
        timeline.validate()?;
        Ok(timeline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_overlap_and_clamps_only_the_measured_frame_tail() {
        let t = SpeakerTimeline::from_native(
            45.0,
            &[(1.0, 3.0, 1), (2.0, 4.0, 2), (37.861, 45.010, 2)],
            "m",
            "r",
            "cpu",
        )
        .unwrap();
        assert_eq!(t.clipped_segments, 1);
        assert_eq!(t.segments[2].end_ms, 45000);
        assert_eq!(t.segments[0].end_ms, 3000);
        assert_eq!(t.segments[1].start_ms, 2000);
        for row in [
            (1.0, 45.02, 1),
            (-0.1, 1.0, 1),
            (0.0, f64::NAN, 1),
            (0.0, 1.0, 9),
            (1.0, 1.0, 1),
        ] {
            assert!(SpeakerTimeline::from_native(45.0, &[row], "m", "r", "cpu").is_err());
        }
        assert!(SpeakerTimeline::from_native(2.0, &[], "m", "r", "cpu")
            .unwrap()
            .segments
            .is_empty());
    }
}
