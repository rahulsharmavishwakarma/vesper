#!/usr/bin/env python3
"""Sequential native BitNet benchmark; public fixtures, no application integration."""

import argparse
import hashlib
import json
import os
import queue
import re
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def host_sample(excluded, child_group=None):
    busy, rss = [], 0
    for line in subprocess.check_output(
        ["ps", "-ww", "-axo", "pgid,pcpu,rss,comm"], text=True
    ).splitlines()[1:]:
        fields = line.split(None, 3)
        if len(fields) != 4:
            continue
        group, cpu, memory = int(fields[0]), float(fields[1]), int(fields[2])
        name = Path(fields[3]).name
        if group == child_group:
            rss += memory * 1024
        if group in excluded:
            continue
        watched = any(
            x in name.lower()
            for x in (
                "rustc",
                "clang",
                "cargo",
                "transcribe",
                "asr_",
                "python",
                "llama",
            )
        )
        if (watched and cpu >= 10) or cpu >= 80:
            busy.append({"name": name, "cpuPercent": cpu})
    return busy, rss


def time_command(command, platform):
    if platform == "darwin":
        return ["/usr/bin/time", "-l", *command]
    if platform == "linux":
        # GNU time's %M is KiB; use a stable, locale-independent record.
        return ["/usr/bin/time", "-f", "VESPER_PEAK_RSS_KIB %M", *command]
    raise RuntimeError(f"native resource measurement is unsupported on {platform}")


def peak_rss_bytes(stderr, platform):
    if platform == "darwin":
        pattern, scale = r"^\s*(\d+)\s+maximum resident set size\s*$", 1
    elif platform == "linux":
        pattern, scale = r"^(?:VESPER|TRANSCRIBE)_PEAK_RSS_KIB (\d+)\s*$", 1024
    else:
        return None
    matches = re.findall(pattern, stderr, flags=re.MULTILINE)
    return int(matches[-1]) * scale if matches else None


