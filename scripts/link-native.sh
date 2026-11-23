#!/usr/bin/env bash
set -euo pipefail

# Build scripts and procedural macros are host tools. Only toview passes
# -static-pie from build.rs and needs the static C runtime and unwinder.
for arg in "$@"; do
    if [[ "$arg" == -static-pie ]]; then
        args=()
        for arg in "$@"; do
            case "$arg" in
                -Wl,-Bdynamic) args+=(-Wl,-Bstatic) ;;
                -lgcc_s) ;;
                -pie) ;;
                *) args+=("$arg") ;;
            esac
        done
        exec cc "${args[@]}"
    fi
done
exec cc "$@"
