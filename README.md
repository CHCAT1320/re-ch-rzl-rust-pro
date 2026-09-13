# re-ch-rzl-rust

Rizline 谱面播放 / 录制器。字体和命中音已编入二进制，运行时不依赖 `assets` 目录。

## 用法

播放：

```text
re-ch-rzl-rust.exe [谱面.json] [音频.wav]
re-ch-rzl-rust [谱面.json] [音频.wav]
```

未传路径时会弹出文件选择框。

| 参数 | 说明 |
| --- | --- |
| `--revelation 0.3` | 揭示缩放，默认 `1.0` |
| `--recorder` | 离线渲染视频 |
| `--width` / `--height` / `--fps` | 录制分辨率和帧率 |
| `--hwaccel` / `--no-hwaccel` | 是否使用硬件编码 |

录制模式会先找程序同目录的 `ffmpeg` / `ffmpeg.exe`，找不到再用 PATH。Mac 可用 Homebrew 安装：`brew install ffmpeg`。输出 `output.mp4`。

## Web

字体和命中音内嵌在 wasm 里，谱面和音乐在页面上手动上传。音频由页面内的 WebAudio 桥接处理，不使用 cpal。

仓库里没有 `web/` 目录，`index.html`、`app.js`、miniquad 加载器和单文件版都由 `.github/scripts/build_web.py` 在构建时生成。

```text
cargo rustc --release --target wasm32-unknown-unknown -- -C link-arg=--allow-undefined
python .github/scripts/build_web.py --wasm target/wasm32-unknown-unknown/release/re-ch-rzl-rust.wasm --out dist/web
```

生成两个版本：

```text
dist/web/multifile/   index.html + app.js + mq_js_bundle.js + re-ch-rzl-rust.wasm
dist/web/single/      re-ch-rzl-rust.html（单文件，wasm 以 base64 内联）
```

本地预览：

```text
python -m http.server 8123 --directory dist/web/multifile
```

打开 `http://127.0.0.1:8123/`，依次选择谱面 JSON 和音乐。必须用 HTTP 访问，`file://` 无法加载 wasm。

GitHub Actions 的 `web` job 会构建并上传 `re-ch-rzl-rust-web-multifile` 和 `re-ch-rzl-rust-web-single` 两个产物。

## 构建

```text
cargo build --release
```

GitHub Actions 会在 `master` 上自动编译 Windows exe 和 macOS 二进制，并生成提交差异图。

## 发布

推送 `v*` 形式的 tag 会触发 Release，产物包含：

```text
re-ch-rzl-rust.exe              Windows
re-ch-rzl-rust                  macOS
re-ch-rzl-rust-web-multifile.zip  web 多文件版
re-ch-rzl-rust-web-single.html    web 单文件版
```

```text
git tag v0.1.0
git push origin v0.1.0
```

已存在的 tag 重新运行会覆盖同名资产。

差异图由 bot 提交到 `diff/`，本地若用 merge 拉取会产生大量 `Merge branch 'master' of ...`，并让差异图只显示 bot 的图片更新。建议用 rebase：

```text
git config --global pull.rebase true
```

VS Code 仓库内已提供 `.vscode/settings.json`：

```json
{
  "git.rebaseWhenSync": true,
  "git.autofetch": true
}
```

`git.rebaseWhenSync` 控制“同步”按钮；`pull.rebase` 控制所有 `git pull`。两者一起设置最稳妥。

## 中文

![最新提交差异](diff/diff.zh.png)

## English

![Latest commit diff](diff/diff.en.png)
