import MLX
import XCTest

@testable import MLXAudioSTT

final class QwenFrontendTests: XCTestCase {
  func testOfficialAudioTokenGeometryAtChunkBoundaries() {
    // Fixed expectations from Qwen's official processor, not floating-point
    // division followed by truncation. The short final encoder chunk matters.
    let frames: [Int32] = [0, 1, 8, 99, 100, 101, 159, 199, 200, 999, 1000, 1001]
    let expected: [Int32] = [0, 1, 1, 13, 13, 14, 21, 26, 26, 130, 130, 131]
    let lengths = getFeatExtractOutputLengths(MLXArray(frames))
    XCTAssertEqual(lengths.dtype, .int32)
    XCTAssertEqual(lengths.asArray(Int32.self), expected)
  }

  func testWhisperFrontendDoesNotPadQwenToThirtySeconds() {
    // Qwen consumes all frames of this ten-second waveform, without adding
    // Whisper's fixed-window padding or an extra final centered STFT frame.
    let features = WhisperAudio.logMelSpectrogram(
      MLXArray.zeros([160_000], type: Float.self), nMels: 128)
    XCTAssertEqual(features.shape, [128, 1000])
    XCTAssertEqual(
      getFeatExtractOutputLengths(MLXArray(Int32(features.dim(1)))).item(Int32.self), 130)
  }
}
