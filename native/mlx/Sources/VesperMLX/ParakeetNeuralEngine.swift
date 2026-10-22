import CoreML
import Foundation
import MLX
import MLXAudioSTT

// Parakeet TDT 0.6B v3 with its FastConformer encoder on the Neural Engine
// (#143). Vesper's converted directory (scripts/convert-parakeet-v3-coreml-ane.py)
// holds ParakeetEncoder15s.mlmodelc, a float16 Core ML program for one fixed
// 15 s window, beside an MLX checkpoint with zero encoder layers that keeps the
// TDT decoder and joint network. Parakeet's own log-mel frontend and decoder
// run in MLX; Core ML runs only the encoder. Rust delivers parts of at most
// 15 s, so each call is one encoder window with fresh decoder state.

/// Loads the encoder once and releases it, so Core ML specializes it for this
/// Mac's Neural Engine after download rather than on the first recording.
@_cdecl("vesper_mlx_prepare_neural_engine")
public func prepareNeuralEngine(_ path: UnsafePointer<CChar>?) -> Bool {
  guard let path else { return false }
  let configuration = MLModelConfiguration()
  configuration.computeUnits = .cpuAndNeuralEngine
  let encoder = URL(fileURLWithPath: String(cString: path), isDirectory: true)
    .appendingPathComponent(ParakeetNeuralEngineModel.encoderName)
  return (try? MLModel(contentsOf: encoder, configuration: configuration)) != nil
}

final class ParakeetNeuralEngineModel: STTGenerationModel {
  static let encoderName = "ParakeetEncoder15s.mlmodelc"
  static let windowFrames = 1501

  let parakeet: ParakeetModel
  let encoder: MLModel

  /// The first load on a Mac specializes the program for its Neural Engine
  /// (seconds); Core ML caches that per model path until the OS changes.
  init(directory: URL) throws {
    parakeet = try ParakeetModel.fromDirectory(directory)
    let configuration = MLModelConfiguration()
    configuration.computeUnits = .cpuAndNeuralEngine
    encoder = try MLModel(
      contentsOf: directory.appendingPathComponent(Self.encoderName), configuration: configuration)
  }

  var defaultGenerationParameters: STTGenerateParameters { parakeet.defaultGenerationParameters }

  func transcribe(_ audio: MLXArray) throws -> STTOutput {
    let mel = parakeet.vesperLogMel(audio).asType(.float32)
    let frames = mel.dim(0)
    guard frames <= Self.windowFrames, mel.dim(1) == 128 else {
      throw fail("Audio part exceeds the 15 s Neural Engine window")
    }
    // Channel-first [1, 128, 1501]; the program zeroes frames past mel_length.
    let values = MLX.padded(mel, widths: [.init((0, Self.windowFrames - frames)), .init((0, 0))])
      .transposed(1, 0).asArray(Float.self)
    let input = try MLMultiArray(
      shape: [1, 128, NSNumber(value: Self.windowFrames)], dataType: .float32)
    guard input.strides.map(\.intValue) == [128 * Self.windowFrames, Self.windowFrames, 1] else {
      throw fail("Unexpected Core ML input layout")
    }
    values.withUnsafeBufferPointer { source in
      input.dataPointer.copyMemory(
        from: source.baseAddress!, byteCount: source.count * MemoryLayout<Float>.stride)
    }
    let length = try MLMultiArray(shape: [1], dataType: .int32)
    length[0] = NSNumber(value: Int32(frames))
    let output = try encoder.prediction(
      from: MLDictionaryFeatureProvider(dictionary: ["mel": input, "mel_length": length]))
    guard let encoded = output.featureValue(for: "encoder")?.multiArrayValue,
      let encodedFrames = output.featureValue(for: "encoder_length")?.multiArrayValue?[0].intValue
    else { throw fail("Core ML encoder returned no features") }
    return parakeet.vesperDecode(
      encoded: try Self.channelsLast(encoded, frames: encodedFrames), frames: encodedFrames)
  }

  /// [1, dModel, T] Core ML output to MLX [1, frames, dModel]. The Neural Engine
  /// pads the time axis for alignment (188 -> 192), so read through the strides.
  static func channelsLast(_ encoded: MLMultiArray, frames: Int) throws -> MLXArray {
    let shape = encoded.shape.map(\.intValue)
    let strides = encoded.strides.map(\.intValue)
    guard shape.count == 3, shape[0] == 1, strides[2] == 1, strides[1] >= shape[2],
      strides[0] == shape[1] * strides[1], frames > 0, frames <= shape[2]
    else { throw fail("Unexpected Core ML encoder layout") }
    let storageShape = [1, shape[1], strides[1]]
    let count = shape[1] * strides[1]
    let storage: MLXArray
    switch encoded.dataType {
    case .float16:
      let pointer = encoded.dataPointer.assumingMemoryBound(to: Float16.self)
      storage = MLXArray(Array(UnsafeBufferPointer(start: pointer, count: count)), storageShape)
    case .float32:
      let pointer = encoded.dataPointer.assumingMemoryBound(to: Float.self)
      storage = MLXArray(Array(UnsafeBufferPointer(start: pointer, count: count)), storageShape)
    default:
      throw fail("Unexpected Core ML encoder precision")
    }
    return storage[0..., 0..., ..<frames].transposed(0, 2, 1)
  }

  func generate(audio: MLXArray, generationParameters: STTGenerateParameters) -> STTOutput {
    do { return try transcribe(audio) } catch {
      // The bridge calls transcribe directly; this keeps the protocol total.
      return STTOutput(text: "", vesperCompletionReason: "error")
    }
  }

  func generateStream(audio: MLXArray, generationParameters: STTGenerateParameters)
    -> AsyncThrowingStream<STTGeneration, Error>
  {
    AsyncThrowingStream { continuation in
      do {
        continuation.yield(.result(try transcribe(audio)))
        continuation.finish()
      } catch { continuation.finish(throwing: error) }
    }
  }
}
