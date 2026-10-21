import Foundation
import MLX
import MLXAudioSTT
import MLXNN

// Parakeet Redux (moondream/parakeet-redux): a ternary student of Parakeet TDT
// 0.6B v3. Vesper's converted files (scripts/convert-parakeet-redux-mlx.py)
// keep each of the 264 encoder projections as MLX affine 2-bit codes with the
// original fp16 per-group scales and bias = -scale, so w = scale * (code - 1).
// That is one native quantized matmul per projection; everything else is the
// pinned upstream Parakeet TDT graph, run in float32 as mlx-audio runs Redux.

/// Ternary codes and their per-(row, 128-input group) scales. Scales are kept
/// in float32 outside the module parameters so the loader's dtype cast cannot
/// round them.
struct TernaryPlane {
  let scales: MLXArray
  let biases: MLXArray

  func apply(_ x: MLXArray, codes: MLXArray) -> MLXArray {
    quantizedMatmul(
      x.asType(.float32), codes, scales: scales, biases: biases, transpose: true,
      groupSize: 128, bits: 2
    ).asType(x.dtype)
  }
}

/// Replaces an encoder `Linear`; `weight` is an unused [outputs, 1] placeholder.
final class TernaryLinear: Linear {
  @ParameterInfo(key: "codes") var codes: MLXArray
  let plane: TernaryPlane

  init(outputs: Int, inputs: Int, plane: TernaryPlane) {
    _codes.wrappedValue = MLXArray.zeros([outputs, inputs / 16], type: UInt32.self)
    self.plane = plane
    super.init(weight: MLXArray.zeros([outputs, 1]), bias: nil)
  }

  override func callAsFunction(_ x: MLXArray) -> MLXArray { plane.apply(x, codes: codes) }
}

/// Replaces a kernel-1 encoder `Conv1d` (channels-last input).
final class TernaryConv1d: Conv1d {
  @ParameterInfo(key: "codes") var codes: MLXArray
  let plane: TernaryPlane

  init(outputs: Int, inputs: Int, plane: TernaryPlane) {
    _codes.wrappedValue = MLXArray.zeros([outputs, inputs / 16], type: UInt32.self)
    self.plane = plane
    super.init(inputChannels: 1, outputChannels: outputs, kernelSize: 1, bias: false)
  }

  override func callAsFunction(_ x: MLXArray) -> MLXArray { plane.apply(x, codes: codes) }
}

enum ParakeetRedux {
  private struct Config: Decodable {
    struct Module: Decodable {
      let name: String
      let shape: [Int]
    }
    struct Encoder: Decodable { let nLayers: Int }
    let encoder: Encoder
    let vesperTernaryModules: [Module]
  }

  private static let projections = [
    "feed_forward1.linear1", "feed_forward1.linear2", "feed_forward2.linear1",
    "feed_forward2.linear2", "self_attn.linear_q", "self_attn.linear_k", "self_attn.linear_v",
    "self_attn.linear_out", "self_attn.linear_pos", "conv.pointwise_conv1",
    "conv.pointwise_conv2",
  ]

  static func fromDirectory(_ directory: URL) throws -> ParakeetModel {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let config = try decoder.decode(
      Config.self, from: Data(contentsOf: directory.appendingPathComponent("config.json")))
    let names = config.vesperTernaryModules.map(\.name)
    let expected = Set(
      (0 ..< config.encoder.nLayers).flatMap { layer in
        projections.map { "encoder.layers.\(layer).\($0)" }
      })
    guard names.count == Set(names).count, Set(names) == expected,
      config.vesperTernaryModules.allSatisfy({
        $0.shape.count == 2 && $0.shape[0] > 0 && $0.shape[1] > 0 && $0.shape[1] % 128 == 0
      })
    else { throw fail("Redux must replace every encoder projection exactly once") }

    return try ParakeetModel.fromDirectory(directory, computeDType: .float32) { model, loaded in
      var weights = loaded
      var replacements: [(String, Module)] = []
      for module in config.vesperTernaryModules {
        let (outputs, inputs) = (module.shape[0], module.shape[1])
        guard let codes = weights.removeValue(forKey: module.name + ".weight"),
          let scales = weights.removeValue(forKey: module.name + ".scales"),
          let biases = weights.removeValue(forKey: module.name + ".biases"),
          codes.dtype == .uint32, codes.shape == [outputs, inputs / 16],
          scales.shape == [outputs, inputs / 128], biases.shape == scales.shape,
          scales.dtype.isFloatingPoint, biases.dtype == scales.dtype
        else { throw fail("Invalid Redux ternary weights for \(module.name)") }
        let plane = TernaryPlane(scales: scales.asType(.float32), biases: biases.asType(.float32))
        let conv = module.name.hasSuffix("pointwise_conv1") || module.name.hasSuffix("pointwise_conv2")
        let replacement: Module =
          conv
          ? TernaryConv1d(outputs: outputs, inputs: inputs, plane: plane)
          : TernaryLinear(outputs: outputs, inputs: inputs, plane: plane)
        weights[module.name + ".codes"] = codes
        weights[module.name + ".weight"] =
          conv ? MLXArray.zeros([outputs, 1, 1]) : MLXArray.zeros([outputs, 1])
        replacements.append((module.name, replacement))
      }
      try model.update(modules: ModuleChildren.unflattened(replacements), verify: .all)
      return weights
    }
  }
}
