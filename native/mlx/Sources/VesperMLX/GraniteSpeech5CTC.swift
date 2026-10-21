import Foundation
import MLX
import MLXAudioSTT
import MLXFFT
import MLXNN

// Granite Speech 5.0 TurboCTC (ibm-granite/granite-speech-5.0-470m-turboctc).
// Mirrors transformers v5.16.1 `models/granite_speech5` and the mlx-audio
// 0.6.1 reference: a non-autoregressive Conformer encoder with 128-frame block
// attention, Shaw relative positions, two stride-2 convolution layers,
// self-conditioning at the middle layer and a greedy CTC head tied to it.

struct GraniteSpeech5Config: Decodable {
  struct Encoder: Decodable {
    let hiddenSize: Int
    let intermediateSize: Int
    let numHiddenLayers: Int
    let numAttentionHeads: Int
    let headDim: Int
    let numMelBins: Int
    let maxPositionEmbeddings: Int
    let contextSize: Int
    let convKernelSize: Int
    let convExpansionFactor: Int
    let subsampleLayers: [Int]
    let attentionBias: Bool
    let vocabSize: Int
  }
  let modelType: String
  let vocabSize: Int
  let padTokenId: Int
  let encoderConfig: Encoder
}

/// Inference-only BatchNorm over the channel axis, evaluated in float32.
private final class GraniteEvalBatchNorm: Module {
  var weight: MLXArray
  var bias: MLXArray
  @ParameterInfo(key: "running_mean") var runningMean: MLXArray
  @ParameterInfo(key: "running_var") var runningVar: MLXArray

  init(_ features: Int) {
    weight = MLXArray.ones([features])
    bias = MLXArray.zeros([features])
    _runningMean.wrappedValue = MLXArray.zeros([features])
    _runningVar.wrappedValue = MLXArray.ones([features])
  }

  func callAsFunction(_ x: MLXArray) -> MLXArray {
    let normalized =
      (x.asType(.float32) - runningMean.asType(.float32)) * rsqrt(runningVar.asType(.float32) + 1e-5)
    return (normalized * weight.asType(.float32) + bias.asType(.float32)).asType(x.dtype)
  }
}

private final class GraniteFeedForward: Module {
  @ModuleInfo var linear1: Linear
  @ModuleInfo var linear2: Linear

  init(_ config: GraniteSpeech5Config.Encoder) {
    _linear1.wrappedValue = Linear(
      config.hiddenSize, config.intermediateSize, bias: config.attentionBias)
    _linear2.wrappedValue = Linear(
      config.intermediateSize, config.hiddenSize, bias: config.attentionBias)
  }

  func callAsFunction(_ x: MLXArray) -> MLXArray { linear2(silu(linear1(x))) }
}

/// Attention restricted to non-overlapping `contextSize` blocks with Shaw
/// relative-position logits. Padded keys of the final block are masked.
private final class GraniteBlockAttention: Module {
  let context: Int
  let heads: Int
  let headDim: Int
  let scale: Float
  @ModuleInfo(key: "q_proj") var qProj: Linear
  @ModuleInfo(key: "k_proj") var kProj: Linear
  @ModuleInfo(key: "v_proj") var vProj: Linear
  @ModuleInfo(key: "o_proj") var oProj: Linear
  @ModuleInfo(key: "rel_pos_emb") var relPosEmb: Embedding

  init(_ config: GraniteSpeech5Config.Encoder) {
    context = config.contextSize
    heads = config.numAttentionHeads
    headDim = config.headDim
    scale = pow(Float(config.headDim), -0.5)
    let inner = heads * headDim
    _qProj.wrappedValue = Linear(config.hiddenSize, inner, bias: false)
    _kProj.wrappedValue = Linear(config.hiddenSize, inner, bias: false)
    _vProj.wrappedValue = Linear(config.hiddenSize, inner, bias: false)
    _oProj.wrappedValue = Linear(inner, config.hiddenSize, bias: true)
    _relPosEmb.wrappedValue = Embedding(
      embeddingCount: 2 * config.maxPositionEmbeddings + 1, dimensions: config.headDim)
  }

