// Opt-in comparison probe: installed weights and public WAVs only; no downloads.
#include <chrono>
#include <iostream>
#include <string>
#include "sherpa-onnx/c-api/c-api.h"

int main(int argc, char **argv) {
  if (argc != 5) {
    std::cerr << "Usage: asr-probe qwen|cohere|parakeet MODEL_ROOT WAV LANGUAGE\n";
    return 2;
  }
  const std::string mode = argv[1], root = argv[2];
  const std::string folder = root + (mode == "qwen" ? "/qwen3-asr-0.6b/" :
      mode == "cohere" ? "/cohere-transcribe/" : "/parakeet-tdt-0.6b-v3-wo4/");
  // Built-in Parakeet uses 4-bit weight-only files without the .int8 suffix.
  const std::string suffix = mode == "parakeet" ? ".onnx" : ".int8.onnx";
  const std::string encoder = folder + "encoder" + suffix;
  const std::string decoder = folder + "decoder" + suffix;
  const std::string joiner = folder + "joiner" + suffix;
  const std::string tokens = folder + "tokens.txt";
  const std::string frontend = folder + "conv_frontend.onnx";
  const std::string tokenizer = folder + "tokenizer";
  SherpaOnnxOfflineRecognizerConfig config{};
  config.feat_config.sample_rate = 16000;
  config.feat_config.feature_dim = 80;
  config.model_config.num_threads = 8;
  config.model_config.provider = "cpu";
  config.decoding_method = "greedy_search";
  if (mode == "qwen") {
    auto &q = config.model_config.qwen3_asr;
    q.conv_frontend = frontend.c_str(); q.encoder = encoder.c_str();
    q.decoder = decoder.c_str(); q.tokenizer = tokenizer.c_str();
    q.max_total_len = 512; q.max_new_tokens = 128;
    q.temperature = 1e-6f; q.top_p = 0.8f; q.seed = 42;
  } else if (mode == "cohere") {
    auto &c = config.model_config.cohere_transcribe;
    c.encoder = encoder.c_str(); c.decoder = decoder.c_str();
    c.language = argv[4]; c.use_punct = 1;
    config.model_config.tokens = tokens.c_str();
  } else if (mode == "parakeet") {
    auto &t = config.model_config.transducer;
    t.encoder = encoder.c_str(); t.decoder = decoder.c_str(); t.joiner = joiner.c_str();
    config.model_config.tokens = tokens.c_str();
    config.model_config.model_type = "nemo_transducer";
  } else {
    return 2;
  }
  const auto *wave = SherpaOnnxReadWave(argv[3]);
  if (!wave || wave->sample_rate != 16000 || wave->num_samples > 160000) return 3;
  const auto begin = std::chrono::steady_clock::now();
  const auto *recognizer = SherpaOnnxCreateOfflineRecognizer(&config);
  if (!recognizer) return 4;
  const auto loaded = std::chrono::steady_clock::now();
  const auto *stream = SherpaOnnxCreateOfflineStream(recognizer);
  if (mode != "parakeet") SherpaOnnxOfflineStreamSetOption(stream, "language", argv[4]);
  SherpaOnnxAcceptWaveformOffline(stream, wave->sample_rate, wave->samples, wave->num_samples);
  const auto decoding = std::chrono::steady_clock::now();
  SherpaOnnxDecodeOfflineStream(recognizer, stream);
  const auto finished = std::chrono::steady_clock::now();
  const char *result = SherpaOnnxGetOfflineStreamResultAsJson(stream);
  const char *report = SherpaOnnxOfflineStreamGetOption(stream, "vesper.decode_report.v1");
  std::cout << "{\"loadSeconds\":" << std::chrono::duration<double>(loaded - begin).count()
            << ",\"decodeSeconds\":" << std::chrono::duration<double>(finished - decoding).count()
            << ",\"coldAsrSeconds\":" << std::chrono::duration<double>(finished - begin).count()
            << ",\"report\":" << (report && *report ? report : "null")
            << ",\"result\":" << (result ? result : "null") << "}\n";
  const bool has_result = result != nullptr;
  if (result) SherpaOnnxDestroyOfflineStreamResultJson(result);
  SherpaOnnxDestroyOfflineStream(stream);
  SherpaOnnxDestroyOfflineRecognizer(recognizer);
  SherpaOnnxFreeWave(wave);
  return has_result ? 0 : 5;
}
