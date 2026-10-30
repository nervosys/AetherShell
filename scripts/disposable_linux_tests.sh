#!/bin/bash
# Run system-changing contract tests as root in a disposable WSL distro.
#
# Builds the named test binaries in the developer's own Linux distro, as the
# developer, then runs only those binaries as root inside the disposable
# distro, with IRONSHELL_DISPOSABLE=1 and --ignored. The tests themselves refuse
# to run without that variable AND the /etc/ae-disposable marker, so neither
# a plain `cargo test` nor this script pointed at the wrong distro can create
# users, mount filesystems or install packages on a real machine.
#
# Usage (from Windows Git Bash, or anywhere wsl.exe works):
#   scripts/disposable_linux_tests.sh <source-dir-in-wsl> <target-dir-in-wsl> test_name [test_name...]
# Environment:
#   AE_BUILD_DISTRO   distro that builds (default: Debian)
#   AE_TEST_DISTRO    disposable distro that runs (default: ae-disposable)
#   AE_STAGE          Windows-visible staging dir as seen from WSL (default: /mnt/e/wsl/stage)
#   AE_TEST_MEMORY_MAX  memory ceiling per test binary, a systemd size (default: 4G)
set -euo pipefail
src=$1 target=$2
shift 2
build=${AE_BUILD_DISTRO:-Debian}
disposable=${AE_TEST_DISTRO:-ae-disposable}
stage=${AE_STAGE:-/mnt/e/wsl/stage}

if ! MSYS_NO_PATHCONV=1 wsl.exe -d "$disposable" -u root test -f /etc/ae-disposable; then
    echo "refusing: $disposable has no /etc/ae-disposable marker" >&2
    exit 2
fi

tests=()
for t in "$@"; do tests+=(--test "$t"); done
# Build as the developer; print each test executable's path.
exes=$(MSYS_NO_PATHCONV=1 wsl.exe -d "$build" bash -lc "cd '$src' && CARGO_TARGET_DIR='$target' CARGO_INCREMENTAL=0 cargo test --jobs 6 --no-run --message-format=json ${tests[*]} 2>/dev/null | grep -o '\"executable\":\"[^\"]*\"' | cut -d'\"' -f4" | tr -d '\r')
# The JSON also names the binaries the tests depend on (ae, aimodel); keep only
# the requested test executables, deps/<test>-<hash>.
wanted=$(echo "$exes" | grep -E "/deps/($(IFS='|'; echo "$*"))-[0-9a-f]+$" || true)
if [ -z "$wanted" ]; then
    # Say what came back instead: a failed build, or WSL itself failing.
    echo "build produced no test executables; output was:" >&2
    echo "$exes" | tail -20 >&2
    exit 1
fi
exes=$wanted

MSYS_NO_PATHCONV=1 wsl.exe -d "$build" bash -c "mkdir -p '$stage' && cp $(echo $exes) '$stage/'"
status=0
# Each binary runs in its own cgroup with a hard memory ceiling that covers the
# processes it starts. The WSL VM shares memory with the host; one runaway test
# (an awk_contracts parser loop, 2026-10-19) grew past 80 GB and took the host
# down with it.
memory_max=${AE_TEST_MEMORY_MAX:-4G}
for exe in $exes; do
    name=$(basename "$exe")
    echo "### $name"
    MSYS_NO_PATHCONV=1 wsl.exe -d "$disposable" -u root systemd-run --scope --quiet \
        -p MemoryMax="$memory_max" -p MemorySwapMax=0 \
        env IRONSHELL_DISPOSABLE=1 timeout 1800 "$stage/$name" --ignored --test-threads=1 || status=1
done
exit $status
