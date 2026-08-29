#!/usr/bin/env bash
# Capture README screenshots from the real frontend (Open Volume) and a
# widget composition that uses the same DirectoryBrowser (Volume Browser).
# Requires a local Chrome/Chromium. Does not need Screen Recording TCC.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
gui="$root/apps/cryptovol-gui"
out="$root/assets/screenshots"
mkdir -p "$out"

chrome=""
for c in \
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  "/Applications/Chromium.app/Contents/MacOS/Chromium" \
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"; do
  if [[ -x "$c" ]]; then
    chrome="$c"
    break
  fi
done
if [[ -z "$chrome" ]]; then
  echo "ERROR: Chrome/Chromium/Edge not found." >&2
  exit 1
fi

killall cryptovol-gui 2>/dev/null || true

cd "$gui"
npm run dev -- --host 127.0.0.1 --port 1420 >/tmp/cryptovol-vite-shots.log 2>&1 &
vite_pid=$!
cleanup() { kill "$vite_pid" 2>/dev/null || true; }
trap cleanup EXIT

for _ in $(seq 1 40); do
  if curl -sf "http://127.0.0.1:1420/" >/dev/null; then
    break
  fi
  sleep 0.25
done
curl -sf "http://127.0.0.1:1420/" >/dev/null

shot() {
  local url="$1"
  local dest="$2"
  "$chrome" --headless=new --disable-gpu --hide-scrollbars \
    --window-size=900,640 \
    --screenshot="$dest" \
    "$url" >/dev/null 2>&1
}

shot "http://127.0.0.1:1420/" "$out/open-volume.png"
shot "http://127.0.0.1:1420/screenshot-browser.html" "$out/volume-browser.png"

echo "Wrote $out/open-volume.png"
echo "Wrote $out/volume-browser.png"
