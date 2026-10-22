import Foundation
import MLX
import XCTest

@testable import MLXAudioSTT
@testable import VesperMLX

final class ParakeetBlockedDecodeTests: XCTestCase {
  /// Optional equivalence check for the pinned blocked TDT loop. Set
  /// VESPER_TDT_EQUIVALENCE to a JSON file with `models` ({kind, directory};
  /// kind is parakeet, phonon2 or parakeet_redux) and `audio` (raw f32 PCM
  /// paths). Every decode step must match the serial loop exactly.
  func testBlockedLoopReproducesTheSerialTraceWhenFixtureIsProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["VESPER_TDT_EQUIVALENCE"] else {
      throw XCTSkip("VESPER_TDT_EQUIVALENCE is not set")
    }
    let fixture = try XCTUnwrap(
      JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path)))
        as? [String: Any])
    let clips = try XCTUnwrap(fixture["audio"] as? [String]).map { file in
      try Data(contentsOf: URL(fileURLWithPath: file)).withUnsafeBytes {
        Array($0.bindMemory(to: Float.self))
      }
    }
    let parameters = STTGenerateParameters(
      maxTokens: 512, temperature: 0, verbose: false, language: nil, chunkDuration: 30,
      minChunkDuration: 0)
    final class Trace: @unchecked Sendable { var steps: [ParakeetModel.TDTTraceStep] = [] }
    for entry in try XCTUnwrap(fixture["models"] as? [[String: String]]) {
      let directory = URL(fileURLWithPath: try XCTUnwrap(entry["directory"]), isDirectory: true)
      let model: ParakeetModel =
        switch entry["kind"] {
        case "phonon2": try Phonon2.fromDirectory(directory)
        case "parakeet_redux": try ParakeetRedux.fromDirectory(directory)
        default: try ParakeetModel.fromDirectory(directory)
        }
      for (index, clip) in clips.enumerated() {
        var traces: [[ParakeetModel.TDTTraceStep]] = []
        var texts: [String] = []
        for implementation in [ParakeetModel.TDTDecoderImplementation.serial, .blocked] {
          let trace = Trace()
          model.tdtDecoderImplementation = implementation
          model.tdtTraceEmitter = { trace.steps.append($0) }
          texts.append(model.generate(audio: MLXArray(clip), generationParameters: parameters).text)
          traces.append(trace.steps)
        }
        model.tdtTraceEmitter = nil
        XCTAssertFalse(traces[0].isEmpty, "\(entry["kind"] ?? "") clip \(index)")
        XCTAssertEqual(traces[0], traces[1], "\(entry["kind"] ?? "") clip \(index)")
        XCTAssertEqual(texts[0], texts[1], "\(entry["kind"] ?? "") clip \(index)")
      }
    }
  }
}
