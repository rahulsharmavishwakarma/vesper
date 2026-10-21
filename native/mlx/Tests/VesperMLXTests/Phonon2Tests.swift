import Foundation
import MLX
import MLXAudioSTT
import XCTest

@testable import VesperMLX

final class Phonon2Tests: XCTestCase {
  /// Packs 2-bit codes the way MLX affine quantization stores them: sixteen
  /// codes per UInt32, least significant bits first.
  private func pack(_ codes: [[UInt32]]) -> MLXArray {
    let words = codes.map { row in
      stride(from: 0, to: row.count, by: 16).map { start in
        (0 ..< 16).reduce(UInt32(0)) { $0 | (row[start + $1] << (2 * UInt32($1))) }
      }
    }
    return MLXArray(words.flatMap { $0 }, [codes.count, codes[0].count / 16])
  }

  func testPackedPlanesEqualTheDenseFiveValueWeights() {
    let (outputs, inputs) = (3, 256)
    var state: UInt64 = 7
    func next() -> UInt64 {
      state = state &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
      return state >> 33
    }
    let base = (0 ..< outputs).map { _ in (0 ..< inputs).map { _ in UInt32(next() % 3) } }
    let residual = (0 ..< outputs).map { _ in (0 ..< inputs).map { _ in UInt32(next() % 3) } }
    let lo: [Float] = [0.0123, -0.5, 0.25]
    let hi: [Float] = [0.0456, 0.75, -0.125]
    let x = (0 ..< inputs).map { _ in Float(Int(next() % 2001) - 1000) / 1000 }
    // Dense reference: w = lo (q - 1) + (hi - lo) (s - 1), from fp16 palettes.
    let lo16 = lo.map { Float(Float16($0)) }
    let hi16 = hi.map { Float(Float16($0)) }
    let expected = (0 ..< outputs).map { row in
      (0 ..< inputs).reduce(Float(0)) { sum, column in
        let w =
          lo16[row] * (Float(base[row][column]) - 1)
          + (hi16[row] - lo16[row]) * (Float(residual[row][column]) - 1)
        return sum + w * x[column]
      }
    }
    let planes = FiveValuePlanes(
      lo: MLXArray(lo).asType(.float16), hi: MLXArray(hi).asType(.float16), inputs: inputs)
    let y = planes.apply(MLXArray(x, [1, inputs]), base: pack(base), residual: pack(residual))
    let actual = y.asType(.float32).asArray(Float.self)
    for row in 0 ..< outputs {
      XCTAssertEqual(actual[row], expected[row], accuracy: 1e-3 * max(1, abs(expected[row])))
    }
  }

  func testDigitSpacingRepairKeepsAcronymsOrdinalsAndPlurals() {
    let cases = [
      "about70 km": "about 70 km", "MP3 player": "MP3 player", "COVID19": "COVID19",
      "the 4th time": "the 4th time", "in1990s": "in 1990s", "is15 meters": "is 15 meters",
      "10 ,000": "10,000", "Between10 -11 p.m.": "Between 10-11 p.m.", "iPhone15": "iPhone 15",
      "H2O": "H2O", "5km": "5 km", "around800 .": "around 800.", "the 21st and 3rd": "the 21st and 3rd",
      "15states": "15 states", "": "",
    ]
    for (raw, expected) in cases {
      XCTAssertEqual(Phonon2.repairDigitSpacing(raw), expected, raw)
    }
  }

  /// Optional cross-check against the Python rules used for qualification.
  /// Set VESPER_PHONON2_REPAIR_PAIRS to a JSON list of {raw, repaired}.
  func testDigitSpacingRepairMatchesQualificationRulesWhenPairsAreProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["VESPER_PHONON2_REPAIR_PAIRS"] else {
      throw XCTSkip("VESPER_PHONON2_REPAIR_PAIRS is not set")
    }
    let pairs = try XCTUnwrap(
      JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))
        as? [[String: String]])
    for pair in pairs {
      XCTAssertEqual(Phonon2.repairDigitSpacing(pair["raw"]!), pair["repaired"])
    }
  }

  /// Optional whole-model parity. Set VESPER_PHONON2_PARITY to a JSON file
  /// with `model` and `cases` (`audio` as raw little-endian f32 PCM, `text`).
  func testWholeModelMatchesReferenceWhenFixtureIsProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["VESPER_PHONON2_PARITY"] else {
      throw XCTSkip("VESPER_PHONON2_PARITY is not set")
    }
    let fixture = try XCTUnwrap(
      JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))
        as? [String: Any])
    let model = try Phonon2.fromDirectory(
      URL(fileURLWithPath: try XCTUnwrap(fixture["model"] as? String), isDirectory: true))
    for case let item as [String: Any] in try XCTUnwrap(fixture["cases"] as? [Any]) {
      let bytes = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(item["audio"] as? String)))
      let samples = bytes.withUnsafeBytes { Array($0.bindMemory(to: Float.self)) }
      let output = model.generate(
        audio: MLXArray(samples), generationParameters: model.defaultGenerationParameters)
      XCTAssertEqual(
        output.text.trimmingCharacters(in: .whitespacesAndNewlines), item["text"] as? String)
      XCTAssertEqual(output.vesperCompletionReason, "audio_consumed")
    }
  }
}
