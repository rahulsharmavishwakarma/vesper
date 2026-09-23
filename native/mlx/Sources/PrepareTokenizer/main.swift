import Foundation
import MLXAudioSTT

// Explicit qualification/install preparation, never called by inference.
// Verify the pinned source files before this command; hash its derived result.
guard CommandLine.arguments.count == 2 else {
  fatalError("usage: prepare-qwen-tokenizer VERIFIED_MODEL_DIRECTORY")
}
let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
for name in ["vocab.json", "merges.txt", "tokenizer_config.json"] {
  guard FileManager.default.fileExists(atPath: directory.appendingPathComponent(name).path) else {
    fatalError("Missing verified tokenizer input: \(name)")
  }
}
try Qwen3ASRModel.generateTokenizerJSONIfMissing(in: directory)