  func callAsFunction(_ x: MLXArray, distances: MLXArray) -> MLXArray {
    let (batch, length, hidden) = (x.dim(0), x.dim(1), x.dim(2))
    let padding = (context - length % context) % context
    let input = padding > 0 ? padded(x, widths: [.init((0, 0)), .init((0, padding)), .init((0, 0))]) : x
    let blocks = input.dim(1) / context
    func split(_ projection: MLXArray) -> MLXArray {
      projection.reshaped(batch, blocks, context, heads, headDim).transposed(0, 1, 3, 2, 4)
    }
    let query = split(qProj(input))
    let key = split(kProj(input))
    let value = split(vProj(input))
    let relative = relPosEmb(distances) * scale
    let positionBias = einsum("bmhcd,crd->bmhcr", query, relative)
    var logits = (matmul(query, key.transposed(0, 1, 2, 4, 3)) * scale + positionBias)
      .asType(.float32)
    if padding > 0 {
      let valid = arange(blocks * context, dtype: .int32) .< MLXArray(Int32(length))
      logits = MLX.where(
        valid.reshaped(1, blocks, 1, 1, context), logits, MLXArray(-Float.infinity))
    }
    let probabilities = softmax(logits, axis: -1).asType(query.dtype)
    let output = matmul(probabilities, value).transposed(0, 1, 3, 2, 4)
      .reshaped(batch, blocks * context, hidden)
    return oProj(output[0..., ..<length])
  }
}

private final class GraniteConvolution: Module {
  let padding: Int
  @ModuleInfo(key: "pointwise_lin1") var pointwiseLin1: Linear
  @ModuleInfo(key: "depthwise_conv") var depthwiseConv: Conv1d
  @ModuleInfo var norm: GraniteEvalBatchNorm
  @ModuleInfo(key: "pointwise_lin2") var pointwiseLin2: Linear

  init(_ config: GraniteSpeech5Config.Encoder, stride: Int) {
    let inner = config.hiddenSize * config.convExpansionFactor
    padding = (config.convKernelSize - 1) / 2
    _pointwiseLin1.wrappedValue = Linear(config.hiddenSize, inner * 2)
    _depthwiseConv.wrappedValue = Conv1d(
      inputChannels: inner, outputChannels: inner, kernelSize: config.convKernelSize,
      stride: stride, groups: inner, bias: false)
    _norm.wrappedValue = GraniteEvalBatchNorm(inner)
    _pointwiseLin2.wrappedValue = Linear(inner, config.hiddenSize)
  }

  func callAsFunction(_ x: MLXArray) -> MLXArray {
    let parts = MLX.split(pointwiseLin1(x), parts: 2, axis: -1)
    let gated = parts[0] * sigmoid(parts[1])
    let convolved = depthwiseConv(padded(gated, widths: [.init((0, 0)), .init((padding, padding)), .init((0, 0))]))
    return pointwiseLin2(silu(norm(convolved)))
  }
}

private final class GraniteConformerBlock: Module {
  let subsample: Bool
  @ModuleInfo(key: "feed_forward1") var feedForward1: GraniteFeedForward
  @ModuleInfo(key: "self_attn") var selfAttn: GraniteBlockAttention
  @ModuleInfo var conv: GraniteConvolution
  @ModuleInfo(key: "feed_forward2") var feedForward2: GraniteFeedForward
  @ModuleInfo(key: "norm_feed_forward1") var normFeedForward1: LayerNorm
  @ModuleInfo(key: "norm_self_att") var normSelfAtt: LayerNorm
  @ModuleInfo(key: "norm_conv") var normConv: LayerNorm
  @ModuleInfo(key: "norm_feed_forward2") var normFeedForward2: LayerNorm
  @ModuleInfo(key: "norm_out") var normOut: LayerNorm

