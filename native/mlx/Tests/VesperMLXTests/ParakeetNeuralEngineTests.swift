import CoreML
import Foundation
import MLX
import MLXAudioSTT
import XCTest

@testable import VesperMLX

final class ParakeetNeuralEngineTests: XCTestCase {
  /// The Neural Engine pads the time axis of [1, channels, T] outputs; the
  /// padding must never reach the decoder.
  func testChannelsLastReadsThroughPaddedStrides() throws {
    let (channels, frames, rowStride) = (3, 4, 6)
    var storage = [Float16](repeating: 99, count: channels * rowStride)
    for channel in 0 ..< channels {
      for time in 0 ..< frames { storage[channel * rowStride + time] = Float16(channel * 10 + time) }
    }
    let encoded = try storage.withUnsafeMutableBufferPointer { buffer in
      try MLMultiArray(
        dataPointer: buffer.baseAddress!, shape: [1, NSNumber(value: channels), NSNumber(value: frames)],
        dataType: .float16, strides: [NSNumber(value: channels * rowStride), NSNumber(value: rowStride), 1])
    }
    let features = try ParakeetNeuralEngineModel.channelsLast(encoded, frames: 3)
    XCTAssertEqual(features.shape, [1, 3, channels])
    XCTAssertEqual(
      features.asType(.float32).asArray(Float.self), [0, 10, 20, 1, 11, 21, 2, 12, 22])
    XCTAssertThrowsError(try ParakeetNeuralEngineModel.channelsLast(encoded, frames: frames + 1))
  }

  /// Optional: every compute unit in the fallback order loads the same
  /// installed files and transcribes alike. Set VESPER_ANE_MODEL to the
  /// installed `neural-engine/parakeet` directory and VESPER_ANE_AUDIO to raw
  /// f32 16 kHz PCM of at most 15 s.
  func testEveryComputeUnitRunsTheInstalledEncoderWhenProvided() throws {
    let environment = ProcessInfo.processInfo.environment
    guard let model = environment["VESPER_ANE_MODEL"], let audio = environment["VESPER_ANE_AUDIO"]
    else { throw XCTSkip("VESPER_ANE_MODEL and VESPER_ANE_AUDIO are not set") }
    let samples = try Data(contentsOf: URL(fileURLWithPath: audio)).withUnsafeBytes {
      Array($0.bindMemory(to: Float.self))
    }
    var texts: [String] = []
    for unit in ParakeetNeuralEngineModel.computeUnitOrder {
      let engine = try ParakeetNeuralEngineModel(
        directory: URL(fileURLWithPath: model, isDirectory: true), order: [unit])
      XCTAssertEqual(engine.computeUnits, unit.1)
      let output = try engine.transcribe(MLXArray(samples))
      XCTAssertEqual(output.vesperCompletionReason, "audio_consumed")
      texts.append(output.text)
    }
    XCTAssertFalse(texts[0].isEmpty)
    XCTAssertEqual(Set(texts).count, 1, "\(texts)")
  }

  /// One cut at the quietest 100 ms within 3 s of the middle, so each part of
  /// a clip up to 21 s fits the 15 s window. The app segments in Rust instead.
  private func parts(_ audio: [Float]) -> [[Float]] {
    guard audio.count > 240_000 else { return [audio] }
    let (window, middle) = (1600, audio.count / 2)
    var best = (energy: Float.infinity, index: middle)
    for start in stride(from: middle - 48_000, to: middle + 48_000, by: 160) {
      let energy = audio[start ..< start + window].reduce(0) { $0 + $1 * $1 }
      if energy < best.energy { best = (energy, start) }
    }
    let cut = best.index + window / 2
    return [Array(audio[..<cut]), Array(audio[cut...])]
  }

  /// Optional comparison with the all-MLX path. Set VESPER_ANE_PARITY to a JSON
  /// file with `model` (converted Neural Engine directory), `parakeet` (MLX
  /// directory), `audio` (raw f32 16 kHz PCM paths, up to 21 s) and `output`
  /// (results JSON path, for scoring outside the test). Optional `baseline`
  /// (`parakeet`, `phonon2` or `parakeet_redux`) selects the MLX model; for
  /// Phonon-2 both sides get its digit-spacing repair.
  func testNeuralEngineModelAgainstMLXWhenFixtureIsProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["VESPER_ANE_PARITY"] else {
      throw XCTSkip("VESPER_ANE_PARITY is not set")
    }
    let fixture = try XCTUnwrap(
      JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))
        as? [String: Any])
    func elapsed(_ start: ContinuousClock.Instant) -> Double {
      let parts = start.duration(to: .now).components
      return Double(parts.seconds) * 1000 + Double(parts.attoseconds) / 1e15
    }
    var start = ContinuousClock.now
    let neuralEngine = try ParakeetNeuralEngineModel(
      directory: URL(fileURLWithPath: try XCTUnwrap(fixture["model"] as? String), isDirectory: true))
    let loadMs = elapsed(start)
    let baselineDirectory = URL(
      fileURLWithPath: try XCTUnwrap(fixture["parakeet"] as? String), isDirectory: true)
    let baseline = fixture["baseline"] as? String ?? "parakeet"
    let parakeet: any STTGenerationModel
    if baseline == "phonon2" {
      parakeet = Phonon2Model(try Phonon2.fromDirectory(baselineDirectory))
    } else if baseline == "parakeet_redux" {
      parakeet = try ParakeetRedux.fromDirectory(baselineDirectory)
    } else {
      parakeet = try ParakeetModel.fromDirectory(baselineDirectory)
    }
    let phonon2 = baseline == "phonon2"
    let repair = { (text: String) -> String in phonon2 ? Phonon2.repairDigitSpacing(text) : text }
    let parameters = parakeet.defaultGenerationParameters
    var rows: [[String: Any]] = []
    for (index, file) in try XCTUnwrap(fixture["audio"] as? [String]).enumerated() {
      let samples = try Data(contentsOf: URL(fileURLWithPath: file)).withUnsafeBytes {
        Array($0.bindMemory(to: Float.self))
      }
      if index == 0 { _ = try neuralEngine.transcribe(MLXArray(Array(samples.prefix(16_000)))) }
      start = ContinuousClock.now
      var texts: [String] = []
      for part in parts(samples) {
        let output = try neuralEngine.transcribe(MLXArray(part))
        XCTAssertEqual(output.vesperCompletionReason, "audio_consumed")
        texts.append(repair(output.text))
      }
      let neuralEngineMs = elapsed(start)
      start = ContinuousClock.now
      let mlx = parts(samples).map {
        parakeet.generate(audio: MLXArray($0), generationParameters: parameters).text
      }
      rows.append([
        "audio": file, "ane": texts.joined(separator: " "), "mlx": mlx.joined(separator: " "),
        "aneMs": neuralEngineMs, "mlxMs": elapsed(start),
      ])
    }
    try JSONSerialization.data(
      withJSONObject: ["loadMs": loadMs, "rows": rows], options: [.prettyPrinted]
    ).write(to: URL(fileURLWithPath: try XCTUnwrap(fixture["output"] as? String)))
  }
}
