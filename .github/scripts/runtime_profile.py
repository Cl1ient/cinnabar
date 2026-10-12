"""Run release CPU benchmarks and compare their Criterion median estimates in CI."""

import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import tomllib


SAMPLE_SIZE = 50
VARIANTS = {
    "baseline": {},
    "fat-lto": {"lto": "fat"},
    "x86-64-v2": {"rustflags": "-C target-cpu=x86-64-v2"},
    "mimalloc": {"mimalloc": True},
    "all": {"lto": "fat", "rustflags": "-C target-cpu=x86-64-v2", "mimalloc": True},
}
BENCHES = {
    "chunk-pipeline": ("chunk_costs", "flight_costs"),
    "meshing": ("chunk_costs", "entity_occlusion"),
    "world": ("chunk_costs", "light_work"),
}


def plan(output: Path) -> None:
    """Export the matrix from the same variant definitions used by the runner and summary."""
    matrix = {"include": [{"variant": name} for name in VARIANTS]}
    with output.open("a") as stream:
        stream.write(f"matrix={json.dumps(matrix)}\n")


def run(variant: str, root: Path) -> None:
    """Measure each requested bench sequentially, using isolated result directories."""
    settings = VARIANTS[variant]
    env = dict(os.environ)
    env["RUSTFLAGS"] = settings.get("rustflags", "")
    if "lto" in settings:
        env["CARGO_PROFILE_RELEASE_LTO"] = settings["lto"]
    target = Path(env["CARGO_TARGET_DIR"]).resolve()
    target.mkdir(parents=True, exist_ok=True)
    profile = tomllib.loads((root / "Cargo.toml").read_text())["profile"]["release"]
    metadata = {
        "variant": variant,
        "head_sha": env["GITHUB_SHA"],
        "sample_size": SAMPLE_SIZE,
        "profile": "release",
        "opt_level": profile.get("opt-level", 3),
        "codegen_units": profile["codegen-units"],
        "lto": settings.get("lto", profile["lto"]),
        "rustflags": env["RUSTFLAGS"],
        "allocator": "mimalloc" if settings.get("mimalloc") else "system",
        "rustc": subprocess.check_output(["rustc", "--version", "--verbose"], text=True),
        "cpu": json.loads(subprocess.check_output(["lscpu", "--json"], text=True)),
    }
    (target / "runtime-profile.json").write_text(json.dumps(metadata, indent=2) + "\n")
    for package, benches in BENCHES.items():
        features = []
        if package != "meshing":
            features.append("runtime-profile")
        if package == "chunk-pipeline":
            features.append("benchmark-support")
        if settings.get("mimalloc"):
            features.append("benchmark-mimalloc")
        for bench in benches:
            env["CRITERION_HOME"] = str(target / "criterion" / package / bench)
            command = [
                "cargo", "bench", "--locked", "--profile", "release",
                "--no-default-features",
                "-p", package, "--bench", bench,
            ]
            if features:
                command += ["--features", ",".join(features)]
            command += ["--", "--sample-size", str(SAMPLE_SIZE), "--noplot"]
            subprocess.run(command, cwd=root, env=env, check=True)


def read_results(directory: Path) -> dict[str, float]:
    """Read positive median nanoseconds, rejecting missing benches and wrong sample counts."""
    results = {}
    for package, benches in BENCHES.items():
        for bench in benches:
            paths = sorted(
                (directory / "criterion" / package / bench).glob("**/new/estimates.json")
            )
            if not paths:
                raise ValueError(f"Missing estimates: {directory.name}/{package}/{bench}")
            for path in paths:
                identity = json.loads(path.with_name("benchmark.json").read_text())["full_id"]
                key = f"{package}/{bench}: {identity}"
                median = json.loads(path.read_text())["median"]["point_estimate"]
                sample = json.loads(path.with_name("sample.json").read_text())
                if len(sample["times"]) != SAMPLE_SIZE or len(sample["iters"]) != SAMPLE_SIZE:
                    raise ValueError(f"Wrong sample count: {directory.name}/{key}")
                if (
                    not isinstance(median, (int, float))
                    or not math.isfinite(median)
                    or median <= 0
                ):
                    raise ValueError(f"Invalid median: {directory.name}/{key}")
                if key in results:
                    raise ValueError(f"Duplicate benchmark: {directory.name}/{key}")
                results[key] = median
    return results


