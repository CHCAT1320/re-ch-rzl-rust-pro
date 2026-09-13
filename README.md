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

音频在页面内用 WebAudio 播放，浏览器要求用户手势后才能出声，所以 AudioContext 只在选择文件时才创建。

Web 版需要 **WebGL2**：挑战过渡用离屏 RenderTarget 合成，miniquad 的 MSAA resolve 依赖 `glReadBuffer` 和 `READ/DRAW_FRAMEBUFFER`，这些在 WebGL1 下不存在。`window_conf()` 已显式请求 WebGL2。

GitHub Actions 的 `Build Web` 工作流会构建并上传 `re-ch-rzl-rust-web-multifile` 和 `re-ch-rzl-rust-web-single` 两个产物。

## 构建

```text
cargo build --release
```

构建按平台拆成可复用工作流，由 `build.yml` 统一编排：

```text
build-windows.yml   Windows exe
build-macos.yml     macOS 通用二进制
build-linux.yml     Linux x86_64
build-web.yml       web 多文件版 + 单文件版
build.yml           调用上面 4 个，并汇总成一个 release 草稿
```

每个平台工作流也支持单独 `workflow_dispatch`。另外 `commit-diff-image.yml` 负责生成提交差异图。

## 发布

推送 `v*` 形式的 tag 会触发 Release，产物包含：

```text
re-ch-rzl-rust.exe                Windows
re-ch-rzl-rust-macos              macOS 通用二进制（Intel + Apple Silicon）
re-ch-rzl-rust-linux              Linux x86_64
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