  init(_ config: GraniteSpeech5Config.Encoder, subsample: Bool) {
    self.subsample = subsample
    _feedForward1.wrappedValue = GraniteFeedForward(config)
    _selfAttn.wrappedValue = GraniteBlockAttention(config)
    _conv.wrappedValue = GraniteConvolution(config, stride: subsample ? 2 : 1)
    _feedForward2.wrappedValue = GraniteFeedForward(config)
    _normFeedForward1.wrappedValue = LayerNorm(dimensions: config.hiddenSize)
    _normSelfAtt.wrappedValue = LayerNorm(dimensions: config.hiddenSize)
    _normConv.wrappedValue = LayerNorm(dimensions: config.hiddenSize)
    _normFeedForward2.wrappedValue = LayerNorm(dimensions: config.hiddenSize)
    _normOut.wrappedValue = LayerNorm(dimensions: config.hiddenSize)
  }

  func callAsFunction(_ x: MLXArray, distances: MLXArray) -> MLXArray {
    var hidden = x + 0.5 * feedForward1(normFeedForward1(x))
    hidden = hidden + selfAttn(normSelfAtt(hidden), distances: distances)
    let convolved = conv(normConv(hidden))
    if subsample {
      // Stride-2 layers average frame pairs for the residual path.
      let (batch, half, width) = (hidden.dim(0), hidden.dim(1) / 2, hidden.dim(2))
      hidden =
        hidden[0..., ..<(2 * half)].reshaped(batch, half, 2, width).mean(axis: 2)
        + convolved[0..., ..<half]
    } else {
      hidden = hidden + convolved
    }
    hidden = hidden + 0.5 * feedForward2(normFeedForward2(hidden))
    return normOut(hidden)
  }
}

private final class GraniteEncoder: Module {
  let config: GraniteSpeech5Config.Encoder
  @ModuleInfo(key: "input_linear") var inputLinear: Linear
  @ModuleInfo var layers: [GraniteConformerBlock]
  @ModuleInfo var out: Linear
  @ModuleInfo(key: "out_mid") var outMid: Linear

  init(_ config: GraniteSpeech5Config.Encoder) {
    self.config = config
    _inputLinear.wrappedValue = Linear(config.numMelBins * 4, config.hiddenSize)
    _layers.wrappedValue = (0 ..< config.numHiddenLayers).map {
      GraniteConformerBlock(config, subsample: config.subsampleLayers.contains($0))
    }
    _out.wrappedValue = Linear(config.hiddenSize, config.vocabSize)
    _outMid.wrappedValue = Linear(config.vocabSize, config.hiddenSize)
  }

  func callAsFunction(_ features: MLXArray) -> MLXArray {
    // Clipped relative distances index the shared Shaw embedding table.
    let positions = arange(config.contextSize, dtype: .int32)
    let distances =
      clip(
        positions.expandedDimensions(axis: 1) - positions.expandedDimensions(axis: 0),
        min: MLXArray(Int32(-config.contextSize)), max: MLXArray(Int32(config.contextSize)))
      + MLXArray(Int32(config.maxPositionEmbeddings))
    var hidden = inputLinear(features.asType(inputLinear.weight.dtype))
    for (index, layer) in layers.enumerated() {
      hidden = layer(hidden, distances: distances)
      if index + 1 == config.numHiddenLayers / 2 {
        // Self-conditioning: feed intermediate CTC posteriors back in.
        let posteriors = softmax(out(hidden).asType(.float32), axis: -1).asType(hidden.dtype)
        hidden = hidden + outMid(posteriors)
      }
    }
    return hidden
  }
}

/// Decoder for the `tokenizers` BPE vocabulary: `▁` becomes a space, `<0xNN>`
/// pieces are byte fallback, special tokens are skipped, and one leading space
/// is stripped before trimming, as in the published decoder sequence.
struct GraniteSpeech5Detokenizer {
  let pieces: [String]
  let special: Set<Int>

