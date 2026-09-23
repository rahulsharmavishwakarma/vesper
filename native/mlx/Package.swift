// swift-tools-version:6.2
import PackageDescription

let package = Package(
  name: "TranscribeMLX",
  platforms: [.macOS(.v14)],
  products: [
    .library(name: "TranscribeMLX", type: .dynamic, targets: ["TranscribeMLX"]),
    .executable(name: "prepare-qwen-tokenizer", targets: ["PrepareTokenizer"]),
  ],
  dependencies: [
    .package(
      url: "https://github.com/Blaizzy/mlx-audio-swift.git",
      revision: "d302a5c6080d2bb97bae38c7418f82abb76013b6"),
    .package(url: "https://github.com/ml-explore/mlx-swift.git", exact: "0.31.4"),
    .package(url: "https://github.com/ml-explore/mlx-swift-lm.git", exact: "3.31.4"),
  ],
  targets: [
    .testTarget(
      name: "TranscribeMLXTests",
      dependencies: [
        .product(name: "MLXAudioSTT", package: "mlx-audio-swift"),
        .product(name: "MLX", package: "mlx-swift"),
      ]),
    .executableTarget(
      name: "PrepareTokenizer",
      dependencies: [
        .product(name: "MLXAudioSTT", package: "mlx-audio-swift")
      ]),
    .target(
      name: "TranscribeMLX",
      dependencies: [
        .product(name: "MLXAudioSTT", package: "mlx-audio-swift"),
        .product(name: "MLX", package: "mlx-swift"),
      ]),
  ]
)
