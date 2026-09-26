#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration for {field}: {message}")]
    InvalidConfig {
        field: &'static str,
        message: String,
    },
    #[error("{component} is not configured")]
    NotConfigured { component: &'static str },
    #[error("audio sample rate must be greater than zero (received {sample_rate})")]
    InvalidSampleRate { sample_rate: u32 },
    #[error("expected {expected} Hz audio, received {actual} Hz")]
    SampleRateMismatch { expected: u32, actual: u32 },
    #[error("audio sample {index} is not finite")]
    NonFiniteAudio { index: usize },
    #[error("{component} failed: {message}")]
    Backend {
        component: &'static str,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
