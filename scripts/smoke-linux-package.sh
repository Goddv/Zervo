#!/usr/bin/env bash
# Start a Linux package in Xvfb and wait for Servo to load a local HTML page.
#
#   ./scripts/smoke-linux-package.sh <executable> [launcher options...]
#
# A launcher may use ZERVO_SMOKE_DIR and ZERVO_SMOKE_URL to expose the fixture
# to a sandbox. No public site or real browser profile is needed.
set -euo pipefail

# shellcheck source=scripts/lib/common.sh
. "$(dirname -- "${BASH_SOURCE[0]}")/lib/common.sh"

[ "$#" -gt 0 ] || die "pass an AppImage, Flatpak launcher or binary"
need xvfb-run xdotool timeout
WORK="$(mktemp -d)"
zervo_cleanup_on_exit "$WORK"
export ZERVO_SMOKE_DIR="$WORK/page"
export ZERVO_SMOKE_URL="file://$ZERVO_SMOKE_DIR/test.html"
export XDG_CONFIG_HOME="$WORK/config"
export LIBGL_ALWAYS_SOFTWARE=1
mkdir -p "$ZERVO_SMOKE_DIR" "$XDG_CONFIG_HOME"
cat > "$ZERVO_SMOKE_DIR/test.html" <<'HTML'
<!doctype html><meta charset="utf-8"><title>Zervo package smoke test</title>
<p>Servo loaded the packaged browser.</p>
HTML

# Variables here are expanded by the shell running under Xvfb.
# shellcheck disable=SC2016
timeout --kill-after=5s 90s xvfb-run -a -s '-screen 0 1280x800x24 +extension GLX' bash -euc '
    "$@" "$ZERVO_SMOKE_URL" &
    app_pid=$!
    trap '\''kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true'\'' EXIT
    for attempt in {1..60}; do
        if xdotool search --onlyvisible --name "^Zervo package smoke test.*Zervo$" >/dev/null; then
            kill -0 "$app_pid"
            echo "Packaged Zervo opened a window and loaded the local page"
            exit 0
        fi
        if ! kill -0 "$app_pid" 2>/dev/null; then
            wait "$app_pid" || exit "$?"
            echo "Zervo exited before loading the page" >&2
            exit 1
        fi
        sleep 1
    done
    echo "Zervo did not show the test page within 60 seconds" >&2
    exit 1
' bash "$@"
