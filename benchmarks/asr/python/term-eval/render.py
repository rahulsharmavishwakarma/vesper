"""Local audio rendering for the frozen term-eval corpus.

Synthetic speech comes from macOS `say`; it is labelled synthetic everywhere.
Outputs go to <cache>/datasets/term-eval-2026-10-26/ (never into git). A small
summary without absolute paths is written to the committed manifest directory.
"""

from __future__ import annotations

import json
import platform
import re
import shutil
import subprocess
import tarfile
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np
import soundfile as sf

import build_corpus as BC
import fetch

DATASET = "term-eval-2026-10-26"
SAMPLE_RATE = 16000
SNR_DB = 10.0
PEAK_LIMIT = 0.99
VOICES = [("Samantha", "en_US"), ("Daniel", "en_GB"), ("Rishi", "en_IN"),
          ("Karen", "en_AU"), ("Moira", "en_IE"), ("Tessa", "en_ZA")]
NEGATIVE_RENDER_QUOTAS = {"homophone": 100, "subword": 80, "protected": 64, "ordinary": 50}
BABBLE_POOL_SIZE = 30
BABBLE_TALKERS = 3
LIBRISPEECH_SUBSET = 300

FLEURS_SOURCE_DIR = "fleurs-en-us-100"
FLEURS_CORPUS_DIR = Path("corpora") / "fleurs-en-us" / "test"


def voice_label(v):
    return f"{v[0]} ({v[1]})"


def run(cmd):
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def macos_info() -> dict:
    out = {}
    for key in ("productName", "productVersion", "buildVersion"):
        out[key] = subprocess.run(["sw_vers", f"-{key}"], check=True, capture_output=True,
                                  text=True).stdout.strip()
    out["machine"] = platform.machine()
    return out


def installed_voices() -> dict:
    listing = subprocess.run(["say", "-v", "?"], check=True, capture_output=True, text=True).stdout
    found = {}
    for name, locale in VOICES:
        lines = [ln for ln in listing.splitlines()
                 if re.match(rf"^{re.escape(name)}\b", ln) and f" {locale} " in ln + " "]
        if not lines:
            raise SystemExit(f"voice {name} ({locale}) is not installed")
        found[name] = re.sub(r"\s+", " ", lines[0]).strip()
    return found


# ---------------------------------------------------------------- audio io
def read_mono(path: Path) -> np.ndarray:
    data, sr = sf.read(str(path), dtype="float32", always_2d=True)
    if sr != SAMPLE_RATE:
        raise SystemExit(f"{path.name}: expected {SAMPLE_RATE} Hz, got {sr}")
    return data.mean(axis=1)


def write_pcm16(path: Path, x: np.ndarray) -> None:
    sf.write(str(path), x.astype(np.float32), SAMPLE_RATE, subtype="PCM_16", format="WAV")


def rms(x: np.ndarray) -> float:
    return float(np.sqrt(np.mean(np.square(x.astype(np.float64)))))


def say_to_wav(text: str, voice: str, wav: Path, tmp_dir: Path) -> None:
    aiff = tmp_dir / (wav.stem + ".aiff")
    assert not text.startswith("-")
    run(["say", "-v", voice, "-o", str(aiff), text])  # always -o: never play audio aloud
    run(["afconvert", "-f", "WAVE", "-d", f"LEI16@{SAMPLE_RATE}", "-c", "1", str(aiff), str(wav)])
    aiff.unlink()


def pink_noise(n: int, rng: np.random.Generator) -> np.ndarray:
    spec = np.fft.rfft(rng.standard_normal(n))
    f = np.fft.rfftfreq(n, d=1.0 / SAMPLE_RATE)
    scale = np.zeros_like(f)
    scale[1:] = 1.0 / np.sqrt(f[1:])
    x = np.fft.irfft(spec * scale, n=n)
    return x / (rms(x) or 1.0)


