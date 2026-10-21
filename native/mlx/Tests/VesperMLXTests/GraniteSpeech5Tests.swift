import Foundation
import MLX
import XCTest

@testable import VesperMLX

final class GraniteSpeech5Tests: XCTestCase {
  // Deterministic tones plus LCG noise with a quiet first quarter. 21,937
  // samples exercise odd mel-frame padding and a partial attention block.
  private func signal(_ count: Int) -> [Float] {
    var state: UInt64 = 12345
    return (0 ..< count).map { n in
      state = (1_103_515_245 * state + 12345) % (1 << 31)
      let t = Double(n) / 16_000
      var x = 0.3 * sin(2 * .pi * 440 * t) + 0.2 * sin(2 * .pi * 1234.5 * t + 0.5)
      x += 0.05 * (Double(state) / Double(1 << 31) - 0.5)
      return Float(n < count / 4 ? x * 0.01 : x)
    }
  }

  func testFrontendMatchesReferenceFeatureExtractor() throws {
    // Expected values from a float64 NumPy evaluation of the published feature
    // extractor (periodic Hann, centred reflect STFT, HTK mel, log10 floor,
    // replicate deltas, stacking). The MLX Python reference differs by up to
    // 1.6e-4 with a -4e-5 mean bias; this port stays within 5.2e-5.
    let features = try GraniteSpeech5Frontend.features(signal(21_937))[0].asType(.float32)
    XCTAssertEqual(features.shape, [69, 320])
    let values = features.asArray(Float.self)
    XCTAssertEqual(values.reduce(0) { $0 + Double($1) }, 6441.405, accuracy: 0.05)
    let expected: [(Int, Int, Float)] = [
      (0, 0, 0.254212), (0, 79, -0.252689), (0, 80, -0.261658), (0, 159, 0.041271),
      (0, 160, -0.269104), (0, 319, 0.102750), (5, 17, 0.234413), (33, 200, 0.717510),
      (68, 3, 0.597478), (68, 319, 0.002396), (40, 100, 0.074001), (20, 250, 0.022164),
    ]
    for (row, column, value) in expected {
      XCTAssertEqual(values[row * 320 + column], value, accuracy: 1e-4, "(\(row), \(column))")
    }
  }

  func testLevelingOnlyBoostsQuietInputWithoutClipping() {
    let quiet = signal(16_000).map { $0 * 0.03 }
    let boosted = GraniteSpeech5Frontend.leveled(quiet)
    let rms = { (x: [Float]) in (x.reduce(0) { $0 + Double($1 * $1) } / Double(x.count)).squareRoot() }
    XCTAssertEqual(20 * log10(rms(boosted)), -23, accuracy: 0.01)
    XCTAssertLessThanOrEqual(boosted.map(abs).max()!, 0.9)
    // A sparse click is limited by the peak cap, not the RMS target.
    var click = [Float](repeating: 0, count: 16_000)
    click[100] = 0.01
    XCTAssertEqual(GraniteSpeech5Frontend.leveled(click).map(abs).max()!, 0.9, accuracy: 1e-6)
    let loud = signal(16_000)
    XCTAssertEqual(GraniteSpeech5Frontend.leveled(loud), loud)
    XCTAssertEqual(GraniteSpeech5Frontend.leveled([0, 0, 0]), [0, 0, 0])
  }

  func testDetokenizerFollowsPublishedDecoderSequence() throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    let pieces = ["<unk>", "<|en|>", "\u{2581}Hello", ",", "\u{2581}wor", "ld", "<0xE2>", "<0x82>",
      "<0xAC>", "<0xFF>", "\u{2581}5", "\u{2581}"]
    let vocabulary = Dictionary(uniqueKeysWithValues: pieces.enumerated().map { ($1, $0) })
    let tokenizer: [String: Any] = [
      "added_tokens": [["id": 0, "special": true], ["id": 1, "special": true]],
      "model": ["type": "BPE", "byte_fallback": true, "vocab": vocabulary],
    ]
    let url = directory.appendingPathComponent("tokenizer.json")
    try JSONSerialization.data(withJSONObject: tokenizer).write(to: url)
    let detokenizer = try GraniteSpeech5Detokenizer(tokenizerJSON: url)
    XCTAssertEqual(detokenizer.decode([1, 2, 3, 0, 4, 5]), "Hello, world")
    XCTAssertEqual(detokenizer.decode([10, 6, 7, 8]), "5€")
    XCTAssertEqual(detokenizer.decode([10, 9, 11]), "5\u{FFFD}")
    XCTAssertEqual(detokenizer.decode([1, 0]), "")
  }

  /// Optional whole-model parity. Set VESPER_GRANITE_PARITY to a JSON file with
  /// `model`, `audio` (raw little-endian f32 PCM), reference `text` and `argmax`.
  func testWholeModelMatchesReferenceWhenFixtureIsProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["VESPER_GRANITE_PARITY"] else {
      throw XCTSkip("VESPER_GRANITE_PARITY is not set")
    }
    let fixture = try XCTUnwrap(
      JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))
        as? [String: Any])
    let model = try GraniteSpeech5CTCModel.fromDirectory(
      URL(fileURLWithPath: try XCTUnwrap(fixture["model"] as? String), isDirectory: true))
    let bytes = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(fixture["audio"] as? String)))
    let samples = bytes.withUnsafeBytes { Array($0.bindMemory(to: Float.self)) }
    let expected = try XCTUnwrap(fixture["argmax"] as? [Int])
    let best = try model.logits(samples)[0].argMax(axis: -1).asArray(Int32.self).map(Int.init)
    XCTAssertEqual(best.count, expected.count)
    let agreement = Double(zip(best, expected).filter { $0 == $1 }.count) / Double(expected.count)
    XCTAssertGreaterThanOrEqual(agreement, 0.99)
    // The reference has no level stage; compare it on input leveling leaves unchanged.
    XCTAssertEqual(GraniteSpeech5Frontend.leveled(samples), samples)
    let output = model.generate(audio: MLXArray(samples), generationParameters: model.defaultGenerationParameters)
    XCTAssertEqual(output.text, fixture["text"] as? String)
    XCTAssertEqual(output.vesperCompletionReason, "audio_consumed")
  }
}
