# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""What the checking scripts in this folder share: the compiler they
compare against, and the list of the compiler's source files.

The oracle is a compiler that is taken to be right: the one before the
change that is being checked. Every script takes `--oracle <path>`. Without
it the oracle is the checked-in compiler for this machine under
`bootstrap/`.
"""
import os, platform, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def host_target():
    system, machine = platform.system(), platform.machine().lower()
    if system == "Darwin" and machine in ("arm64", "aarch64"):
        return "aarch64-macos"
    if system == "Linux" and machine in ("x86_64", "amd64"):
        return "x86_64-linux"
    if system == "Linux" and machine in ("aarch64", "arm64"):
        return "aarch64-linux"
    return None


def seed(target=None):
    """The checked-in compiler that runs on `target` (this machine when not given)."""
    return os.path.join(ROOT, "bootstrap", target or host_target() or "unknown", "azol")


def path():
    """The oracle: `--oracle <path>`, else the seed."""
    arguments = sys.argv[1:]
    if "--oracle" in arguments:
        at = arguments.index("--oracle")
        if at + 1 >= len(arguments):
            sys.exit("--oracle needs the path of a compiler")
        return os.path.abspath(arguments[at + 1])
    return seed()


def missing():
    """What to say when the oracle is not there."""
    return "no compiler to compare with at %s: build one with `sh build.sh` and pass it with `--oracle ./azol`" % path()


def sources(target):
    """The files a compiler that runs on `target` is built from, as names in this folder."""
    with open(os.path.join(HERE, "SOURCES"), encoding="utf-8") as listed:
        names = [line.strip() for line in listed if line.strip() and not line.startswith("#")]
    return names + ["host_" + target.replace("-", "_") + ".azl"]


def generators():
    """The generator scripts whose output must not be stale, with what each needs."""
    return [os.path.join(HERE, "gen_runtime_azl.py")]
