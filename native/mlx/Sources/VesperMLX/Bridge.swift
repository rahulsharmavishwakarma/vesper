import Foundation
import MLX
import MLXAudioSTT
import Metal

// C ABI ownership: the Rust inference worker serializes calls and keeps the
// library alive until the last model/string is released. No download APIs.
@_cdecl("vesper_mlx_abi_version")
public func abiVersion() -> UInt32 { 1 }

// Read-only availability probe; does not construct a speech model.
@_cdecl("vesper_mlx_device_available")
public func deviceAvailable() -> Bool { MTLCreateSystemDefaultDevice() != nil }

@_cdecl("vesper_mlx_free_string")
public func freeString(_ pointer: UnsafeMutablePointer<CChar>?) { free(pointer) }

final class ModelHandle: @unchecked Sendable {
  let model: any STTGenerationModel
  let kind: String
  init(_ model: any STTGenerationModel, kind: String) {
    self.model = model
    self.kind = kind
  }
}

final class ResultBox<T>: @unchecked Sendable {
  var result: Result<T, Error>?
  let ready = DispatchSemaphore(value: 0)
}

typealias CancelCallback = @convention(c) (UnsafeMutableRawPointer?) -> Bool

func blocking<T>(
  cancel: (() -> Bool)? = nil,
  _ operation: @escaping @Sendable () async throws -> T
) throws -> T {
  let box = ResultBox<T>()
  let task = Task.detached {
    do { box.result = .success(try await operation()) } catch { box.result = .failure(error) }
    box.ready.signal()
  }
  while box.ready.wait(timeout: .now() + .milliseconds(10)) == .timedOut {
    if cancel?() == true { task.cancel() }
  }
  return try box.result!.get()
}

func fail(_ message: String) -> NSError {
  NSError(domain: "VesperMLX", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
}

func jsonString(_ object: [String: Any]) -> UnsafeMutablePointer<CChar>? {
  guard let bytes = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]),
    let string = String(data: bytes, encoding: .utf8)
  else { return nil }
  return strdup(string)
}

