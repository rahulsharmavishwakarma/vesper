#!/bin/sh
# Run sequentially after preparation and a release build. Public corpus only; no downloads.
set -eu
PERSONAL_BENCH_ROOT=${1:-.cache/benchmarks/personalization-2026-09-16}
PERSONAL_BENCH_BINARY=./target/release/vesper-eval
PERSONAL_NEMO_ROOT=.cache/benchmarks/model-roots/nemotron-cpu
mkdir -p "$PERSONAL_BENCH_ROOT/results" "$PERSONAL_BENCH_ROOT/logs"
# Calibration is intentionally explicit and separate; preserve an already completed run.
if [ ! -f "$PERSONAL_BENCH_ROOT/results/calibration-nemotron.json" ]; then
  /usr/bin/time -l "$PERSONAL_BENCH_BINARY" run "$PERSONAL_BENCH_ROOT/calibration.json" "$PERSONAL_BENCH_ROOT" "$PERSONAL_NEMO_ROOT" "$PERSONAL_BENCH_ROOT/environment-nemotron.json" "$PERSONAL_BENCH_ROOT/results/calibration-nemotron.json" --model nvidia/nemotron-3.5-asr-streaming-0.6b >"$PERSONAL_BENCH_ROOT/logs/calibration-nemotron.log" 2>&1
fi
"$PERSONAL_BENCH_BINARY" build-memory "$PERSONAL_BENCH_ROOT/calibration.json" "$PERSONAL_BENCH_ROOT/results/calibration-nemotron.json" "$PERSONAL_BENCH_ROOT/memory.json"
run_case() {
  personal_case=$1
  personal_pack=$2
  personal_model=$3
  personal_models=$4
  personal_env=$5
  shift 5
  printf 'Running %s\n' "$personal_case"
  /usr/bin/time -l "$PERSONAL_BENCH_BINARY" run "$PERSONAL_BENCH_ROOT/$personal_pack.json" "$PERSONAL_BENCH_ROOT" "$personal_models" "$PERSONAL_BENCH_ROOT/$personal_env.json" "$PERSONAL_BENCH_ROOT/results/$personal_case.json" --model "$personal_model" --profile "$PERSONAL_BENCH_ROOT/memory.json" "$@" >"$PERSONAL_BENCH_ROOT/logs/$personal_case.log" 2>&1
}
run_case nemotron-holdout holdout nvidia/nemotron-3.5-asr-streaming-0.6b "$PERSONAL_NEMO_ROOT" environment-nemotron
run_case nemotron-hinglish hinglish nvidia/nemotron-3.5-asr-streaming-0.6b "$PERSONAL_NEMO_ROOT" environment-nemotron --language auto
run_case cohere-us us CohereLabs/cohere-transcribe-03-2026 models/downloads environment-cohere --language en
run_case cohere-india india CohereLabs/cohere-transcribe-03-2026 models/downloads environment-cohere --language en
for personal_locale in us india hindi; do
  run_case "nemotron-$personal_locale-realtime" "$personal_locale-realtime" nvidia/nemotron-3.5-asr-streaming-0.6b "$PERSONAL_NEMO_ROOT" environment-nemotron --streaming --realtime --chunk-ms 20
done
for personal_locale in us india; do
  /usr/bin/time -l "$PERSONAL_BENCH_BINARY" run "$PERSONAL_BENCH_ROOT/$personal_locale-cleanup.json" "$PERSONAL_BENCH_ROOT" models/downloads "$PERSONAL_BENCH_ROOT/environment-cohere.json" "$PERSONAL_BENCH_ROOT/results/cohere-$personal_locale-cleanup-off.json" --model CohereLabs/cohere-transcribe-03-2026 --language en --cleanup >"$PERSONAL_BENCH_ROOT/logs/cohere-$personal_locale-cleanup-off.log" 2>&1
  run_case "cohere-$personal_locale-cleanup" "$personal_locale-cleanup" CohereLabs/cohere-transcribe-03-2026 models/downloads environment-cohere --language en --cleanup
done
node scripts/summarize-personalization-benchmark.mjs "$PERSONAL_BENCH_ROOT/summary.json" "$PERSONAL_BENCH_ROOT"/results/*.json
