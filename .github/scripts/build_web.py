#!/usr/bin/env python3
"""Package a wasm build into two web distributions, generated at build time.

multifile/  index.html + app.js + mq_js_bundle.js + re-ch-rzl-rust.wasm
single/     one self-contained .html with the js inlined and the wasm as base64

`mq_js_bundle.js` is taken from the miniquad crate in the cargo registry, so it
always matches the miniquad version the wasm was linked against.

Usage:
    python .github/scripts/build_web.py --wasm target/.../re-ch-rzl-rust.wasm --out dist/web
"""

from __future__ import annotations

import argparse
import base64
import os
import pathlib

INDEX_HTML = """<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>re-ch-rzl-rust</title>
<style>
  html, body { margin: 0; padding: 0; height: 100%; background: #000; overflow: hidden; }
  /* Keep the canvas at 9:16 and letterbox it inside the window instead of
     stretching. min() picks the limiting axis, so the ratio stays exact in
     both landscape and portrait windows. */
  #glcanvas {
    position: absolute; top: 50%; left: 50%;
    transform: translate(-50%, -50%);
    width: min(100vw, 56.25vh);
    height: min(177.7778vw, 100vh);
    display: block; outline: none;
  }
  @supports (height: 100dvh) {
    #glcanvas {
      width: min(100vw, 56.25dvh);
      height: min(177.7778vw, 100dvh);
    }
  }
  /* Small corner control instead of a full-width bar, so it does not cover
     the letterboxed canvas. It collapses automatically once both files load. */
  #uploader {
    position: fixed; top: 8px; left: 8px; z-index: 10;
    color: #eee; font: 13px/1.4 system-ui, sans-serif;
  }
  #uploader-toggle {
    padding: 6px 12px; cursor: pointer;
    border: 1px solid rgba(255, 255, 255, 0.25); border-radius: 6px;
    background: rgba(0, 0, 0, 0.55); color: #eee;
  }
  #uploader-panel {
    margin-top: 6px; padding: 8px 10px; border-radius: 6px;
    display: flex; flex-direction: column; gap: 6px;
    background: rgba(0, 0, 0, 0.7);
  }
  #uploader-panel[hidden] { display: none; }
  #uploader-panel label { display: flex; gap: 6px; align-items: center; }
  #upload-status { opacity: 0.8; }
  /* Bottom-left settings stay reachable while playing, so speed and reveal
     scale can be tuned against the running chart. */
  #settings {
    position: fixed; bottom: 8px; left: 8px; z-index: 10;
    display: flex; flex-direction: column; gap: 6px;
    padding: 8px 10px; border-radius: 6px;
    background: rgba(0, 0, 0, 0.7); color: #eee;
    font: 13px/1.4 system-ui, sans-serif;
  }
  #settings label { display: flex; gap: 6px; align-items: center; }
  #settings input[type="range"] { width: 140px; }
  #settings .val { min-width: 36px; text-align: right; opacity: 0.85; }
</style>
</head>
<body>
<canvas id="glcanvas" tabindex="1"></canvas>
<div id="uploader">
  <button id="uploader-toggle" type="button">文件</button>
  <div id="uploader-panel">
    <label>chart JSON <input id="chart-file" type="file" accept=".json,application/json"></label>
    <label>music <input id="music-file" type="file" accept="audio/*,.wav,.ogg,.mp3"></label>
    <span id="upload-status">waiting</span>
  </div>
</div>
<div id="settings">
  <label>SPEED
    <input id="speed" type="range" min="1" max="20" step="0.1" value="7">
    <span class="val" id="speed-val">7.0</span>
  </label>
  <label>揭秘缩放
    <input id="revelation" type="range" min="0.2" max="2" step="0.01" value="1">
    <span class="val" id="revelation-val">1.00</span>
  </label>
</div>
<script src="mq_js_bundle.js"></script>
<script src="app.js"></script>
<script>
  load("re-ch-rzl-rust.wasm");
</script>
</body>
</html>
"""

