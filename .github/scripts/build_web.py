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
  canvas { display: block; width: 100vw; height: 100vh; outline: none; }
  #uploader {
    position: fixed; top: 0; left: 0; z-index: 10;
    display: flex; gap: 12px; align-items: center;
    padding: 8px 12px; background: rgba(0, 0, 0, 0.6);
    color: #eee; font: 13px/1.4 system-ui, sans-serif;
  }
  #uploader label { display: flex; gap: 6px; align-items: center; }
  #upload-status { opacity: 0.8; }
</style>
</head>
<body>
<canvas id="glcanvas" tabindex="1"></canvas>
<div id="uploader">
  <label>谱面 JSON <input id="chart-file" type="file" accept=".json,application/json"></label>
  <label>音乐 <input id="music-file" type="file" accept="audio/*,.wav,.ogg,.mp3"></label>
  <span id="upload-status">等待上传</span>
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

(function () {
  "use strict";

  const AudioCtor = window.AudioContext || window.webkitAudioContext;
  let ctx = null;
  const sfxBuffers = {};
  let musicBuffer = null;
  let musicSource = null;
  let musicStartTime = 0;
  let musicPlaying = false;

  function ensureContext() {
    if (!ctx) {
      ctx = new AudioCtor();
    }
    return ctx;
  }

  function bytesFromWasm(ptr, len) {
    return new Uint8Array(wasm_memory.buffer, ptr, len).slice();
  }

  function decodeFromWasm(ptr, len) {
    return ensureContext().decodeAudioData(bytesFromWasm(ptr, len).buffer);
  }

  importObject.env.js_sfx_load = function (kind, ptr, len) {
    if (!AudioCtor) return;
    decodeFromWasm(ptr, len)
      .then((buffer) => { sfxBuffers[kind] = buffer; })
      .catch((err) => console.error("sfx decode failed", err));
  };

  importObject.env.js_sfx_play = function (kind) {
    const buffer = sfxBuffers[kind];
    if (!buffer || !ctx) return;
    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(ctx.destination);
    source.start();
  };

  importObject.env.js_music_play = function () {
    if (!musicBuffer) return;
    ensureContext().resume();
    const source = ctx.createBufferSource();
    source.buffer = musicBuffer;
    source.connect(ctx.destination);
    source.start();
    musicSource = source;
    musicStartTime = ctx.currentTime;
    musicPlaying = true;
  };

  importObject.env.js_music_position = function () {
    if (!musicPlaying || !ctx) return 0;
    return ctx.currentTime - musicStartTime;
  };

  const chartInput = document.getElementById("chart-file");
  const musicInput = document.getElementById("music-file");
  const status = document.getElementById("upload-status");

  function setStatus(text) {
    if (status) status.textContent = text;
  }

  if (chartInput) {
    chartInput.addEventListener("change", async () => {
      const file = chartInput.files && chartInput.files[0];
      if (!file) return;
      const bytes = new Uint8Array(await file.arrayBuffer());
      const ptr = wasm_exports.web_alloc(bytes.length);
      new Uint8Array(wasm_memory.buffer, ptr, bytes.length).set(bytes);
      wasm_exports.web_supply_chart(ptr, bytes.length);
      setStatus("谱面已加载：" + file.name);
    });
  }

  if (musicInput) {
    musicInput.addEventListener("change", async () => {
      const file = musicInput.files && musicInput.files[0];
      if (!file) return;
      try {
        musicBuffer = await ensureContext().decodeAudioData(await file.arrayBuffer());
        wasm_exports.web_set_music_ready();
        setStatus("音乐已加载：" + file.name);
      } catch (err) {
        console.error("music decode failed", err);
        setStatus("音乐解码失败：" + err);
      }
    });
  }
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