  init(tokenizerJSON url: URL) throws {
    guard let object = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any],
      let model = object["model"] as? [String: Any], model["type"] as? String == "BPE",
      model["byte_fallback"] as? Bool == true, let vocabulary = model["vocab"] as? [String: Int]
    else { throw fail("Unsupported Granite tokenizer") }
    var pieces = [String](repeating: "", count: vocabulary.count)
    for (piece, id) in vocabulary {
      guard pieces.indices.contains(id), pieces[id].isEmpty else {
        throw fail("Invalid Granite tokenizer vocabulary")
      }
      pieces[id] = piece
    }
    self.pieces = pieces
    special = Set(
      ((object["added_tokens"] as? [[String: Any]]) ?? [])
        .filter { $0["special"] as? Bool == true }.compactMap { $0["id"] as? Int })
  }

  func decode(_ ids: [Int]) -> String {
    var text = ""
    var bytes: [UInt8] = []
    func flush() {
      guard !bytes.isEmpty else { return }
      if let decoded = String(bytes: bytes, encoding: .utf8) {
        text += decoded
      } else {
        text += String(repeating: "\u{FFFD}", count: bytes.count)
      }
      bytes.removeAll()
    }
    for id in ids where pieces.indices.contains(id) && !special.contains(id) {
      let piece = pieces[id]
      if piece.count == 6, piece.hasPrefix("<0x"), piece.hasSuffix(">"),
        let byte = UInt8(piece.dropFirst(3).prefix(2), radix: 16)
      {
        bytes.append(byte)
      } else {
        flush()
        text += piece.replacingOccurrences(of: "\u{2581}", with: " ")
      }
    }
    flush()
    if text.hasPrefix(" ") { text.removeFirst() }
    return text.trimmingCharacters(in: .whitespacesAndNewlines)
  }
}

/// Feature extractor: periodic Hann(400) centred in a 512-point FFT, 10 ms hop,
/// 80 HTK mel bins without area normalization, log10 with a dynamic 8-decade
/// floor, replicate-padded deltas and two-frame stacking (320 features).
enum GraniteSpeech5Frontend {
  static let fft = 512
  static let hop = 160

  static let window: [Float] = {
    let length = 400
    let left = (fft - length) / 2
    var values = [Float](repeating: 0, count: fft)
    for n in 0 ..< length {
      values[left + n] = Float(0.5 * (1 - cos(2 * Double.pi * Double(n) / Double(length))))
    }
    return values
  }()

  /// Triangular HTK filters computed in float64, shaped [257, 80].
  static let melFilters: [Float] = {
    let bins = fft / 2 + 1
    let mels = 80
    let toMel = { (hz: Double) in 2595 * log10(1 + hz / 700) }
    let maximum = toMel(8_000)
    let points = (0 ..< mels + 2).map {
      700 * (pow(10, maximum * Double($0) / Double(mels + 1) / 2595) - 1)
    }
    var values = [Float](repeating: 0, count: bins * mels)
    for bin in 0 ..< bins {
      let frequency = 8_000 * Double(bin) / Double(bins - 1)
      for mel in 0 ..< mels {
        let down = (frequency - points[mel]) / (points[mel + 1] - points[mel])
        let up = (points[mel + 2] - frequency) / (points[mel + 2] - points[mel + 1])
        values[bin * mels + mel] = Float(max(0, min(down, up)))
      }
    }
    return values
  }()

  static func features(_ samples: [Float]) throws -> MLXArray {
    let melFrames = samples.count / hop
    let frames = 2 * ((melFrames + 1) / 2)
    guard frames > 0, samples.count > fft / 2 else { throw fail("Audio is too short") }
    var audio = samples
    let needed = (frames - 1) * hop + 1
    if audio.count < needed { audio += [Float](repeating: 0, count: needed - audio.count) }
    // Centred STFT with reflect padding (numpy/torch `reflect`, edge excluded).
    let half = fft / 2
    let prefix = audio[1 ... half].reversed()
    let suffix = audio[(audio.count - half - 1) ..< (audio.count - 1)].reversed()
    let paddedAudio = MLXArray(Array(prefix) + audio + Array(suffix))
    let framed = asStrided(paddedAudio, [frames, fft], strides: [hop, 1])
    let power = square(abs(MLXFFT.rfft(framed * MLXArray(window), axis: 1)))
    var logMel = log10(maximum(matmul(power, MLXArray(melFilters, [fft / 2 + 1, 80])), MLXArray(Float(1e-10))))
    logMel = maximum(logMel, logMel.max() - 8) / 4 + 1
    let previous = concatenated([logMel[..<1], logMel[..<(frames - 1)]], axis: 0)
    let next = concatenated([logMel[1...], logMel[(frames - 1)...]], axis: 0)
    let stacked = concatenated([logMel, (next - previous) / 2], axis: -1)
    return stacked.reshaped(1, frames / 2, 2 * stacked.dim(-1))
  }
}