def babble_noise(n: int, clips: list[np.ndarray], rng: np.random.Generator) -> np.ndarray:
    out = np.zeros(n, dtype=np.float64)
    for clip in clips:
        c = clip / (rms(clip) or 1.0)
        reps = int(np.ceil((n + len(c)) / len(c)))
        tiled = np.tile(c, reps)
        start = int(rng.integers(0, len(c)))
        out += tiled[start:start + n]
    return out / (rms(out) or 1.0)


def mix(speech: np.ndarray, unit_noise: np.ndarray) -> tuple[np.ndarray, float]:
    target_noise_rms = rms(speech) / (10 ** (SNR_DB / 20))
    y = speech.astype(np.float64) + unit_noise * target_noise_rms
    peak = float(np.max(np.abs(y)))
    scale = 1.0
    if peak > PEAK_LIMIT:
        scale = PEAK_LIMIT / peak
        y = y * scale
    return y, scale


def np_rng(*parts) -> np.random.Generator:
    return np.random.default_rng(int(BC.sha256_text("|".join([BC.SEED, *map(str, parts)]))[:16], 16))


# ------------------------------------------------------------- selection
def select_negatives(rows):
    chosen = []
    review = sorted((r for r in rows if r["subcategory"] == "review_case"), key=lambda r: r["id"])
    for sub, quota in NEGATIVE_RENDER_QUOTAS.items():
        groups = defaultdict(list)
        for r in rows:
            if r["category"] == "hard_negative" and r["subcategory"] == sub:
                groups[(r["workspaceId"], r["relatedTermId"] or "")].append(r)
        order = sorted(groups, key=lambda k: BC.sha256_text(f"render-group:{sub}:{k}"))
        for k in order:
            groups[k].sort(key=lambda r: BC.sha256_text("render:" + r["id"]))
        picked, i = [], 0
        while len(picked) < quota:
            progressed = False
            for k in order:
                if i < len(groups[k]) and len(picked) < quota:
                    picked.append(groups[k][i])
                    progressed = True
            if not progressed:
                break
            i += 1
        chosen += picked
    return review, sorted(chosen, key=lambda r: r["id"])


# ---------------------------------------------------------------- sources
def sources(mac: dict, fleurs_source: dict, frozen: dict, librispeech: bool, repo_url: str):
    out = [
        {"id": "vesper-term-eval-text", "name": "Vesper term-eval synthetic sentences (text)",
         "url": repo_url, "revision": "sentences.jsonl sha256:" + frozen["files"]["sentences.jsonl"],
         "license": "Project-authored synthetic text; no third-party text",
         "attribution": "Vesper contributors (synthetic evaluation text)",
         "redistribution": "Text is committed in the repository; contains no user data."},
        {"id": "macos-say-tts", "name": "macOS say synthetic speech (synthetic)",
         "url": "https://support.apple.com/guide/mac-help/",
         "revision": f"macOS {mac['productVersion']} build {mac['buildVersion']}",
         "license": "generated locally with macOS say; synthetic; not redistributed",
         "attribution": "Apple macOS system voices",
         "redistribution": "Not redistributed; regenerate locally with generate.py render."},
        {**fleurs_source},
        {"id": "fleurs-en_us-test-babble",
         "name": "FLEURS English (United States) test clips used only as babble noise",
         "url": fleurs_source["url"], "revision": fleurs_source["revision"],
         "license": fleurs_source["license"], "attribution": fleurs_source["attribution"],
         "redistribution": fleurs_source["redistribution"]},
        {"id": "cmudict", "name": "CMU Pronouncing Dictionary (homophone selection only)",
         "url": BC.CMUDICT["url"], "revision": BC.CMUDICT["revision"] + " sha256:" + BC.CMUDICT["sha256"],
         "license": "BSD-2-Clause-style (Carnegie Mellon University)",
         "attribution": "Carnegie Mellon University, CMUdict contributors",
         "redistribution": "Not redistributed; used to verify homophone distances."},
    ]
    if librispeech:
        out += [
            {"id": "librispeech-test-clean", "name": "LibriSpeech test-clean (rare-word subset)",
             "url": "https://www.openslr.org/12",
             "revision": "test-clean.tar.gz sha256:"
                         + fetch.PINNED["librispeech-test-clean"]["test-clean.tar.gz"][1]
                         + " md5:32fa31d27d2e1cad72775fee3f4849a9",
             "license": "CC-BY-4.0",
             "attribution": "LibriSpeech ASR corpus, Panayotov, Chen, Povey and Khudanpur (2015), "
                            "derived from LibriVox audiobooks",
             "redistribution": "Redistributions must retain attribution under CC-BY-4.0."},
            {"id": "fbai-speech-is21-deep-bias", "name": "Le et al. 2021 LibriSpeech biasing lists (N=100)",
             "url": f"https://github.com/facebookresearch/fbai-speech/tree/{fetch.FBAI_REV}/is21_deep_bias",
             "revision": fetch.FBAI_REV + " test-clean.biasing_100.tsv sha256:"
                         + fetch.PINNED["fbai-speech-43cdb3ca"]["test-clean.biasing_100.tsv"][1],
             "license": "MIT",
             "attribution": "Le et al., Contextualized Streaming End-to-End Speech Recognition with "
                            "Trie-Based Deep Biasing and Shallow Fusion, Interspeech 2021; "
                            "Copyright (c) Facebook, Inc. and its affiliates",
             "redistribution": "MIT: retain the copyright and permission notice."},
        ]
    for s in out:
        assert s["url"].startswith("https://") and all(str(v).strip() for v in s.values()), s
    return out


