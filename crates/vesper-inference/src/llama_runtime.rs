//! One native backend initialization shared by speech and writing adapters.
use crate::{native, Result};
use llama_cpp_2::llama_backend::LlamaBackend;
use std::sync::OnceLock;

static BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();

pub(crate) fn backend() -> Result<&'static LlamaBackend> {
    BACKEND
        .get_or_init(|| LlamaBackend::init().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| native("llama.cpp initialization", error))
}
