# Copyright (c) 2026 Benjamin Benno Falkner
# SPDX-License-Identifier: MIT

"""Validate a release tag against Cargo.toml and emit GitHub Actions outputs."""

import os
import re
import sys
import tomllib
from pathlib import Path


def metadata(tag: str, version: str) -> dict[str, str]:
    pattern = r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-(alpha|beta|rc)\.(0|[1-9][0-9]*))?"
    if not re.fullmatch(pattern, tag):
        raise ValueError("Expected vMAJOR.MINOR.PATCH with optional -alpha.N, -beta.N or -rc.N")
    if tag[1:] != version:
        raise ValueError(f"Tag {tag} does not match Cargo.toml version {version}")
    prerelease = "-" in version
    return {"prerelease": str(prerelease).lower(), "latest": str(not prerelease).lower()}


if __name__ == "__main__":
    with Path("Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]
    try:
        values = metadata(os.environ["RELEASE_TAG"], version)
    except ValueError as error:
        sys.exit(str(error))
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for key, value in values.items():
            print(f"{key}={value}", file=output)