def cell(value: str) -> str:
    """Escape text that could otherwise break a Markdown table cell."""
    return (
        value.replace("&", "&amp;").replace("<", "&lt;")
        .replace("|", "&#124;").replace("\n", " ")
    )


def summary(directory: Path) -> str:
    """Compare every case with baseline and retain CPU identity for interpreting runner noise."""
    results = {}
    metadata = {}
    for variant in VARIANTS:
        artifact = directory / f"runtime-profile-{variant}"
        data = json.loads((artifact / "runtime-profile.json").read_text())
        if (
            data["variant"] != variant
            or data["sample_size"] != SAMPLE_SIZE
            or data["profile"] != "release"
        ):
            raise ValueError(f"Unexpected run metadata: {variant}")
        metadata[variant] = data
        results[variant] = read_results(artifact)
    baseline = results["baseline"]
    for variant, cases in results.items():
        if metadata[variant]["head_sha"] != metadata["baseline"]["head_sha"]:
            raise ValueError(f"Different source revision: {variant}")
        if metadata[variant]["rustc"] != metadata["baseline"]["rustc"]:
            raise ValueError(f"Different compiler: {variant}")
        if cases.keys() != baseline.keys():
            missing = sorted(baseline.keys() - cases.keys())
            extra = sorted(cases.keys() - baseline.keys())
            raise ValueError(
                f"Different benchmark cases: {variant}; missing={missing}; extra={extra}"
            )
    lines = [
        "## Release runtime profile experiment",
        "",
        f"Commit: `{metadata['baseline']['head_sha']}`. {SAMPLE_SIZE} samples per case.",
        "Medians are nanoseconds per iteration. Change = (variant / baseline - 1) × 100%; negative is faster.",
        "Hosted runners can differ in CPU and load. These measurements guide follow-up captures; they do not change release defaults.",
        "",
        "| Benchmark | " + " | ".join(f"{name} median (ns) / change" for name in VARIANTS) + " |",
        "| --- | " + " | ".join("---:" for _ in VARIANTS) + " |",
    ]
    for case, base in sorted(baseline.items()):
        values = [
            f"{results[name][case]:,.2f} / {(results[name][case] / base - 1) * 100:+.2f}%"
            for name in VARIANTS
        ]
        lines.append("| " + cell(case) + " | " + " | ".join(values) + " |")
    lines += [
        "", "| Variant | CPU model | LTO | Rust flags | Allocator |",
        "| --- | --- | --- | --- | --- |",
    ]
    for variant, data in metadata.items():
        fields = {entry["field"]: entry["data"] for entry in data["cpu"]["lscpu"]}
        values = [
            variant, fields.get("Model name:", "unknown"), data["lto"],
            data["rustflags"] or "generic target", data["allocator"],
        ]
        lines.append("| " + " | ".join(cell(value) for value in values) + " |")
    return "\n".join(lines) + "\n"


def main() -> None:
    """Dispatch CI setup, measurements or report generation and expose incomplete results."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    setup = commands.add_parser("plan")
    setup.add_argument("output", type=Path)
    bench = commands.add_parser("run")
    bench.add_argument("variant", choices=VARIANTS)
    report = commands.add_parser("summary")
    report.add_argument("directory", type=Path)
    report.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.command == "plan":
        plan(args.output)
    elif args.command == "run":
        run(args.variant, Path.cwd())
    else:
        try:
            text = summary(args.directory)
        except (OSError, KeyError, ValueError) as error:
            args.output.write_text(f"## Runtime measurements incomplete\n\n{cell(str(error))}\n")
            raise SystemExit(str(error)) from error
        args.output.write_text(text)


if __name__ == "__main__":
    main()