@_cdecl("vesper_mlx_load")
public func loadModel(
  _ path: UnsafePointer<CChar>?, _ architecture: UnsafePointer<CChar>?,
  _ errorOut: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> UnsafeMutableRawPointer? {
  do {
    guard let path, let architecture else { throw fail("Missing model path or architecture") }
    guard let device = MTLCreateSystemDefaultDevice() else { throw fail("No usable Metal device") }
    // MLX allocator limits reserve unified memory for capture, the writer and OS.
    // They do not impose a hard cross-runtime process cap or change wired memory.
    let workingSet = Int(device.recommendedMaxWorkingSetSize)
    Memory.cacheLimit = min(256 * 1024 * 1024, workingSet / 16)
    Memory.memoryLimit = max(256 * 1024 * 1024, workingSet * 3 / 4)
    let directory = URL(fileURLWithPath: String(cString: path), isDirectory: true)
    let kind = String(cString: architecture)
    let required =
      ["config.json"]
      + (kind == "cohere"
        ? ["tokenizer.model", "tokenizer_config.json"]
        : kind == "parakeet" ? [] : ["tokenizer.json", "tokenizer_config.json"])
    for name in required
    where !FileManager.default.fileExists(atPath: directory.appendingPathComponent(name).path) {
      throw fail("Missing verified local file: \(name)")
    }
    let handle = try blocking {
      try await Device.withDefaultDevice(.gpu) {
        let model: any STTGenerationModel
        switch kind {
        case "cohere": model = try CohereTranscribeModel.fromDirectory(directory)
        case "whisper": model = try await WhisperModel.fromDirectory(directory)
        case "qwen3_asr": model = try await Qwen3ASRModel.fromModelDirectory(directory)
        case "parakeet": model = try ParakeetModel.fromDirectory(directory)
        case "granite_speech5_ctc": model = try GraniteSpeech5CTCModel.fromDirectory(directory)
        default: throw fail("Unsupported compiled speech architecture")
        }
        return ModelHandle(model, kind: kind)
      }
    }
    return Unmanaged.passRetained(handle).toOpaque()
  } catch {
    errorOut?.pointee = strdup(error.localizedDescription)
    return nil
  }
}

@_cdecl("vesper_mlx_unload")
public func unloadModel(_ handle: UnsafeMutableRawPointer?) {
  if let handle { Unmanaged<ModelHandle>.fromOpaque(handle).release() }
}

@_cdecl("vesper_mlx_transcribe")
public func transcribe(
  _ opaque: UnsafeMutableRawPointer?, _ samples: UnsafePointer<Float>?,
  _ count: Int64, _ options: UnsafePointer<CChar>?,
  _ cancelled: (@convention(c) (UnsafeMutableRawPointer?) -> Bool)?,
  _ cancelContext: UnsafeMutableRawPointer?
) -> UnsafeMutablePointer<CChar>? {
  do {
    guard let opaque, let samples, let options, count > 0, count <= 480_000 else {
      throw fail("Invalid bounded 16 kHz audio input")
    }
    if cancelled?(cancelContext) == true { throw fail("cancelled") }
    let handle = Unmanaged<ModelHandle>.fromOpaque(opaque).takeUnretainedValue()
    let object = try JSONSerialization.jsonObject(with: Data(String(cString: options).utf8))
    guard let config = object as? [String: Any], let budget = config["maxTokens"] as? Int,
      budget > 0, budget <= 512
    else { throw fail("Invalid generation budget") }
    let language = config["language"] as? String
    if ["cohere", "whisper"].contains(handle.kind),
      !["en", "fr"].contains(language ?? "")
    {
      throw fail("This MLX path requires an explicit supported language (en or fr)")
    }
    // One native chunk: Rust owns segmentation and complete-audio coverage.
    var audio = Array(UnsafeBufferPointer(start: samples, count: Int(count)))
    guard audio.allSatisfy({ $0.isFinite }) else { throw fail("Non-finite audio") }
    // Reflect-padding STFTs require enough source samples. Only append zeros;
    // never remove quiet audio or interpret padding as additional coverage.
    if audio.count < 16_000 {
      audio.append(contentsOf: repeatElement(0, count: 16_000 - audio.count))
    }
    let boundedAudio = audio
    let start = ContinuousClock.now
    let result = try blocking(
      cancel: { cancelled?(cancelContext) == true },
      {
        Device.withDefaultDevice(.gpu) {
          let output = handle.model.generate(
            audio: MLXArray(boundedAudio),
            generationParameters:
              STTGenerateParameters(
                maxTokens: budget, temperature: 0, verbose: false,
                language: language, chunkDuration: 30, minChunkDuration: 0))
          return output
        }
      })
    if cancelled?(cancelContext) == true { throw fail("cancelled") }
    let elapsed = start.duration(to: .now).components
    var response: [String: Any] = [
      "schemaVersion": 1, "text": result.text,
      "stopReason": result.vesperCompletionReason ?? "unverified",
      "generationTokens": result.generationTokens, "promptTokens": result.promptTokens,
      "nativeSeconds": Double(elapsed.seconds) + Double(elapsed.attoseconds) / 1e18,
      "peakMemoryGB": Double(Memory.peakMemory) / 1e9,
      "resources": ["activeBytes": Memory.activeMemory, "cacheBytes": Memory.cacheMemory,
                    "cacheLimitBytes": Memory.cacheLimit, "memoryLimitBytes": Memory.memoryLimit,
                    "scope": "MLX allocator; shared unified memory"],
      "provider": "metal", "runtimeRevision": "d302a5c6080d2bb97bae38c7418f82abb76013b6",
    ]
    if let scores = result.vesperWhisperScores { response["whisperScores"] = scores }
    return jsonString(response)
  } catch { return jsonString(["schemaVersion": 1, "error": error.localizedDescription]) }
}