def sample(sid, source, split, path, digest, dur, ref, wid, targets, cat, sub, related, voice,
           synthetic, noise, biasing=None):
    return {"id": sid, "sourceId": source, "split": split, "language": "en-US",
            "audioPath": path, "sha256": digest, "durationSeconds": dur, "reference": ref,
            "workspaceId": wid, "targetTerms": list(targets), "category": cat,
            "subcategory": sub, "relatedTermId": related, "voice": voice,
            "synthetic": synthetic, "noise": noise, "biasingList": biasing}


def audio_meta(root: Path, rel: str):
    p = root / rel
    info = sf.info(str(p))
    return fetch.sha256_file(p), round(info.frames / info.samplerate, 3)


# ------------------------------------------------------------------ main
def render_all(cache_root: Path, manifest_dir: Path, frozen: dict, librispeech: bool, force: bool):
    root = cache_root / "datasets" / DATASET
    tmp = root / "_tmp"
    for d in (root, root / "tts", root / "noisy", root / "fleurs", tmp):
        d.mkdir(parents=True, exist_ok=True)
    mac = macos_info()
    voice_lines = installed_voices()
    terms = json.loads((manifest_dir / "terms.json").read_text())
    rows = [json.loads(ln) for ln in (manifest_dir / "sentences.jsonl").read_text().splitlines()]
    patterns = [BC.term_pattern(t["spelling"]) for _, t in BC.all_terms(terms)]
    details = {"peakScaled": [], "babble": {}}

    def tts(row, voice, idx):
        name = f"{row['id']}.{voice[0].lower()}.clean.wav"
        wav = root / "tts" / name
        if force or not wav.exists():
            say_to_wav(row["ttsText"], voice[0], wav, tmp)
        return f"tts/{name}"

    # ---- synthetic renders
    positives = sorted((r for r in rows if r["category"] == "positive"), key=lambda r: r["id"])
    review, neg_subset = select_negatives(rows)
    rendered_ids = set()
    samples = []
    jobs = []  # (row, voice)
    for i, r in enumerate(positives):
        jobs.append((r, VOICES[i % len(VOICES)]))
    k = len(positives)
    for i, r in enumerate(review):
        jobs.append((r, VOICES[(k + i) % 6]))
        jobs.append((r, VOICES[(k + i + 3) % 6]))
    k += len(review)
    for i, r in enumerate(neg_subset):
        jobs.append((r, VOICES[(k + i) % 6]))

    # babble pool: FLEURS en-us test clips NOT among the 100 real-speech controls
    fleurs_manifest_path = cache_root / "datasets" / FLEURS_SOURCE_DIR / "manifest.json"
    fleurs_manifest = json.loads(fleurs_manifest_path.read_text())
    control_ids = {s["id"].removeprefix("en_us-") for s in fleurs_manifest["samples"]}
    corpus_dir = cache_root / FLEURS_CORPUS_DIR
    candidates = sorted(p.stem for p in corpus_dir.glob("*.wav") if p.stem not in control_ids)
    pool_ids = sorted(candidates, key=lambda a: BC.sha256_text("babble:" + a))[:BABBLE_POOL_SIZE]
    assert not set(pool_ids) & control_ids
    pool = {a: read_mono(corpus_dir / f"{a}.wav") for a in pool_ids}
    details["babblePool"] = {a: fetch.sha256_file(corpus_dir / f"{a}.wav") for a in pool_ids}

    for n, (r, voice) in enumerate(jobs, 1):
        rel = tts(r, voice, n)
        rendered_ids.add(r["id"])
        digest, dur = audio_meta(root, rel)
        base = f"{r['id']}.{voice[0].lower()}"
        samples.append(sample(f"{base}.clean", "macos-say-tts", r["split"], rel, digest, dur,
                              r["reference"], r["workspaceId"], r["targetTerms"], r["category"],
                              r["subcategory"], r["relatedTermId"], voice_label(voice), True, "clean"))
        if r["category"] != "positive":
            continue
        speech = read_mono(root / rel)
        picks = [str(a) for a in np_rng("babble-pick", base).choice(pool_ids, BABBLE_TALKERS, replace=False)]
        details["babble"][base] = picks
        variants = {
            "pink10": pink_noise(len(speech), np_rng("pink", base)),
            "babble10": babble_noise(len(speech), [pool[a] for a in picks], np_rng("babble", base)),
        }
        for noise, unit in variants.items():
            name = f"{base}.{noise}.wav"
            out = root / "noisy" / name
            if force or not out.exists():
                y, scale = mix(speech, unit)
                write_pcm16(out, y)
                if scale != 1.0:
                    details["peakScaled"].append({"id": f"{base}.{noise}", "scale": round(scale, 6)})
            digest, dur = audio_meta(root, f"noisy/{name}")
            samples.append(sample(f"{base}.{noise}", "macos-say-tts", r["split"], f"noisy/{name}",
                                  digest, dur, r["reference"], r["workspaceId"], r["targetTerms"],
                                  r["category"], r["subcategory"], r["relatedTermId"],
                                  voice_label(voice), True, noise))
        if n % 50 == 0:
            print(f"rendered {n}/{len(jobs)}")

    # ---- text-only sentences
    for r in rows:
        if r["id"] in rendered_ids:
            continue
        samples.append(sample(r["id"], "vesper-term-eval-text", r["split"], None, None, None,
                              r["reference"], r["workspaceId"], r["targetTerms"], r["category"],
                              r["subcategory"], r["relatedTermId"], None, True, "clean"))

    # ---- FLEURS real-speech negatives (copied, hash-verified)
    fleurs_root = fleurs_manifest_path.parent
    fleurs_kept = fleurs_dropped = 0
    for s in sorted(fleurs_manifest["samples"], key=lambda s: s["id"]):
        src = fleurs_root / s["audioPath"]
        if fetch.sha256_file(src) != s["sha256"]:
            raise SystemExit(f"FLEURS hash mismatch for {s['id']}")
        if any(p.search(s["reference"]) for p in patterns):
            fleurs_dropped += 1
            continue
        rel = f"fleurs/{src.name}"
        if force or not (root / rel).exists():
            shutil.copyfile(src, root / rel)
        digest, dur = audio_meta(root, rel)
        assert digest == s["sha256"]
        sid = f"fleurs-{s['id']}"
        samples.append(sample(sid, s["sourceId"], BC.split_for(sid), rel, digest, dur, s["reference"],
                              None, [], "real_negative", "fleurs", None, None, False, "clean"))
        fleurs_kept += 1

    # ---- LibriSpeech rare-word lane
    libri = {"built": False}
    if librispeech:
        libri = build_librispeech(cache_root, root, samples)

    shutil.rmtree(tmp, ignore_errors=True)
    fleurs_source = next(s for s in fleurs_manifest["sources"] if s["id"] == "fleurs-en_us-test")
    repo_url = "https://github.com/rahulsharmavishwakarma/vesper/tree/main/benchmarks/asr/manifests/term-eval"
    manifest = {"schemaVersion": 1,
                "sources": sources(mac, fleurs_source, frozen, libri["built"], repo_url),
                "workspaces": terms["workspaces"], "distractors": terms["distractors"],
                "samples": sorted(samples, key=lambda s: s["id"])}
    validate_manifest(manifest, rows, root)
    text = json.dumps(manifest, indent=1, ensure_ascii=False) + "\n"
    (root / "term-manifest.json").write_text(text, encoding="utf-8")
    (root / "render-details.json").write_text(json.dumps(details, indent=1) + "\n", encoding="utf-8")

    audio = [s for s in manifest["samples"] if s["audioPath"]]
    dur_by = defaultdict(float)
    for s in audio:
        dur_by[f"{s['category']}/{s['subcategory']}/{s['noise']}"] += s["durationSeconds"]
    summary = {
        "schemaVersion": 1, "dataset": DATASET,
        "statement": "Audio rendered from the frozen corpus; no model output was used.",
        "frozenFiles": frozen["files"],
        "termManifestSha256": BC.sha256_text(text),
        "macos": mac,
        "voices": [{"name": n, "locale": loc, "sayListing": voice_lines[n]} for n, loc in VOICES],
        "conversion": f"say -v VOICE -o x.aiff TEXT; afconvert -f WAVE -d LEI16@{SAMPLE_RATE} -c 1",
        "noise": {"pink10": "1/f power pink noise, per-clip seeded, 10 dB SNR (full-clip RMS)",
                  "babble10": f"sum of {BABBLE_TALKERS} RMS-normalised FLEURS en-US test clips from a "
                              f"{BABBLE_POOL_SIZE}-clip pool disjoint from the real-speech controls, "
                              "10 dB SNR (full-clip RMS)",
                  "peakLimit": PEAK_LIMIT, "peakScaledClips": len(details["peakScaled"]),
                  "babblePoolFleursIds": pool_ids},
        "counts": {
            "samples": len(manifest["samples"]),
            "audioSamples": len(audio),
            "textOnlySamples": len(manifest["samples"]) - len(audio),
            "renderedSentences": len(rendered_ids),
            "syntheticClips": sum(1 for s in audio if s["synthetic"]),
            "byCategorySubcategoryNoise": dict(sorted(Counter(
                f"{s['category']}/{s['subcategory']}/{s['noise']}" for s in audio).items())),
            "byVoice": dict(sorted(Counter(s["voice"] for s in audio if s["voice"]).items())),
            "bySplitAudio": dict(sorted(Counter(s["split"] for s in audio).items())),
            "fleursKept": fleurs_kept, "fleursDroppedForTermCollision": fleurs_dropped,
        },
        "durationSeconds": {"total": round(sum(dur_by.values()), 2),
                            "byLane": {k: round(v, 2) for k, v in sorted(dur_by.items())}},
        "librispeech": libri,
    }
    (manifest_dir / "render-summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary["counts"], indent=1))
    print("total audio seconds", summary["durationSeconds"]["total"])


def build_librispeech(cache_root: Path, root: Path, samples: list) -> dict:
    tsv = fetch.pinned_path(cache_root, "fbai-speech-43cdb3ca", "test-clean.biasing_100.tsv")
    tar_path = fetch.pinned_path(cache_root, "librispeech-test-clean", "test-clean.tar.gz")
    rows = []
    for line in tsv.read_text(encoding="utf-8").splitlines():
        utt, ref, rare, bias = line.split("\t")
        rare, bias = json.loads(rare), json.loads(bias)
        if rare:
            rows.append((utt, ref, rare, bias))
    rows.sort(key=lambda r: BC.sha256_text("librispeech:" + r[0]))
    subset = rows[:LIBRISPEECH_SUBSET]
    want_audio, want_trans = {}, set()
    for utt, *_ in subset:
        spk, chap, _ = utt.split("-")
        want_audio[f"LibriSpeech/test-clean/{spk}/{chap}/{utt}.flac"] = utt
        want_trans.add(f"LibriSpeech/test-clean/{spk}/{chap}/{spk}-{chap}.trans.txt")
    extract = cache_root / "downloads" / "librispeech-test-clean" / "extracted-subset"
    if not all((extract / m).exists() for m in [*want_audio, *want_trans]):
        extract.mkdir(parents=True, exist_ok=True)
        wanted = set(want_audio) | want_trans | {"LibriSpeech/LICENSE.TXT"}
        with tarfile.open(tar_path, "r:gz") as tf:
            members = [m for m in tf if m.name in wanted and m.isfile()]
            tf.extractall(extract, members=members, filter="data")
    trans = {}
    for t in want_trans:
        for line in (extract / t).read_text(encoding="utf-8").splitlines():
            utt, text = line.split(" ", 1)
            trans[utt] = text
    out_dir = root / "librispeech"
    out_dir.mkdir(exist_ok=True)
    built = 0
    for member, utt in sorted(want_audio.items()):
        _, ref_lower, rare, bias = next(r for r in subset if r[0] == utt)
        ref = trans[utt]
        assert ref.lower() == ref_lower, utt
        present = [w for w in rare if BC.term_pattern(w).search(ref)]
        assert present == rare, (utt, rare)
        rel = f"librispeech/{utt}.wav"
        if not (root / rel).exists():
            run(["afconvert", "-f", "WAVE", "-d", f"LEI16@{SAMPLE_RATE}", "-c", "1",
                 str(extract / member), str(root / rel)])
        digest, dur = audio_meta(root, rel)
        sid = f"librispeech-{utt}"
        samples.append(sample(sid, "librispeech-test-clean", BC.split_for(sid), rel, digest, dur, ref,
                              None, rare, "rare_word", "librispeech", None, None, False, "clean",
                              biasing=bias))
        built += 1
    lic = extract / "LibriSpeech" / "LICENSE.TXT"
    return {"built": True, "utterances": built, "candidatesWithRareWords": len(rows),
            "selection": "first 300 by sha256('librispeech:'+utterance id) among rows with >=1 rare word",
            "licenseTxtSha256": fetch.sha256_file(lic) if lic.exists() else None,
            "biasingListRevision": fetch.FBAI_REV}


def validate_manifest(manifest, rows, root: Path):
    ids = set()
    keys = ["id", "sourceId", "split", "language", "audioPath", "sha256", "durationSeconds",
            "reference", "workspaceId", "targetTerms", "category", "subcategory", "relatedTermId",
            "voice", "synthetic", "noise", "biasingList"]
    src_ids = {s["id"] for s in manifest["sources"]}
    for s in manifest["samples"]:
        assert list(s) == keys, s["id"]
        assert s["id"] not in ids, s["id"]
        ids.add(s["id"])
        assert s["sourceId"] in src_ids, s
        assert s["split"] in {"dev", "test"} and s["noise"] in {"clean", "pink10", "babble10"}
        if s["audioPath"]:
            assert not s["audioPath"].startswith("/") and ".." not in s["audioPath"]
            assert (root / s["audioPath"]).exists()
    covered = {s["id"].split(".")[0] for s in manifest["samples"]}
    missing = [r["id"] for r in rows if r["id"] not in covered]
    assert not missing, missing[:5]
