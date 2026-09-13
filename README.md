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
build-ios.yml       iOS ipa（未签名）
build.yml           调用上面几个，并汇总成一个 release 草稿
```

每个平台工作流也支持单独 `workflow_dispatch`。另外 `commit-diff-image.yml` 负责生成提交差异图。

注意 iOS 已加入 release 的 `needs`，它构建失败会跳过 release。

## 移动端

移动端没有命令行参数，谱面和音乐由用户手动选择：

- iOS：App 内弹出系统文件选择器（`UIDocumentPickerViewController`）。先选谱面（`public.json`），再选音乐（`public.audio`）。选择器完全用 Rust + `objc2` 实现，不需要 Swift/Objective-C 文件；选中的文件通过 security-scoped URL 读取
- Android：目前是扫描应用 Documents 目录的临时实现，还没有系统选择器（需要 Java/Kotlin Activity 壳）

录制功能在移动端不可用（依赖 ffmpeg 子进程）。

iOS 的 `.app` 只是一个文件夹（cargo 二进制 + `Info.plist`），所以 CI 直接产出 `.ipa`，不需要 Xcode 工程。但构建未签名，装到设备前需要用 AltStore / Sideloadly 之类重签。

`Build iOS` 同时产出一个模拟器版本 `re-ch-rzl-rust-ios-simulator.zip`（`lipo` 合成的 arm64 + x86_64 通用 `.app`），也会发布到 Release：

```text
unzip re-ch-rzl-rust-ios-simulator.zip
xcrun simctl install booted re-ch-rzl-rust.app
xcrun simctl launch booted io.github.chcat1320.re-ch-rzl-rust
```

## 发布

产物包含：

```text
re-ch-rzl-rust-windows.zip          Windows（内含 re-ch-rzl-rust.exe）
re-ch-rzl-rust-macos.zip            macOS 通用二进制（Intel + Apple Silicon）
re-ch-rzl-rust-linux.zip            Linux x86_64
re-ch-rzl-rust-ios.ipa              iOS 真机（未签名，需自行重签）
re-ch-rzl-rust-ios-simulator.zip    iOS 模拟器（arm64 + x86_64 通用 app）
re-ch-rzl-rust-web-multifile.zip    web 多文件版
re-ch-rzl-rust-web-single.zip       web 单文件版（内含 re-ch-rzl-rust.html）
```

**自动草稿**

推送到 `master` 会自动创建或更新一个滚动草稿，tag 取 `Cargo.toml` 里的版本号：

```text
v0.1.0-draft
```

草稿在发布前不会真正创建 git tag，tag、标题、说明、资产都能在 Releases 页面自行修改，改完点 Publish 即可。

**指定 tag**

推 `v*` tag 会按该 tag 生成草稿：

```text
git tag v0.1.0
git push origin v0.1.0
```

也可以手动 Run workflow，在 `tag` 输入框填 `v0.1.0`；留空则只构建产物、不发 Release。

已存在同名 Release 时只覆盖资产，不会改动你改过的标题和说明。

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