final class GraniteSpeech5CTCModel: Module, STTGenerationModel {
  let blank: Int
  let detokenizer: GraniteSpeech5Detokenizer
  @ModuleInfo fileprivate var encoder: GraniteEncoder

  var defaultGenerationParameters: STTGenerateParameters {
    STTGenerateParameters(maxTokens: 0, temperature: 0, topP: 1, topK: 0, language: nil)
  }

  private init(_ config: GraniteSpeech5Config, detokenizer: GraniteSpeech5Detokenizer) {
    blank = config.padTokenId
    self.detokenizer = detokenizer
    _encoder.wrappedValue = GraniteEncoder(config.encoderConfig)
  }

  static func fromDirectory(_ directory: URL) throws -> GraniteSpeech5CTCModel {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let config = try decoder.decode(
      GraniteSpeech5Config.self, from: Data(contentsOf: directory.appendingPathComponent("config.json")))
    guard config.modelType == "granite_speech5_ctc", config.vocabSize == config.encoderConfig.vocabSize,
      config.encoderConfig.numMelBins == 80
    else { throw fail("Unsupported Granite Speech 5 configuration") }
    let detokenizer = try GraniteSpeech5Detokenizer(
      tokenizerJSON: directory.appendingPathComponent("tokenizer.json"))
    guard detokenizer.pieces.count == config.vocabSize else {
      throw fail("Granite tokenizer and model vocabulary differ")
    }
    let model = GraniteSpeech5CTCModel(config, detokenizer: detokenizer)
    var weights: [String: MLXArray] = [:]
    for (key, value) in try MLX.loadArrays(url: directory.appendingPathComponent("model.safetensors"))
    where !key.hasSuffix("num_batches_tracked") {
      // PyTorch depthwise Conv1d [out, 1, kernel] -> MLX [out, kernel, 1].
      weights[key] = key.hasSuffix("depthwise_conv.weight") ? value.transposed(0, 2, 1) : value
    }
    try model.update(parameters: ModuleParameters.unflattened(weights), verify: .all)
    model.train(false)
    eval(model)
    return model
  }

  func logits(_ samples: [Float]) throws -> MLXArray {
    encoder.out(encoder(try GraniteSpeech5Frontend.features(samples)))
  }

  func generate(audio: MLXArray, generationParameters: STTGenerateParameters) -> STTOutput {
    let start = CFAbsoluteTimeGetCurrent()
    let samples = audio.reshaped(-1).asType(.float32).asArray(Float.self)
    guard let logits = try? logits(samples) else {
      return STTOutput(text: "", vesperCompletionReason: "error")
    }
    let best = logits[0].argMax(axis: -1).asType(.int32)
    eval(best)
    var tokens: [Int] = []
    var previous = -1
    for value in best.asArray(Int32.self).map(Int.init) {
      if value != previous && value != blank { tokens.append(value) }
      previous = value
    }
    let text = detokenizer.decode(tokens)
    let elapsed = CFAbsoluteTimeGetCurrent() - start
    return STTOutput(
      text: text, language: generationParameters.language, generationTokens: tokens.count,
      totalTokens: tokens.count, generationTps: Double(tokens.count) / max(elapsed, 0.001),
      totalTime: elapsed, vesperCompletionReason: "audio_consumed")
  }

  func generateStream(audio: MLXArray, generationParameters: STTGenerateParameters)
    -> AsyncThrowingStream<STTGeneration, Error>
  {
    AsyncThrowingStream { continuation in
      continuation.yield(.result(generate(audio: audio, generationParameters: generationParameters)))
      continuation.finish()
    }
  }
}
