#!/usr/bin/env python3
"""Check tokenizer equivalence and the maintained Transformers loader APIs."""

import argparse
import hashlib
import importlib.metadata
import json
import os
import sys
import tempfile
from pathlib import Path

from vibevoice_compat import REPOSITORY, digest, prepare_source


def check_equal(actual, expected, name):
    if json.dumps(actual, sort_keys=True) != json.dumps(expected, sort_keys=True):
        raise ValueError(f"Tokenizer compatibility mismatch: {name}")


def check_model_api():
    """Exercise tying and loader initialization with small synthetic weights."""
    import torch
    from transformers import AutoModel, Qwen2Config
    from transformers import VibeVoiceAcousticTokenizerConfig as BuiltinAcousticConfig
    from vibevoice.modular.configuration_vibevoice import (
        VibeVoiceAcousticTokenizerConfig,
        VibeVoiceASRConfig,
        VibeVoiceSemanticTokenizerConfig,
    )
    from vibevoice.modular.modeling_vibevoice_asr import VibeVoiceASRForConditionalGeneration
    from vibevoice.modular.modular_vibevoice_tokenizer import VibeVoiceAcousticTokenizerModel

    if (
        AutoModel._model_mapping[VibeVoiceAcousticTokenizerConfig]
        is not VibeVoiceAcousticTokenizerModel
    ):
        raise ValueError("Publisher acoustic configuration no longer dispatches to its own model")
    if not AutoModel._model_mapping[BuiltinAcousticConfig].__module__.startswith("transformers."):
        raise ValueError("Publisher registration overrides the built-in acoustic configuration")
    torch.set_num_threads(1)
    for tied in (True, False):
        common = dict(vae_dim=4, encoder_n_filters=4, encoder_ratios=[2], encoder_depths="1-1")
        config = VibeVoiceASRConfig(
            acoustic_tokenizer_config=VibeVoiceAcousticTokenizerConfig(
                **common, decoder_n_filters=4, decoder_depths="1-1"
            ),
            semantic_tokenizer_config=VibeVoiceSemanticTokenizerConfig(**common),
            decoder_config=Qwen2Config(
                vocab_size=32,
                hidden_size=16,
                intermediate_size=32,
                num_hidden_layers=1,
                num_attention_heads=2,
                num_key_value_heads=2,
                tie_word_embeddings=tied,
            ),
        )
        model = VibeVoiceASRForConditionalGeneration(config).eval()
        expected = {name: value.detach().clone() for name, value in model.state_dict().items()}
        with tempfile.TemporaryDirectory(prefix="transcribe-vibevoice-api-") as directory:
            model.save_pretrained(directory)
            loaded, info = VibeVoiceASRForConditionalGeneration.from_pretrained(
                directory, local_files_only=True, output_loading_info=True
            )
        if any(info.values()):
            raise ValueError(f"Synthetic checkpoint loading mismatch: {info}")
        for candidate in (model, loaded):
            acoustic = candidate.model.acoustic_tokenizer
            if acoustic.fix_std.item() != float(acoustic.config.fix_std):
                raise ValueError("Non-persistent acoustic sampling buffer was not initialized")
            shared = (
                candidate.get_input_embeddings().weight is candidate.get_output_embeddings().weight
            )
            if shared != tied:
                raise ValueError(f"Expected tie_word_embeddings={tied}")
        if not all(
            torch.equal(expected[name], value) for name, value in loaded.state_dict().items()
        ):
            raise ValueError("Synthetic checkpoint tensors changed during loading")
    return {
        "tiedRoundTrip": True,
        "untiedRoundTrip": True,
        "distinctAcousticDispatch": True,
        "acousticSamplingBuffer": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    baseline = json.loads((REPOSITORY / "tests/fixtures/vibevoice-tokenizer-tf4.json").read_text())
    pins = json.loads((args.root / "verified-artifacts.json").read_text())
    # This check loads tokenization assets only, not the multi-GB model tensors.
    for artifact in pins["artifacts"]:
        if not artifact["file"].endswith(".safetensors"):
            if digest(args.root / "model" / artifact["file"]) != artifact["sha256"]:
                raise ValueError(f"Artifact hash mismatch: {artifact['file']}")
    prepared, patch = prepare_source(args.root)
    if importlib.metadata.version("transformers") != patch["transformersVersion"]:
        raise ValueError("Use the locked VibeVoice environment")
    sys.path.insert(0, prepared.name)
    from vibevoice.processor.vibevoice_asr_processor import VibeVoiceASRProcessor

    tokenizer = VibeVoiceASRProcessor.from_pretrained(
        str(args.root / "model"), local_files_only=True
    ).tokenizer
    check_equal(len(tokenizer), baseline["tokenizerLength"], "length")
    check_equal(tokenizer.vocab_size, baseline["baseVocabSize"], "vocabulary size")
    vocab = json.dumps(
        tokenizer.get_vocab(), sort_keys=True, ensure_ascii=False, separators=(",", ":")
    )
    check_equal(hashlib.sha256(vocab.encode()).hexdigest(), baseline["vocabSha256"], "vocabulary")
    for name, value in baseline["tokenizerProperties"].items():
        check_equal(getattr(tokenizer, name), value, name)
    added = [
        {"id": key, **value.__getstate__()}
        for key, value in sorted(tokenizer.added_tokens_decoder.items())
    ]
    check_equal(added, baseline["addedTokens"], "added tokens")
    for row in baseline["controls"]:
        for expected in row["encodings"]:
            encoded = tokenizer(
                row["text"],
                add_special_tokens=expected["addSpecialTokens"],
                return_offsets_mapping=True,
            )
            ids = encoded["input_ids"]
            actual = {
                "addSpecialTokens": expected["addSpecialTokens"],
                "ids": ids,
                "tokens": tokenizer.convert_ids_to_tokens(ids),
                "offsets": encoded["offset_mapping"],
                "attentionMask": encoded["attention_mask"],
                "decoded": tokenizer.decode(
                    ids, skip_special_tokens=False, clean_up_tokenization_spaces=False
                ),
                "decodedSkipSpecial": tokenizer.decode(
                    ids, skip_special_tokens=True, clean_up_tokenization_spaces=False
                ),
            }
            check_equal(actual, expected, row["id"])
    for row in baseline["chatTemplates"]:
        for tokenize, key in ((True, "ids"), (False, "rendered")):
            actual = tokenizer.apply_chat_template(
                row["messages"],
                tokenize=tokenize,
                add_generation_prompt=row["addGenerationPrompt"],
                return_dict=False,
            )
            check_equal(actual, row[key], f"chat template {row['conversationId']}: {key}")
    result = {
        "passed": True,
        "controls": len(baseline["controls"]),
        "chatTemplates": len(baseline["chatTemplates"]),
        "patch": patch,
        "packages": {name: importlib.metadata.version(name) for name in baseline["packages"]},
        "modelApi": check_model_api(),
    }
    with args.output.open("x") as stream:
        stream.write(json.dumps(result, indent=2) + "\n")
    print("Tokenizer equivalence, tied/untied loading and acoustic configuration dispatch passed.")


if __name__ == "__main__":
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    main()
