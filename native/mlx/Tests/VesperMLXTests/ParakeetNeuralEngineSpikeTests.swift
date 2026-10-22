import CoreML
import Foundation
import MLX
import XCTest

@testable import MLXAudioSTT

/// Research spike for #143 (not product code). Runs Vesper's Parakeet mel,
/// a Core ML encoder on the Neural Engine and the existing MLX TDT decoder,
/// and compares each clip with the current all-MLX path. Opt in with
/// VESPER_ANE_SPIKE: a JSON file with `parakeet` (MLX model directory),
/// `encoder` (compiled .mlmodelc for a 15 s window), `frames` (1501),
/// `audio` (raw f32 PCM paths) and `output` (results JSON path).
final class ParakeetNeuralEngineSpikeTests: XCTestCase {
  private func milliseconds(_ start: ContinuousClock.Instant) -> Double {
    let elapsed = start.duration(to: .now).components
    return Double(elapsed.seconds) * 1000 + Double(elapsed.attoseconds) / 1e15
  }

  /// One cut at the quietest 100 ms within 3 s of the middle, as in the
  /// Python spike, so every part fits a 15 s encoder window.
  private func parts(_ audio: [Float], limit: Int) -> [[Float]] {
    guard audio.count > limit else { return [audio] }
    let window = 1600
    let middle = audio.count / 2
    var best = (energy: Float.infinity, index: middle)
    for start in stride(
      from: max(window, middle - 48_000), to: min(audio.count - window, middle + 48_000), by: 160)
    {
      let energy = audio[start ..< start + window].reduce(0) { $0 + $1 * $1 }
      if energy < best.energy { best = (energy, start) }
    }
    let cut = best.index + window / 2
    return [Array(audio[..<cut]), Array(audio[cut...])]
  }

  func testNeuralEngineEncoderAgainstMLXWhenFixtureIsProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["VESPER_ANE_SPIKE"] else {
      throw XCTSkip("VESPER_ANE_SPIKE is not set")
    }
    let fixture = try XCTUnwrap(
      JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))
        as? [String: Any])
    let frames = try XCTUnwrap(fixture["frames"] as? Int)
    let parakeet = try ParakeetModel.fromDirectory(
      URL(fileURLWithPath: try XCTUnwrap(fixture["parakeet"] as? String), isDirectory: true))
    let configuration = MLModelConfiguration()
    configuration.computeUnits = .cpuAndNeuralEngine
    let loadStart = ContinuousClock.now
    let encoder = try MLModel(
      contentsOf: URL(fileURLWithPath: try XCTUnwrap(fixture["encoder"] as? String)),
      configuration: configuration)
    let encoderLoadMs = milliseconds(loadStart)
    let parameters = STTGenerateParameters(
      maxTokens: 512, temperature: 0, verbose: false, language: nil, chunkDuration: 30,
      minChunkDuration: 0)

    func neuralEngine(_ samples: [Float]) throws -> (text: String, encoderMs: Double) {
      var texts: [String] = []
      var encoderMs = 0.0
      for part in parts(samples, limit: (frames - 1) * 160) {
        var mel = ParakeetAudio.logMelSpectrogram(MLXArray(part), config: parakeet.preprocessConfig)
        if mel.ndim == 3 { mel = mel[0] }
        let length = min(mel.dim(0), frames)
        let padded = MLX.padded(
          mel[..<length].asType(.float32),
          widths: [.init((0, frames - length)), .init((0, 0))])
        let input = try MLMultiArray(shape: [1, 128, NSNumber(value: frames)], dataType: .float32)
        let values = padded.transposed(1, 0).asArray(Float.self)
        values.withUnsafeBufferPointer { source in
          input.dataPointer.copyMemory(
            from: source.baseAddress!, byteCount: values.count * MemoryLayout<Float>.stride)
        }
        let lengthInput = try MLMultiArray(shape: [1], dataType: .int32)
        lengthInput[0] = NSNumber(value: Int32(length))
        let start = ContinuousClock.now
        let output = try encoder.prediction(
          from: MLDictionaryFeatureProvider(dictionary: ["mel": input, "mel_length": lengthInput]))
        encoderMs += milliseconds(start)
        let encoded = try XCTUnwrap(output.featureValue(for: "encoder")?.multiArrayValue)
        let encodedLength = try XCTUnwrap(
          output.featureValue(for: "encoder_length")?.multiArrayValue)[0].intValue
        // The Neural Engine pads the last axis for alignment (188 -> 192 here);
        // read through the strides, then drop the padding.
        let shape = encoded.shape.map(\.intValue)
        let strides = encoded.strides.map(\.intValue)
        XCTAssertEqual(strides[2], 1)
        XCTAssertEqual(strides[0], shape[1] * strides[1])
        let storageShape = [shape[0], shape[1], strides[1]]
        let count = storageShape.reduce(1, *)
        let storage: MLXArray
        if encoded.dataType == .float16 {
          let pointer = encoded.dataPointer.assumingMemoryBound(to: Float16.self)
          storage = MLXArray(Array(UnsafeBufferPointer(start: pointer, count: count)), storageShape)
        } else {
          let pointer = encoded.dataPointer.assumingMemoryBound(to: Float.self)
          storage = MLXArray(Array(UnsafeBufferPointer(start: pointer, count: count)), storageShape)
        }
        let raw = storage[0..., 0..., ..<shape[2]]
        let features = raw.transposed(0, 2, 1)[0..., ..<encodedLength].asType(parakeet.computeDType)
        texts.append(
          parakeet.decodeEncoded(
            batchFeatures: features, lengths: MLXArray([Int32(encodedLength)])
          ).first?.text ?? "")
      }
      return (texts.joined(separator: " "), encoderMs)
    }

    var rows: [[String: Any]] = []
    let clips = try XCTUnwrap(fixture["audio"] as? [String])
    for (index, file) in clips.enumerated() {
      let samples = try Data(contentsOf: URL(fileURLWithPath: file)).withUnsafeBytes {
        Array($0.bindMemory(to: Float.self))
      }
      if index == 0 {
        _ = try neuralEngine(samples)
        _ = parakeet.generate(audio: MLXArray(samples), generationParameters: parameters)
      }
      var start = ContinuousClock.now
      let ane = try neuralEngine(samples)
      let aneMs = milliseconds(start)
      start = ContinuousClock.now
      let mlx = parakeet.generate(audio: MLXArray(samples), generationParameters: parameters).text
      let mlxMs = milliseconds(start)
      rows.append([
        "audio": file, "ane": ane.text, "mlx": mlx, "aneMs": aneMs,
        "aneEncoderMs": ane.encoderMs, "mlxMs": mlxMs,
      ])
    }
    let report: [String: Any] = ["encoderLoadMs": encoderLoadMs, "rows": rows]
    try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted])
      .write(to: URL(fileURLWithPath: try XCTUnwrap(fixture["output"] as? String)))
  }
}
