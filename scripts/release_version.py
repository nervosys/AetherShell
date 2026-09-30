"""Validate a release version against the root package before publication."""
import argparse
from pathlib import Path
import re
import tomllib


def verify_release_version(version: str, manifest: Path) -> None:
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise ValueError("release version must be a stable semantic version")
    with manifest.open("rb") as source:
        package_version = tomllib.load(source)["package"]["version"]
    if version != package_version:
        raise ValueError(
            f"release version {version} differs from package version {package_version}"
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "Cargo.toml",
    )
    arguments = parser.parse_args()
    try:
        verify_release_version(arguments.version, arguments.manifest)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))
    print(f"version={arguments.version}")


if __name__ == "__main__":
    main()
