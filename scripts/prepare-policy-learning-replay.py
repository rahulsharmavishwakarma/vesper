"""Build deterministic synthetic trials or adapt cached public feedback. No inference."""

import argparse
import hashlib
import json
from pathlib import Path


class Builder:
    def __init__(self):
        self.serial = 0

    def episode(self, text, desired, feedback=True, model="fixture-model-a"):
        self.serial += 1
        key = f"constructed-{self.serial:04}"
        return {
            "input": {
                "id": key,
                "sourceKey": key,
                "episodeId": key,
                "atMs": self.serial * 180000,
                "project": "workspace",
                "language": "en",
                "modelId": model,
                "text": text,
            },
            "edit": desired if feedback else None,
            "reference": desired,
        }


def constructed():
    b = Builder()
    names = [("Martellis", "Martellys"), ("Alberto", "Albertoo")]

    def positives(feedback=True):
        return [
            b.episode(
                f"Review {a} onboarding handbook", f"Review {z} onboarding handbook", feedback
            )
            for a, z in names
        ]

    def teach(pairs):
        return [b.episode(f"Open {a} for onboarding", f"Open {z} for onboarding") for a, z in pairs]

    def negatives(pairs, topic, feedback=True):
        return [
            b.episode(f"Discuss {a} in {topic}", f"Discuss {a} in {topic}", feedback)
            for a, _ in pairs
        ]

    r1pairs = [
        ("readme sdk", "ReadmeSDK"),
        ("atlas api", "AtlasAPI"),
        ("photon sdk", "PhotonSDK"),
        ("vector api", "VectorAPI"),
        ("nimbus sdk", "NimbusSDK"),
        ("orion api", "OrionAPI"),
    ]
    learn = teach(r1pairs)
    for a, z in names:
        for _ in range(2):
            learn.append(b.episode(f"Meet {a} during onboarding", f"Meet {z} during onboarding"))
    r1 = {
        "id": "context",
        "learn": learn,
        "develop": negatives(r1pairs[:2], "linguistics exercises") + positives(),
        "validate": negatives(r1pairs[2:4], "linguistics exercises") + positives(),
        "audit": negatives(r1pairs[4:], "linguistics exercises", False) + positives(False),
    }

    r2pairs = [
        ("aster tool", "AsterTool"),
        ("comet tool", "CometTool"),
        ("solar tool", "SolarTool"),
        ("lunar tool", "LunarTool"),
        ("quartz tool", "QuartzTool"),
        ("cobalt tool", "CobaltTool"),
    ]
    r2 = {
        "id": "independent-support",
        "learn": teach(r2pairs),
        "develop": negatives(r2pairs[:2], "onboarding exercises") + positives(),
        "validate": negatives(r2pairs[2:4], "onboarding exercises") + positives(),
        "audit": negatives(r2pairs[4:], "onboarding exercises", False) + positives(False),
    }

    # A fresh distribution favors one-edit technical terms; validation can roll back.
    r3pairs = [
        ("helix sdk", "HelixSDK"),
        ("lyra api", "LyraAPI"),
        ("delta sdk", "DeltaSDK"),
        ("omega api", "OmegaAPI"),
        ("stellar sdk", "StellarSDK"),
        ("spectrum api", "SpectrumAPI"),
    ]

    def new_positives(pairs, feedback=True):
        return [
            b.episode(
                f"Review {a} onboarding handbook", f"Review {z} onboarding handbook", feedback
            )
            for a, z in pairs
        ]

    r3 = {
        "id": "distribution-change",
        "learn": teach(r3pairs),
        "develop": new_positives(r3pairs[:2]) + positives(),
        "validate": new_positives(r3pairs[2:4]) + positives(),
        "audit": new_positives(r3pairs[4:], False) + positives(False),
    }
    final = positives(False)
    for text in [
        "Do not send 15 files on Friday",
        "Quote 'readme sdk' during onboarding",
        'Keep "atlas api" exactly as dictated',
        "Actually make it Monday instead",
    ]:
        final.append(b.episode(text, text, False, "fixture-model-b"))
    # Preserve the adverse case after the distribution shift, not only favorable uses.
    final += negatives(r2pairs[4:], "onboarding exercises", False)
    return {
        "schemaVersion": 1,
        "source": {
            "kind": "authored synthetic feedback with externally supplied desired-text labels",
            "limitation": "Includes explicit outcome labels on unchanged outputs; not passive user behavior or a real-user study.",
            "protocol": "policy family 3x2; one-coordinate proposals; >=4 independent feedback cases, >=2 improved, zero regressed; fresh validation and held-out audit",
            "expectedDecisionSequence": ["promoted", "promoted", "rolled_back"],
            "note": "Expected decisions are report metadata, never passed to the controller. Adverse final cases retained.",
        },
        "rounds": [r1, r2, r3],
        "finalTest": final,
    }


def public(path, heldout_count=48):
    data = json.loads(path.read_text())
    train = data["source"]["learningEpisodes"]
    if (
        train != 192
        or not 1 <= heldout_count <= 1856
        or len(data["episodes"]) != train + heldout_count
    ):
        raise ValueError(
            "This protocol requires 192 calibration episodes and the explicitly requested held-out count"
        )

    def convert(e, feedback):
        return {
            "input": {
                "id": e["id"],
                "sourceKey": e["sourceKey"],
                "episodeId": e["id"],
                "atMs": e["atMs"],
                "project": e["project"],
                "language": e["language"],
                "modelId": e["modelId"],
                "text": e["input"],
            },
            "edit": e["edited"] if feedback else None,
            "reference": e["reference"],
        }

    episodes = data["episodes"]
    rounds = []
    for start in range(0, train, 64):
        chunk = [convert(e, True) for e in episodes[start : start + 64]]
        rounds.append(
            {
                "id": f"public-{start // 64 + 1}",
                "learn": chunk[:48],
                "develop": chunk[48:56],
                "validate": chunk[56:],
                "audit": [],
            }
        )
    return {
        "schemaVersion": 1,
        "source": {
            "kind": "cached public ASR; simulated independently supplied correction/review feedback",
            "inputSha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "upstream": data["source"],
            "limitation": "Chronological ordering is constructed; corpus recordings are not actual longitudinal dictation history.",
        },
        "rounds": rounds,
        "finalTest": [convert(e, False) for e in episodes[train:]],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["constructed", "public"])
    parser.add_argument("output", type=Path)
    parser.add_argument("--public-input", type=Path)
    parser.add_argument("--heldout-count", type=int, default=48)
    args = parser.parse_args()
    if args.mode == "public" and args.public_input is None:
        parser.error("--public-input is required for public mode")
    result = (
        constructed()
        if args.mode == "constructed"
        else public(args.public_input, args.heldout_count)
    )
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(f"Prepared {len(result['rounds'])} rounds; hidden audit/test feedback is absent.")
