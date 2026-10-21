import Foundation
import MLX
import MLXAudioSTT
import XCTest

@testable import VesperMLX

final class ParakeetReduxTests: XCTestCase {
  func testTernaryPlaneEqualsDenseScaledCodes() {
    let (outputs, inputs) = (2, 256)
    let codes: [[UInt32]] = (0 ..< outputs).map { row in
      (0 ..< inputs).map { UInt32(($0 * 7 + row * 3) % 3) }
    }
    let scales: [[Float]] = [[0.5, 0.0125], [0.25, 1.5]]
    let x = (0 ..< inputs).map { Float(($0 % 17) - 8) / 8 }
    let packed = MLXArray(
      codes.flatMap { row in
        stride(from: 0, to: inputs, by: 16).map { start in
          (0 ..< 16).reduce(UInt32(0)) { $0 | (row[start + $1] << (2 * UInt32($1))) }
        }
      }, [outputs, inputs / 16])
    let s = MLXArray(scales.flatMap { $0 }, [outputs, 2])
    let y = TernaryPlane(scales: s, biases: -s).apply(MLXArray(x, [1, inputs]), codes: packed)
      .asArray(Float.self)
    for row in 0 ..< outputs {
      let expected = (0 ..< inputs).reduce(Float(0)) {
        $0 + scales[row][$1 / 128] * (Float(codes[row][$1]) - 1) * x[$1]
      }
      XCTAssertEqual(y[row], expected, accuracy: 1e-3 * max(1, abs(expected)))
    }
  }

  /// Optional whole-model parity. Set VESPER_REDUX_PARITY to a JSON file with
  /// the converted `model` directory and `cases` (`audio` raw f32 PCM, `text`
  /// from mlx-audio's Redux on Moondream's original files).
  func testConvertedModelMatchesReferenceWhenFixtureIsProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["VESPER_REDUX_PARITY"] else {
      throw XCTSkip("VESPER_REDUX_PARITY is not set")
    }
    let fixture = try XCTUnwrap(
      JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))
        as? [String: Any])
    let model = try ParakeetRedux.fromDirectory(
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
