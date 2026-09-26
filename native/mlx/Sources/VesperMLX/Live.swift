import Foundation
import MLXAudioSTT

// Only Qwen's native input session is admitted. Live text is provisional; the
// existing completion-checked full-audio decoder supplies the durable final.
final class LiveHandle: @unchecked Sendable {
  let owner: ModelHandle
  let session: StreamingInferenceSession
  let lock = NSLock()
  var latest: String?
  var samples: Int64 = 0
  var collector: Task<Void, Never>?

  init(_ owner: ModelHandle, model: Qwen3ASRModel) {
    self.owner = owner
    session = StreamingInferenceSession(model: model,
      config: StreamingConfig(maxCachedWindows: 2, language: "English"))
    collector = Task.detached { [self] in
      for await event in session.events {
        if case .displayUpdate(let confirmed, let provisional) = event {
          lock.withLock {
            latest = confirmed + ((!confirmed.isEmpty && !provisional.isEmpty) ? " " : "") + provisional
          }
        }
      }
    }
  }

  func close() {
    _ = try? blocking {
      await self.session.cancelAndWait()
      await self.collector?.value
    }
    collector = nil
  }
}

@_cdecl("vesper_mlx_live_open")
public func liveOpen(_ model: UnsafeMutableRawPointer?,
  _ errorOut: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?) -> UnsafeMutableRawPointer? {
  do {
    guard let model else { throw fail("Missing live model") }
    let owner = Unmanaged<ModelHandle>.fromOpaque(model).takeUnretainedValue()
    guard owner.kind == "qwen3_asr", let qwen = owner.model as? Qwen3ASRModel else {
      throw fail("Native live input requires Qwen3-ASR")
    }
    return Unmanaged.passRetained(LiveHandle(owner, model: qwen)).toOpaque()
  } catch { errorOut?.pointee = strdup(error.localizedDescription); return nil }
}

@_cdecl("vesper_mlx_live_feed")
public func liveFeed(_ opaque: UnsafeMutableRawPointer?, _ samples: UnsafePointer<Float>?,
  _ count: Int64) -> UnsafeMutablePointer<CChar>? {
  do {
    guard let opaque, let samples, count > 0, count <= 16000 else { throw fail("Invalid live PCM packet") }
    let handle = Unmanaged<LiveHandle>.fromOpaque(opaque).takeUnretainedValue()
    let audio = Array(UnsafeBufferPointer(start: samples, count: Int(count)))
    guard audio.allSatisfy({ $0.isFinite }) else { throw fail("Non-finite live PCM") }
    // On overload the caller retains the original recording for its final pass.
    // Never allow an unbounded queue of completed acoustic windows.
    guard handle.session.bufferedWindows <= 2 else { throw fail("Native live decoder backlog exceeded") }
    handle.session.feedAudio(samples: audio)
    handle.samples += count
    guard handle.session.bufferedWindows <= 2 else { throw fail("Native live decoder backlog exceeded") }
    return jsonString(["ok": true])
  } catch { return jsonString(["error": error.localizedDescription]) }
}

@_cdecl("vesper_mlx_live_poll")
public func livePoll(_ opaque: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
  guard let opaque else { return jsonString(["error": "Missing live session"]) }
  let handle = Unmanaged<LiveHandle>.fromOpaque(opaque).takeUnretainedValue()
  let text = handle.lock.withLock { let text = handle.latest; handle.latest = nil; return text }
  return jsonString(["text": text as Any? ?? NSNull(), "inputSeconds": Double(handle.samples) / 16000])
}

@_cdecl("vesper_mlx_live_close")
public func liveClose(_ opaque: UnsafeMutableRawPointer?) {
  guard let opaque else { return }
  let handle = Unmanaged<LiveHandle>.fromOpaque(opaque).takeRetainedValue()
  handle.close()
}
