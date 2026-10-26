# Copyright (c) 2026 Benjamin Benno Falkner
# SPDX-License-Identifier: MIT

"""Package an existing macOS binary into a locally signed, unnotarized DMG."""

import argparse
import platform
import plistlib
import re
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path


def run(*args):
    subprocess.run([str(arg) for arg in args], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--bundle-id", default="xyz.falkner.edoproxy")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("DMG packaging requires macOS")
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    if output.exists():
        parser.error(f"output already exists: {output}")
    root = Path(__file__).resolve().parent.parent
    with (root / "Cargo.toml").open("rb") as file:
        version = tomllib.load(file)["package"]["version"]
    numeric = re.match(r"\d+\.\d+\.\d+", version).group()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="edoproxy-dmg-") as temporary:
        work = Path(temporary)
        stage = work / "stage"
        app = stage / "edoproxy.app"
        contents = app / "Contents"
        resources = contents / "Resources"
        resources.mkdir(parents=True)
        (contents / "MacOS").mkdir()
        executable = contents / "MacOS/edoproxy"
        shutil.copy2(binary, executable)
        executable.chmod(0o755)
        with (root / "packaging/macos/Info.plist.in").open("rb") as file:
            info = plistlib.load(file)
        info.update(CFBundleIdentifier=args.bundle_id, CFBundleShortVersionString=numeric,
                    CFBundleVersion=numeric, EdoproxyVersion=version)
        with (contents / "Info.plist").open("wb") as file:
            plistlib.dump(info, file)
        iconset = work / "edoproxy.iconset"
        iconset.mkdir()
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                pixels = size * scale
                suffix = "@2x" if scale == 2 else ""
                run("sips", "-z", pixels, pixels, root / "assets/icons/edoproxy_logo.png",
                    "--out", iconset / f"icon_{size}x{size}{suffix}.png")
        run("iconutil", "-c", "icns", iconset, "-o", resources / "edoproxy.icns")
        for name in ("LICENSE", "edoproxy.example.toml"):
            shutil.copy2(root / name, resources / name)
            shutil.copy2(root / name, stage / name)
        (stage / "Applications").symlink_to("/Applications", target_is_directory=True)
        # Ad-hoc signing uses no certificate or developer identity. Apple Silicon
        # requires a code signature; this does not establish Gatekeeper trust.
        run("codesign", "--force", "--sign", "-", app)
        run("codesign", "--verify", "--strict", app)
        run("plutil", "-lint", contents / "Info.plist")
        run("hdiutil", "create", "-volname", f"edoproxy {version}", "-srcfolder", stage,
            "-format", "UDZO", output)
        run("hdiutil", "verify", output)
    print(output)


if __name__ == "__main__":
    main()
