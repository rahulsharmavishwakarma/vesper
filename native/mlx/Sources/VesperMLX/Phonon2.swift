import Foundation
import MLX
import MLXAudioSTT
import MLXNN

// Phonon-2 (FermionResearch/Phonon-2, MLX build mlx-community/Phonon-2):
// Parakeet TDT 0.6B v3 whose 264 encoder projections store five-value weights.
// Each row r keeps an fp16 palette (lo, hi) and two 2-bit code planes q and s:
//   w = lo * (q - 1) + (hi - lo) * (s - 1)
// which executes exactly as two native MLX affine 2-bit matmuls with
// scale = lo, bias = -lo and scale = hi - lo, bias = -(hi - lo). Mirrors
// mlx-audio's `phonon2/packed.py`, adapted from fermion-research 0.2.11
// (Copyright 2026 Fermion Research, Apache-2.0). The rest of the graph is the
// pinned upstream Parakeet TDT implementation.

private let groupSize = 128
private let projectionSuffixes = [
  "feed_forward1.linear1", "feed_forward1.linear2", "feed_forward2.linear1",
  "feed_forward2.linear2", "self_attn.linear_q", "self_attn.linear_k", "self_attn.linear_v",
  "self_attn.linear_out", "self_attn.linear_pos", "conv.pointwise_conv1",
  "conv.pointwise_conv2",
]

/// Shared arithmetic. Scales are derived once from the fp16 palettes in
/// float32 (hi - lo rounded to fp16 would change the learned weights) and are
/// not module parameters, so the loader's later dtype cast never touches them.
struct FiveValuePlanes {
  let baseScales: MLXArray
  let residualScales: MLXArray

  init(lo: MLXArray, hi: MLXArray, inputs: Int) {
    let groups = inputs / groupSize
    let lo32 = lo.asType(.float32)
    let delta = hi.asType(.float32) - lo32
    baseScales = contiguous(broadcast(lo32.expandedDimensions(axis: 1), to: [lo.dim(0), groups]))
    residualScales = contiguous(
      broadcast(delta.expandedDimensions(axis: 1), to: [lo.dim(0), groups]))
  }

  func apply(_ x: MLXArray, base: MLXArray, residual: MLXArray) -> MLXArray {
    let x32 = x.asType(.float32)
    let a = quantizedMatmul(
      x32, base, scales: baseScales, biases: -baseScales, transpose: true,
      groupSize: groupSize, bits: 2)
    let b = quantizedMatmul(
      x32, residual, scales: residualScales, biases: -residualScales, transpose: true,
      groupSize: groupSize, bits: 2)
    return (a + b).asType(x.dtype)
  }
}

/// Replaces an encoder `Linear`. `weight` is an unused [outputs, 1] placeholder
/// so the upstream slot type and strict key verification still apply.
final class PackedFiveValueLinear: Linear {
  @ParameterInfo(key: "base_q") var baseQ: MLXArray
  @ParameterInfo(key: "residual_q") var residualQ: MLXArray
  let planes: FiveValuePlanes

  init(outputs: Int, inputs: Int, planes: FiveValuePlanes) {
    _baseQ.wrappedValue = MLXArray.zeros([outputs, inputs / 16], type: UInt32.self)
    _residualQ.wrappedValue = MLXArray.zeros([outputs, inputs / 16], type: UInt32.self)
    self.planes = planes
    super.init(weight: MLXArray.zeros([outputs, 1]), bias: nil)
  }

  override func callAsFunction(_ x: MLXArray) -> MLXArray {
    planes.apply(x, base: baseQ, residual: residualQ)
  }
}

/// Replaces a kernel-1 encoder `Conv1d` (channels-last input), which is the
/// same projection over the last axis.
final class PackedFiveValueConv1d: Conv1d {
  @ParameterInfo(key: "base_q") var baseQ: MLXArray
  @ParameterInfo(key: "residual_q") var residualQ: MLXArray
  let planes: FiveValuePlanes

  init(outputs: Int, inputs: Int, planes: FiveValuePlanes) {
    _baseQ.wrappedValue = MLXArray.zeros([outputs, inputs / 16], type: UInt32.self)
    _residualQ.wrappedValue = MLXArray.zeros([outputs, inputs / 16], type: UInt32.self)
    self.planes = planes
    super.init(inputChannels: 1, outputChannels: outputs, kernelSize: 1, bias: false)
  }

  override func callAsFunction(_ x: MLXArray) -> MLXArray {
    planes.apply(x, base: baseQ, residual: residualQ)
  }
}

enum Phonon2 {
  private struct Config: Decodable {
    struct Module: Decodable {
      let name: String
      let shape: [Int]
    }
    struct Encoder: Decodable { let nLayers: Int }
    let modelType: String
    let encoder: Encoder
    let phonon2Modules: [Module]
  }