APP_JS = """// WebAudio bridge + file upload intake for the wasm build.
//
// mq_js_bundle.js exposes `importObject` (the wasm import object) and, after
// instantiation, the globals `wasm_exports` and `wasm_memory`. This file must
// be loaded after mq_js_bundle.js and before load(...), so the extra imports
// exist by the time the module is instantiated.
//
// Every import is wrapped so it can never throw back into wasm: an exception
// raised inside a wasm import call aborts the frame without running Rust
// destructors, which leaks miniquad's event-handler borrow and makes every
// later input event trap with "unreachable executed". Safari throws
// synchronously when an AudioContext is created without a user gesture, so the
// context is only created once a file is picked.

(function () {
  "use strict";

  const AudioCtor = window.AudioContext || window.webkitAudioContext;
  let ctx = null;
  const sfxRaw = {};
  const sfxBuffers = {};
  let musicBuffer = null;
  let musicSource = null;
  let musicStartTime = 0;
  let musicPlaying = false;

  function safe(fn) {
    return function () {
      try {
        return fn.apply(null, arguments);
      } catch (err) {
        console.error("audio bridge error", err);
      }
    };
  }

  function ensureContext() {
    if (!ctx) {
      if (!AudioCtor) throw new Error("WebAudio is unavailable");
      ctx = new AudioCtor();
    }
    return ctx;
  }

  function copyFromWasm(ptr, len) {
    return new Uint8Array(wasm_memory.buffer, ptr, len).slice();
  }

  async function decodeSfx() {
    for (const kind of Object.keys(sfxRaw)) {
      if (sfxBuffers[kind]) continue;
      const copy = sfxRaw[kind].slice();
      sfxBuffers[kind] = await ensureContext().decodeAudioData(copy.buffer);
    }
  }

  importObject.env.js_sfx_load = safe(function (kind, ptr, len) {
    sfxRaw[kind] = copyFromWasm(ptr, len);
  });

  importObject.env.js_sfx_play = safe(function (kind) {
    const buffer = sfxBuffers[kind];
    if (!buffer || !ctx) return;
    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(ctx.destination);
    source.start();
  });

  importObject.env.js_music_play = safe(function () {
    if (!musicBuffer) return;
    const context = ensureContext();
    context.resume();
    const source = context.createBufferSource();
    source.buffer = musicBuffer;
    source.connect(context.destination);
    source.start();
    musicSource = source;
    musicStartTime = context.currentTime;
    musicPlaying = true;
  });

  importObject.env.js_music_position = function () {
    try {
      if (!musicPlaying || !ctx) return 0;
      return ctx.currentTime - musicStartTime;
    } catch (err) {
      console.error("audio bridge error", err);
      return 0;
    }
  };

  const chartInput = document.getElementById("chart-file");
  const musicInput = document.getElementById("music-file");
  const status = document.getElementById("upload-status");
  const toggle = document.getElementById("uploader-toggle");
  const panel = document.getElementById("uploader-panel");

  let chartLoaded = false;
  let musicLoaded = false;

  function setStatus(text) {
    if (status) status.textContent = text;
  }

  // Collapse the panel once both files are in, so it stops covering the canvas.
  function collapseWhenReady() {
    if (chartLoaded && musicLoaded && panel) {
      panel.hidden = true;
    }
  }

  if (toggle && panel) {
    toggle.addEventListener("click", () => {
      panel.hidden = !panel.hidden;
    });
  }

  if (chartInput) {
    chartInput.addEventListener("change", async () => {
      try {
        const file = chartInput.files && chartInput.files[0];
        if (!file) return;
        const bytes = new Uint8Array(await file.arrayBuffer());
        const ptr = wasm_exports.web_alloc(bytes.length);
        new Uint8Array(wasm_memory.buffer, ptr, bytes.length).set(bytes);
        wasm_exports.web_supply_chart(ptr, bytes.length);
        chartLoaded = true;
        setStatus("chart: " + file.name);
        collapseWhenReady();
      } catch (err) {
        console.error("chart load failed", err);
        setStatus("chart failed: " + err);
      }
    });
  }

  if (musicInput) {
    musicInput.addEventListener("change", async () => {
      try {
        const file = musicInput.files && musicInput.files[0];
        if (!file) return;
        const data = await file.arrayBuffer();
        await decodeSfx();
        musicBuffer = await ensureContext().decodeAudioData(data);
        wasm_exports.web_set_music_ready();
        musicLoaded = true;
        setStatus("music: " + file.name);
        collapseWhenReady();
      } catch (err) {
        console.error("music load failed", err);
        setStatus("music failed: " + err);
      }
    });
  }

  // SPEED and the chart reveal scale are live wasm state, so the sliders push
  // straight into the running module instead of reloading the chart.
  const speedInput = document.getElementById("speed");
  const speedValue = document.getElementById("speed-val");
  const revelationInput = document.getElementById("revelation");
  const revelationValue = document.getElementById("revelation-val");

  function applySpeed() {
    if (!speedInput) return;
    const value = parseFloat(speedInput.value);
    if (speedValue) speedValue.textContent = value.toFixed(1);
    if (typeof wasm_exports !== "undefined" && wasm_exports.web_set_speed) {
      wasm_exports.web_set_speed(value);
    }
  }

  function applyRevelation() {
    if (!revelationInput) return;
    const value = parseFloat(revelationInput.value);
    if (revelationValue) revelationValue.textContent = value.toFixed(2);
    if (typeof wasm_exports !== "undefined" && wasm_exports.web_set_revelation) {
      wasm_exports.web_set_revelation(value);
    }
  }

  if (speedInput) speedInput.addEventListener("input", applySpeed);
  if (revelationInput) revelationInput.addEventListener("input", applyRevelation);

  // The module instantiation is asynchronous, so seed the initial slider
  // values as soon as wasm_exports exists.
  const settingsTimer = setInterval(function () {
    if (typeof wasm_exports === "undefined") return;
    clearInterval(settingsTimer);
    applySpeed();
    applyRevelation();
  }, 100);
})();
"""

