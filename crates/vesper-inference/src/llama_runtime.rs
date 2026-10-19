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

/// Whether tokenization may add the model's special BOS/EOS tokens.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddBos {
    Always,
    Never,
}

/// llama-cpp-2 0.1.159 moved tokenization and token metadata from
/// `LlamaModel` to `LlamaVocab`. This keeps the previous call shape and exact
/// semantics: `llama_tokenize(add_special = Always, parse_special = true)` and
/// streaming UTF-8 decoding of each piece.
pub(crate) trait ModelText {
    fn str_to_token(
        &self,
        text: &str,
        add_bos: AddBos,
    ) -> std::result::Result<Vec<llama_cpp_2::token::LlamaToken>, std::convert::Infallible>;
    fn token_to_piece(
        &self,
        token: llama_cpp_2::token::LlamaToken,
        decoder: &mut encoding_rs::Decoder,
        special: bool,
        lstrip: Option<std::num::NonZeroU16>,
    ) -> std::result::Result<String, std::convert::Infallible>;
    fn is_eog_token(&self, token: llama_cpp_2::token::LlamaToken) -> bool;
    fn token_attr(
        &self,
        token: llama_cpp_2::token::LlamaToken,
    ) -> llama_cpp_2::token_type::LlamaTokenAttrs;
    fn token_bos(&self) -> llama_cpp_2::token::LlamaToken;
}

impl ModelText for llama_cpp_2::model::LlamaModel {
    fn str_to_token(
        &self,
        text: &str,
        add_bos: AddBos,
    ) -> std::result::Result<Vec<llama_cpp_2::token::LlamaToken>, std::convert::Infallible> {
        Ok(self
            .vocab()
            .tokenize(text.as_bytes(), add_bos == AddBos::Always, true))
    }

    fn token_to_piece(
        &self,
        token: llama_cpp_2::token::LlamaToken,
        decoder: &mut encoding_rs::Decoder,
        special: bool,
        lstrip: Option<std::num::NonZeroU16>,
    ) -> std::result::Result<String, std::convert::Infallible> {
        let bytes = self.vocab().token_to_piece(token, special, lstrip);
        // Same bound and full-consumption contract as llama-cpp-2 0.1.156.
        let mut output = String::with_capacity(
            decoder
                .max_utf8_buffer_length(bytes.len())
                .expect("token output is too large to decode"),
        );
        let (result, read, _) = decoder.decode_to_string(&bytes, &mut output, false);
        assert!(
            matches!(result, encoding_rs::CoderResult::InputEmpty) && read == bytes.len(),
            "UTF-8 decoder capacity bound must consume the complete token"
        );
        Ok(output)
    }

    fn is_eog_token(&self, token: llama_cpp_2::token::LlamaToken) -> bool {
        self.vocab().is_eog(token)
    }

    fn token_attr(
        &self,
        token: llama_cpp_2::token::LlamaToken,
    ) -> llama_cpp_2::token_type::LlamaTokenAttrs {
        self.vocab().attr(token)
    }

    fn token_bos(&self) -> llama_cpp_2::token::LlamaToken {
        self.vocab().bos()
    }
}