  static func fromDirectory(_ directory: URL) throws -> ParakeetModel {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let config = try decoder.decode(
      Config.self, from: Data(contentsOf: directory.appendingPathComponent("config.json")))
    let names = config.phonon2Modules.map(\.name)
    let expected = Set(
      (0 ..< config.encoder.nLayers).flatMap { layer in
        projectionSuffixes.map { "encoder.layers.\(layer).\($0)" }
      })
    guard config.modelType == "phonon2", names.count == Set(names).count, Set(names) == expected,
      config.phonon2Modules.allSatisfy({
        $0.shape.count == 2 && $0.shape[0] > 0 && $0.shape[1] > 0 && $0.shape[1] % groupSize == 0
      })
    else { throw fail("Phonon-2 must replace every encoder projection exactly once") }

    return try ParakeetModel.fromDirectory(directory) { model, loaded in
      var weights = loaded
      var replacements: [(String, Module)] = []
      for module in config.phonon2Modules {
        let (outputs, inputs) = (module.shape[0], module.shape[1])
        guard let base = weights[module.name + ".base_q"],
          let residual = weights[module.name + ".residual_q"],
          let lo = weights.removeValue(forKey: module.name + ".row_lo"),
          let hi = weights.removeValue(forKey: module.name + ".row_hi"),
          base.dtype == .uint32, residual.dtype == .uint32, lo.dtype == .float16,
          hi.dtype == .float16, base.shape == [outputs, inputs / 16],
          residual.shape == base.shape, lo.shape == [outputs], hi.shape == [outputs]
        else { throw fail("Invalid Phonon-2 packed weights for \(module.name)") }
        let planes = FiveValuePlanes(lo: lo, hi: hi, inputs: inputs)
        let packed: Module =
          module.name.hasSuffix("pointwise_conv1") || module.name.hasSuffix("pointwise_conv2")
          ? PackedFiveValueConv1d(outputs: outputs, inputs: inputs, planes: planes)
          : PackedFiveValueLinear(outputs: outputs, inputs: inputs, planes: planes)
        weights[module.name + ".weight"] =
          packed is Conv1d ? MLXArray.zeros([outputs, 1, 1]) : MLXArray.zeros([outputs, 1])
        replacements.append((module.name, packed))
      }
      try model.update(modules: ModuleChildren.unflattened(replacements), verify: .all)
      return weights
    }
  }
}

extension Phonon2 {
  /// Vesper adaptation. Phonon-2's compressed encoder often emits digit pieces
  /// without the SentencePiece word boundary, giving "about70 km", "in1990"
  /// and "800 ." (the reference runtimes return that text unchanged). This
  /// restores only digit/word boundaries; acronyms with digits ("MP3", "H2O"),
  /// ordinals ("4th") and plurals ("1990s") are left alone.
  static func repairDigitSpacing(_ text: String) -> String {
    var result = replace(#"\b([A-Za-z]*[a-z][A-Za-z]*)(?=\d)"#, in: text) { word, _, _ in
      word.count >= 2 ? word + " " : word
    }
    result = replace(#"(\d)(?=[A-Za-z]{2,})"#, in: result) { digit, rest, _ in
      rest.range(of: #"^(st|nd|rd|th|s)\b"#, options: [.regularExpression, .caseInsensitive])
        == nil ? digit + " " : digit
    }
    result = result.replacingOccurrences(
      of: #"\s+([.,;:!?%)])"#, with: "$1", options: .regularExpression)
    return result.replacingOccurrences(
      of: #"(\d)\s+-(\d)"#, with: "$1-$2", options: .regularExpression)
  }

  /// Replaces each first capture group; `transform` also receives the text
  /// after the match and the match range.
  private static func replace(
    _ pattern: String, in text: String,
    _ transform: (String, Substring, Range<String.Index>) -> String
  ) -> String {
    let expression = try! NSRegularExpression(pattern: pattern)
    var output = ""
    var cursor = text.startIndex
    for match in expression.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
      guard let range = Range(match.range, in: text),
        let group = Range(match.range(at: 1), in: text)
      else { continue }
      output += text[cursor ..< range.lowerBound]
      output += transform(String(text[group]), text[range.upperBound...], range)
      cursor = range.upperBound
    }
    return output + text[cursor...]
  }
}

/// The application model: unmodified Parakeet TDT decoding plus the digit
/// spacing repair. Completion reporting comes from the pinned Parakeet patch.
final class Phonon2Model: STTGenerationModel {
  let parakeet: ParakeetModel

  init(_ parakeet: ParakeetModel) { self.parakeet = parakeet }

  var defaultGenerationParameters: STTGenerateParameters { parakeet.defaultGenerationParameters }

  func generate(audio: MLXArray, generationParameters: STTGenerateParameters) -> STTOutput {
    let output = parakeet.generate(audio: audio, generationParameters: generationParameters)
    return STTOutput(
      text: Phonon2.repairDigitSpacing(output.text), segments: output.segments,
      language: output.language, promptTokens: output.promptTokens,
      generationTokens: output.generationTokens, totalTokens: output.totalTokens,
      promptTps: output.promptTps, generationTps: output.generationTps,
      totalTime: output.totalTime, peakMemoryUsage: output.peakMemoryUsage,
      vesperCompletionReason: output.vesperCompletionReason,
      vesperWhisperScores: output.vesperWhisperScores)
  }

  func generateStream(audio: MLXArray, generationParameters: STTGenerateParameters)
    -> AsyncThrowingStream<STTGeneration, Error>
  {
    AsyncThrowingStream { continuation in
      continuation.yield(.result(generate(audio: audio, generationParameters: generationParameters)))
      continuation.finish()
    }
  }
}