LOAD_BLOCK = """<script src="mq_js_bundle.js"></script>
<script src="app.js"></script>
<script>
  load("re-ch-rzl-rust.wasm");
</script>"""


def locked_miniquad_version(lock_path: pathlib.Path) -> str | None:
    if not lock_path.is_file():
        return None
    name = None
    for raw in lock_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith("name = "):
            name = line.split("=", 1)[1].strip().strip('"')
        elif line.startswith("version = ") and name == "miniquad":
            return line.split("=", 1)[1].strip().strip('"')
    return None


def find_bundle(lock_path: pathlib.Path) -> pathlib.Path:
    cargo_home = pathlib.Path(os.environ.get("CARGO_HOME", pathlib.Path.home() / ".cargo"))
    src_root = cargo_home / "registry" / "src"
    version = locked_miniquad_version(lock_path)
    patterns = []
    if version:
        patterns.append(f"*/miniquad-{version}/js/gl.js")
    patterns.append("*/miniquad-*/js/gl.js")
    for pattern in patterns:
        candidates = list(src_root.glob(pattern))
        if candidates:
            return max(candidates, key=lambda path: path.stat().st_mtime)
    raise SystemExit("miniquad gl.js not found in the cargo registry")


def inline_script(text: str) -> str:
    return text.replace("</script", "<\\/script")


def build_multifile(index_html: str, bundle: str, out_dir: pathlib.Path, wasm: pathlib.Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "index.html").write_text(index_html, encoding="utf-8", newline="\n")
    (out_dir / "app.js").write_text(APP_JS, encoding="utf-8", newline="\n")
    (out_dir / "mq_js_bundle.js").write_text(bundle, encoding="utf-8", newline="\n")
    (out_dir / "re-ch-rzl-rust.wasm").write_bytes(wasm.read_bytes())


def build_single(index_html: str, bundle: str, out_dir: pathlib.Path, wasm: pathlib.Path) -> None:
    encoded = base64.b64encode(wasm.read_bytes()).decode("ascii")
    loader = f"""<script>
{inline_script(bundle)}
</script>
<script>
{inline_script(APP_JS)}
</script>
<script>
(function () {{
  var binary = atob("{encoded}");
  var bytes = new Uint8Array(binary.length);
  for (var i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  var url = URL.createObjectURL(new Blob([bytes], {{ type: "application/wasm" }}));
  load(url);
}})();
</script>"""

    if LOAD_BLOCK not in index_html:
        raise SystemExit("index template no longer contains the expected loader block")

    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "re-ch-rzl-rust.html").write_text(
        index_html.replace(LOAD_BLOCK, loader), encoding="utf-8", newline="\n"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--wasm", type=pathlib.Path, required=True)
    parser.add_argument("--bundle", type=pathlib.Path)
    parser.add_argument("--lock", type=pathlib.Path, default=pathlib.Path("Cargo.lock"))
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("dist/web"))
    args = parser.parse_args()

    if not args.wasm.is_file():
        raise SystemExit(f"wasm not found: {args.wasm}")

    bundle_path = args.bundle or find_bundle(args.lock)
    bundle = bundle_path.read_text(encoding="utf-8")

    build_multifile(INDEX_HTML, bundle, args.out / "multifile", args.wasm)
    build_single(INDEX_HTML, bundle, args.out / "single", args.wasm)
    print(f"multifile -> {args.out / 'multifile'}")
    print(f"single    -> {args.out / 'single' / 're-ch-rzl-rust.html'}")


if __name__ == "__main__":
    main()
