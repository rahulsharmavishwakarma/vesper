#include <cassert>
#include <limits>
#include <map>
#include <string>
#include "sherpa-onnx/csrc/transcribe-decode-report.h"
#include "sherpa-onnx/csrc/transcribe-whisper-score.h"
struct Stream {
  std::map<std::string, std::string> options;
  void SetOption(const std::string &key, const std::string &value) { options[key] = value; }
  std::string GetOption(const std::string &key) { return options[key]; }
};
int main() {
  using namespace sherpa_onnx;
  Stream a, b;
  TranscribeBegin(&a);
  assert(a.GetOption(kTranscribeDecodeReport).find("\"stopReason\":\"error\"") != std::string::npos);
  TranscribeInputTruncated(&a);
  TranscribeReport(&a, "eos");
  assert(a.GetOption(kTranscribeDecodeReport).find("\"inputContextTruncated\":true") != std::string::npos);
  TranscribeBegin(&b);
  assert(b.GetOption(kTranscribeInputTruncated) == "false");
  TranscribeBegin(&a);
  assert(a.GetOption(kTranscribeInputTruncated) == "false");
  assert(a.GetOption(kTranscribeDecodeReport).find("eos") == std::string::npos);
  // EOS on the last model/duration iteration must take precedence over the cap.
  assert(std::string(TranscribeCohereStop(true, true)) == "eos");
  assert(std::string(TranscribeCohereStop(true, false)) == "eos");
  assert(std::string(TranscribeCohereStop(false, true)) == "model_token_limit");
  assert(std::string(TranscribeCohereStop(false, false)) == "duration_token_limit");
  float logits[] = {std::numeric_limits<float>::quiet_NaN(), -std::numeric_limits<float>::infinity(), 1.0f};
  auto read = [&](int64_t index) { return logits[index]; };
  assert(!TranscribeHasFiniteLogit(0, -1, read));
  assert(!TranscribeHasFiniteLogit(2, -1, read));
  assert(TranscribeHasFiniteLogit(3, -1, read));
  assert(!TranscribeHasFiniteLogit(3, 2, read)); // EOS-only row with EOS excluded.
  assert(!TranscribeValidGreedyLogits(logits, 3)); // Mixed NaN/finite also invalid for Eigen.
  logits[0] = -std::numeric_limits<float>::infinity();
  assert(TranscribeValidGreedyLogits(logits, 3));
  assert(!TranscribeValidGreedyLogits(logits, 2));
  logits[0] = std::numeric_limits<float>::infinity();
  assert(!TranscribeValidGreedyLogits(logits, 3));
  // Stable softmax at large logits, masked logits, invalid token/NaN cases.
  float whisper[] = {10000.0f, 10000.0f, -INFINITY};
  assert(std::abs(TranscribeWhisperLogProbability(whisper, 3, 0) + std::log(2.0)) < 1e-12);
  for (int32_t token : {-1, 2, 3}) {
    bool rejected = false;
    try { TranscribeWhisperLogProbability(whisper, 3, token); }
    catch (const std::runtime_error &) { rejected = true; }
    assert(rejected);
  }
  whisper[1] = NAN;
  bool rejected = false;
  try { TranscribeWhisperLogProbability(whisper, 3, 0); }
  catch (const std::runtime_error &) { rejected = true; }
  assert(rejected);
  assert(TranscribeWhisperBudget(448, 4) == 224);
  assert(TranscribeWhisperBudget(16, 12) == 3);
  assert(TranscribeWhisperBudget(4, 4) == 0);
  assert(TranscribeWhisperBudget(448, -1) == 0);
  assert(std::string(TranscribeWhisperStop(true, true)) == "eos");
  assert(std::string(TranscribeWhisperStop(true, false)) == "eos");
  assert(std::string(TranscribeWhisperStop(false, true)) == "context_limit");
  assert(std::string(TranscribeWhisperStop(false, false)) == "model_token_limit");
}