class Native:
    def __init__(self, command, log_prefix, rss_limit):
        self.lines = queue.Queue()
        self.stderr = []
        self.stdout = []
        self.contention = []
        self.observed_rss = 0
        self.rss_limit = rss_limit
        self.started = time.perf_counter()
        self.last_sample = 0.0
        self.end_received = None
        self.command = command
        self.platform = sys.platform
        env = dict(
            os.environ,
            OMP_NUM_THREADS="1",
            OPENBLAS_NUM_THREADS="1",
            VECLIB_MAXIMUM_THREADS="1",
            MKL_NUM_THREADS="1",
        )
        self.process = subprocess.Popen(
            time_command(command, self.platform),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
            env=env,
        )
        self.readers = []
        for tag, pipe in (
            ("stdout", self.process.stdout),
            ("stderr", self.process.stderr),
        ):
            thread = threading.Thread(
                target=self.pump,
                args=(tag, pipe, Path(str(log_prefix) + f".{tag}")),
                daemon=True,
            )
            thread.start()
            self.readers.append(thread)

    def pump(self, tag, pipe, path):
        with path.open("xb") as log:
            for line in iter(pipe.readline, b""):
                log.write(line)
                log.flush()
                self.lines.put((tag, line.decode("utf-8", errors="replace"), time.perf_counter()))
        self.lines.put((tag, None, time.perf_counter()))

    def sample(self):
        now = time.perf_counter()
        if now - self.last_sample < 1:
            return
        self.last_sample = now
        busy, rss = host_sample({os.getpgrp(), self.process.pid}, self.process.pid)
        self.observed_rss = max(self.observed_rss, rss)
        if busy:
            self.contention.append({"seconds": now - self.started, "processes": busy})
        if rss > self.rss_limit:
            raise RuntimeError("native process exceeded benchmark RSS budget")

    def send(self, text):
        if "\n" in text or "\r" in text:
            raise ValueError("newline in native protocol argument")
        self.process.stdin.write((text + "\n").encode())
        self.process.stdin.flush()

    def collect(self, marker=None, require_record=False, timeout=300):
        deadline = time.perf_counter() + timeout
        seen = marker is None
        record = None
        ended = set()
        self.end_received = None
        while True:
            self.sample()
            if time.perf_counter() >= deadline:
                raise TimeoutError("native benchmark deadline exceeded")
            try:
                tag, line, observed = self.lines.get(timeout=0.1)
            except queue.Empty:
                continue
            if line is None:
                ended.add(tag)
            else:
                getattr(self, tag).append(line)
                if tag == "stderr" and line.startswith("VIBEASR_BENCH "):
                    record = json.loads(line.removeprefix("VIBEASR_BENCH "))
                if tag == "stdout" and marker and line.rstrip("\r\n") == marker:
                    seen = True
                    self.end_received = observed
            if marker and seen and (record is not None or not require_record):
                return record
            if len(ended) == 2:
                self.process.wait(timeout=5)
                if marker and not seen:
                    raise RuntimeError("native process exited before expected response")
                if require_record and record is None:
                    raise RuntimeError("missing native measurement record")
                return record

    def stop(self):
        if self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGTERM)
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait(timeout=5)
        for reader in self.readers:
            reader.join(timeout=2)
        for pipe in (self.process.stdin, self.process.stdout, self.process.stderr):
            try:
                pipe.close()
            except BrokenPipeError:
                pass

    def evidence(self):
        text = "".join(self.stderr)
        return {
            "command": self.command,
            "exitCode": self.process.poll(),
            "wallMs": (time.perf_counter() - self.started) * 1000,
            "peakRssBytes": peak_rss_bytes(text, self.platform),
            "sampledPeakRssBytes": self.observed_rss,
            "contention": self.contention,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("plan", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--mode", choices=("cli", "resident"), required=True)
    parser.add_argument("--threads", type=int, choices=(1, 2, 4, 8), default=8)
    parser.add_argument("--no-normalize", action="store_true")
    parser.add_argument("--allow-contention", action="store_true")
    parser.add_argument("--max-arena-gib", type=float, default=16)
    parser.add_argument("--max-rss-gib", type=float, default=20)
    parser.add_argument("--max-tokens", type=int, default=1024)
    args = parser.parse_args()
    with args.output.open("x") as stream:
        stream.write("{}\n")
    plan = json.loads(args.plan.read_text())
    pins = json.loads((args.root / "verified-artifacts.json").read_text())
    for entry in pins["artifacts"]:
        if digest(args.root / "models" / entry["file"]) != entry["sha256"]:
            raise ValueError("model artifact checksum mismatch")
    name = "asr_infer" if args.mode == "cli" else "asr_stream_server"
    binary = args.root / "binaries" / name
    command = [
        str(binary),
        "--vae-model",
        str(args.root / "models/vibeasr-vae-encoder-i8_s.gguf"),
        "--lm-model",
        str(args.root / "models/vibeasr-lm-i2_s-embed-q6_k.gguf"),
        "-t",
        str(args.threads),
        "-c",
        "16384",
        "-b",
        "2048",
        "--max-tokens",
        str(args.max_tokens),
        "--greedy",
    ]
    if args.no_normalize:
        command.append("--no-normalize")
    logs = args.root / "logs"
    logs.mkdir(exist_ok=True)
    report = {
        "schemaVersion": 1,
        "status": "running",
        "mode": args.mode,
        "threads": args.threads,
        "normalization": not args.no_normalize,
        "precision": "I8_S encoders, I2_S decoder, Q6_K embeddings/head; CPU",
        "planSha256": digest(args.plan),
        "runnerSha256": digest(__file__),
        "binarySha256": digest(binary),
        "artifacts": pins,
        "plan": plan,
        "maxTokens": args.max_tokens,
        "maxArenaGiB": args.max_arena_gib,
        "maxRssGiB": args.max_rss_gib,
        "allowContention": args.allow_contention,
        "powerBefore": subprocess.check_output(["pmset", "-g", "batt"], text=True),
        "cases": [],
        "processes": [],
    }

    def save():
        args.output.write_text(json.dumps(report, indent=2) + "\n")

    started = time.perf_counter()
    server = None
    try:
        while not args.allow_contention:
            busy, _ = host_sample({os.getpgrp()})
            if not busy:
                break
            if time.perf_counter() - started > 60:
                raise RuntimeError(
                    "no quiet slot within 60s; rerun requires explicit contention mode"
                )
            time.sleep(2)
        for index, sample in enumerate(plan["cases"]):
            row = {"id": sample["id"], "error": None, "status": "started"}
            report["cases"].append(row)
            save()
            child = None
            try:
                if (
                    sample["durationSeconds"] * 24000 * 10240 + 512 * 1024**2
                    > args.max_arena_gib * 1024**3
                ):
                    row.update(
                        status="blocked",
                        error="source-derived VAE arena exceeds benchmark memory budget",
                    )
                    continue
                if digest(sample["audioPath"]) != sample["sha256"]:
                    raise ValueError("audio checksum mismatch")
                if args.mode == "resident":
                    if server is None:
                        server = Native(
                            command + ["--no-token-stream"],
                            logs / f"{args.output.stem}-{index}",
                            args.max_rss_gib * 1024**3,
                        )
                        server.collect("---READY---", timeout=120)
                        report.setdefault("residentLoadMs", []).append(
                            (time.perf_counter() - server.started) * 1000
                        )
                    child = server
                    child.send("CONTEXT:" + sample.get("hotwords", ""))
                    child.collect("---ACK---", timeout=10)
                    child.send("FORMAT:" + sample.get("promptFormat", "text"))
                    child.collect("---ACK---", timeout=10)
                    request_start = time.perf_counter()
                    child.send(sample["audioPath"])
                    value = child.collect("---END---", require_record=True)
                    row["responseMs"] = (child.end_received - request_start) * 1000
                else:
                    request = command + [
                        "--audio",
                        sample["audioPath"],
                        "--prompt-format",
                        sample.get("promptFormat", "text"),
                    ]
                    if sample.get("hotwords"):
                        request += ["--context", sample["hotwords"]]
                    child = Native(
                        request,
                        logs / f"{args.output.stem}-{index}",
                        args.max_rss_gib * 1024**3,
                    )
                    value = child.collect(require_record=True)
                    row["process"] = child.evidence()
                    if child.process.returncode != 0:
                        raise RuntimeError("native process failed")
                row["native"] = value
                row["sampledProcessRssBytes"] = child.observed_rss
                if value["stopReason"] != "eos":
                    raise RuntimeError("incomplete decoding: " + value["stopReason"])
                if value["acousticFrames"] != value["semanticFrames"]:
                    raise RuntimeError("audio encoder frame mismatch")
                row["status"] = "completed"
            except Exception as error:  # noqa: BLE001 - preserve failed native cases
                row.update(status="failed", error=f"{type(error).__name__}: {error}")
                if child and child.process.poll() is None and "native" not in row:
                    child.stop()
                    if child is server:
                        report["processes"].append(child.evidence())
                        server = None
            finally:
                if child and args.mode == "cli":
                    child.stop()
                    row.setdefault("process", child.evidence())
                save()
                print(
                    f"{index + 1}/{len(plan['cases'])} {sample['id']}: {row['status']}",
                    flush=True,
                )
        report["status"] = "completed"
    except Exception as error:
        report.update(status="failed", fatalError=str(error))
        raise
    finally:
        if server:
            try:
                server.send("EXIT")
                server.collect(timeout=20)
            finally:
                server.stop()
                report["processes"].append(server.evidence())
        report["powerAfter"] = subprocess.check_output(["pmset", "-g", "batt"], text=True)
        save()


if __name__ == "__main__":
    main()
