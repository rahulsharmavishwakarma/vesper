#include <cassert>
#include <limits>
#include <map>
#include <string>
#include "sherpa-onnx/csrc/vesper-decode-report.h"
#include "sherpa-onnx/csrc/vesper-whisper-score.h"
struct Stream {
  std::map<std::string, std::string> options;
  void SetOption(const std::string &key, const std::string &value) { options[key] = value; }
  std::string GetOption(const std::string &key) { return options[key]; }
};
int main() {
  using namespace sherpa_onnx;
  Stream a, b;
  VesperBegin(&a);
  assert(a.GetOption(kVesperDecodeReport).find("\"stopReason\":\"error\"") != std::string::npos);
  VesperInputTruncated(&a);
  VesperReport(&a, "eos");
  assert(a.GetOption(kVesperDecodeReport).find("\"inputContextTruncated\":true") != std::string::npos);
  VesperBegin(&b);
  assert(b.GetOption(kVesperInputTruncated) == "false");
  VesperBegin(&a);
  assert(a.GetOption(kVesperInputTruncated) == "false");
  assert(a.GetOption(kVesperDecodeReport).find("eos") == std::string::npos);
  // EOS on the last model/duration iteration must take precedence over the cap.
  assert(std::string(VesperCohereStop(true, true)) == "eos");
  assert(std::string(VesperCohereStop(true, false)) == "eos");
  assert(std::string(VesperCohereStop(false, true)) == "model_token_limit");
  assert(std::string(VesperCohereStop(false, false)) == "duration_token_limit");
  float logits[] = {std::numeric_limits<float>::quiet_NaN(), -std::numeric_limits<float>::infinity(), 1.0f};
  auto read = [&](int64_t index) { return logits[index]; };
  assert(!VesperHasFiniteLogit(0, -1, read));
  assert(!VesperHasFiniteLogit(2, -1, read));
  assert(VesperHasFiniteLogit(3, -1, read));
  assert(!VesperHasFiniteLogit(3, 2, read)); // EOS-only row with EOS excluded.
  assert(!VesperValidGreedyLogits(logits, 3)); // Mixed NaN/finite also invalid for Eigen.
  logits[0] = -std::numeric_limits<float>::infinity();
  assert(VesperValidGreedyLogits(logits, 3));
  assert(!VesperValidGreedyLogits(logits, 2));
  logits[0] = std::numeric_limits<float>::infinity();
  assert(!VesperValidGreedyLogits(logits, 3));
  // Stable softmax at large logits, masked logits, invalid token/NaN cases.
  float whisper[] = {10000.0f, 10000.0f, -INFINITY};
  assert(std::abs(VesperWhisperLogProbability(whisper, 3, 0) + std::log(2.0)) < 1e-12);
  for (int32_t token : {-1, 2, 3}) {
    bool rejected = false;
    try { VesperWhisperLogProbability(whisper, 3, token); }
    catch (const std::runtime_error &) { rejected = true; }
    assert(rejected);
  }
  whisper[1] = NAN;
  bool rejected = false;
  try { VesperWhisperLogProbability(whisper, 3, 0); }
  catch (const std::runtime_error &) { rejected = true; }
  assert(rejected);
  assert(VesperWhisperBudget(448, 4) == 224);
  assert(VesperWhisperBudget(16, 12) == 3);
  assert(VesperWhisperBudget(4, 4) == 0);
  assert(VesperWhisperBudget(448, -1) == 0);
  assert(std::string(VesperWhisperStop(true, true)) == "eos");
  assert(std::string(VesperWhisperStop(true, false)) == "eos");
  assert(std::string(VesperWhisperStop(false, true)) == "context_limit");
  assert(std::string(VesperWhisperStop(false, false)) == "model_token_limit");
}
