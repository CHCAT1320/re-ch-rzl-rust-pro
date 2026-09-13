#!/usr/bin/env python3
import datetime
import glob
import json
import os
import re
import subprocess
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

TEXT_EXT = {
    ".json",
    ".txt",
    ".md",
    ".yml",
    ".yaml",
    ".py",
    ".js",
    ".ts",
    ".css",
    ".html",
    ".xml",
    ".csv",
    ".gitignore",
    ".rs",
    ".toml",
    ".lock",
    ".hpp",
    ".h",
    ".c",
    ".cpp",
}
IMAGE_EXT = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"}
AUDIO_EXT = {".wav", ".mp3", ".ogg", ".flac", ".acb"}
VIDEO_EXT = {".webm", ".mp4", ".mov"}
FONT_EXT = {".ttf", ".otf", ".ttc", ".woff"}

CONTEXT_LINES = 3
LINE_H = 22
WIDTH = 1620
PAD = 28
GAP = 16
SIDEBAR_W = 400
PAGE_BG = "#0d1117"
CARD_BG = "#161b22"
HEADER_BG = "#21262d"
TRACK_BG = "#21262d"
BORDER = "#30363d"
FG = "#c9d1d9"
MUTED = "#8b949e"
ADD = "#3fb950"
DEL = "#f85149"
MOD = "#d29922"
LINK = "#58a6ff"
PURPLE = "#a371f7"
PINK = "#db61a2"
ADD_BG = "#12261e"
DEL_BG = "#2d1214"
HUNK_BG = "#121d2f"
NUM_FG = "#6e7681"

KIND_COLOR = {
    "text": LINK,
    "image": MOD,
    "audio": ADD,
    "video": PURPLE,
    "font": PINK,
    "binary": MUTED,
}
PALETTE = [LINK, ADD, MOD, PURPLE, PINK, DEL]
COMMIT_ROW_H = 46
COMMIT_DOT_Y = 12
LANE_W = 18
GRAPH_LEFT = 14
DIFF_CHUNK = 80

HUNK_RE = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")

STRINGS = {
    "zh": {
        "title": "提交差异图",
        "badge": {"A": "新增", "M": "修改", "D": "删除", "R": "重命名"},
        "kinds": {
            "text": "文本",
            "image": "图片",
            "audio": "音频",
            "video": "视频",
            "font": "字体",
            "binary": "二进制",
        },
        "old": "旧",
        "new": "新",
        "content": "内容",
        "none": "无",
        "lines": "行",
        "failed": "读取文本差异失败",
        "no_text_changes": "无文本改动",
        "new_text": "新增文本文件{extra}；略过内容",
        "del_text": "删除文本文件{extra}；略过内容",
        "rename_only": "仅重命名",
        "new_x": "新增{detail}",
        "del_x": "删除{detail}",
        "changed_x": "{detail}已有改动",
        "no_prev": "没有上一个提交",
        "nothing": "暂无可比较的改动。",
        "info": "提示",
        "summary": "共 {files} 个文件，新增 {added} 行，删除 {deleted} 行",
        "summary_lines": "共 {files} 个文件",
        "lang": "zh",
        "weekdays": ["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
        "history_badge": "历史",
        "history_title": "提交历史",
        "stats_badge": "统计",
        "stats_title": "本次提交概览",
        "stats_types": "文件类型分布",
        "stats_status": "文件状态",
        "stats_lines": "行数变化",
        "stat_files": "文件总数",
        "stat_added": "新增行",
        "stat_deleted": "删除行",
        "no_lines": "无文本行变化",
        "top_badge": "文件",
        "top_title": "变更最多文件",
        "top_hint": "按变更量排序",
        "dirs_badge": "目录",
        "dirs_title": "顶层目录分布",
        "dirs_hint": "按文件数排序",
        "root_dir": "根目录",
    },
    "en": {
        "title": "Commit Diff Image",
        "badge": {"A": "ADDED", "M": "MODIFIED", "D": "DELETED", "R": "RENAMED"},
        "kinds": {
            "text": "text",
            "image": "image",
            "audio": "audio",
            "video": "video",
            "font": "font",
            "binary": "binary",
        },
        "old": "old",
        "new": "new",
        "content": "content",
        "none": "none",
        "lines": "lines",
        "failed": "failed to read text diff",
        "no_text_changes": "no textual changes",
        "new_text": "new text file{extra}; content omitted",
        "del_text": "deleted text file{extra}; content omitted",
        "rename_only": "rename only",
        "new_x": "new {detail}",
        "del_x": "deleted {detail}",
        "changed_x": "{detail} changed",
        "no_prev": "No previous commit",
        "nothing": "Nothing to diff.",
        "info": "INFO",
        "summary": "{files} files changed, {added} insertions(+), {deleted} deletions(-)",
        "summary_lines": "{files} files changed",
        "lang": "en",
        "weekdays": ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        "history_badge": "HISTORY",
        "history_title": "Commit history",
        "stats_badge": "STATS",
        "stats_title": "Commit overview",
        "stats_types": "Files by type",
        "stats_status": "Files by status",
        "stats_lines": "Line changes",
        "stat_files": "Files",
        "stat_added": "Insertions",
        "stat_deleted": "Deletions",
        "no_lines": "no textual line changes",
        "top_badge": "FILES",
        "top_title": "Most changed files",
        "top_hint": "by change size",
        "dirs_badge": "DIRS",
        "dirs_title": "Top-level folders",
        "dirs_hint": "by file count",
        "root_dir": "(root)",
    },
}

BOT_MARKERS = ("update diff image", "update diff png", "更新差异图")


def run_git(*args) -> str:
    env = os.environ.copy()
    env["LC_ALL"] = "C.UTF-8"
    env["GIT_PAGER"] = "cat"
    return subprocess.check_output(
        ["git", "-c", "core.quotepath=false", *args],
        text=True,
        errors="replace",
        env=env,
    )


# The workflow bot commits `diff/*.png` back to the branch, so a plain
# `HEAD~1..HEAD` diff often shows only the bot's own image update (or nothing
# at all after a merge). Bot commits and the diff folder are ignored here.
DIFF_EXCLUDE = ":(exclude)diff"
BASE = "HEAD~1"


def has_parent() -> bool:
    return subprocess.run(
        ["git", "rev-parse", "--verify", "HEAD~1"],
        capture_output=True,
    ).returncode == 0


def sha_is_bot(sha: str) -> bool:
    raw = run_git("log", "-1", "--pretty=%an%x1f%s", sha)
    parts = raw.split("\x1f")
    author = parts[0].strip() if parts else ""
    subject = parts[1].strip().lower() if len(parts) > 1 else ""
    return author.endswith("[bot]") or any(marker in subject for marker in BOT_MARKERS)


def resolve_base() -> str | None:
    """Pick the commit to compare against.

    A merge commit's first parent is the local side, so `HEAD~1..HEAD` would
    only show whatever was merged in (the bot's diff images). Prefer a bot
    parent as the base when present, otherwise fall back to `HEAD~1`.
    """
    if not has_parent():
        return None
    parents = run_git("rev-list", "--parents", "-n", "1", "HEAD").split()[1:]
    for parent in parents:
        if sha_is_bot(parent):
            return parent
    return "HEAD~1"


def _glob(patterns: list[str]) -> list[str]:
    out: list[str] = []
    for pattern in patterns:
        out.extend(sorted(glob.glob(pattern, recursive=True)))
    return out


def load_face(size: int, patterns: list[str]):
    for path in _glob(patterns):
        if not os.path.isfile(path):
            continue
        for index in (0, 1, 2, 3, 4):
            try:
                return ImageFont.truetype(path, size=size, index=index)
            except OSError:
                continue
    return None


def load_pair(size: int) -> tuple:
    mono = load_face(
        size,
        [
            ".github/fonts/JetBrainsMono-Regular.ttf",
            os.path.expanduser("~/.fonts/JetBrainsMono-Regular.ttf"),
            "/usr/share/fonts/**/JetBrainsMono-Regular*",
            "/usr/share/fonts/**/JetBrainsMono*",
            "/usr/share/fonts/**/DejaVuSansMono.ttf",
        ],
    )
    cjk = load_face(
        size,
        [
            ".github/fonts/wqy-microhei.ttc",
            ".github/fonts/*CJK*",
            "/usr/share/fonts/**/*NotoSansCJK*",
            "/usr/share/fonts/**/*wqy*",
            "/usr/share/fonts/**/*NotoSerifCJK*",
        ],
    )
    if mono is None and cjk is None:
        raise SystemExit("No usable font found")
    mono = mono or cjk
    cjk = cjk or mono
    return (mono, cjk)


def use_cjk(ch: str) -> bool:
    code = ord(ch)
    return (
        0x2E80 <= code <= 0x303F
        or 0x3040 <= code <= 0x30FF
        or 0x3400 <= code <= 0x4DBF
        or 0x4E00 <= code <= 0x9FFF
        or 0xAC00 <= code <= 0xD7A3
        or 0xF900 <= code <= 0xFAFF
        or 0xFF00 <= code <= 0xFFEF
    )


_CHAR_W: dict[tuple[int, str], float] = {}


def char_w(font, ch: str) -> float:
    key = (id(font), ch)
    value = _CHAR_W.get(key)
    if value is None:
        value = font.getlength(ch)
        _CHAR_W[key] = value
    return value


def text_width(text: str, pair: tuple) -> float:
    mono, cjk = pair
    return sum(char_w(cjk if use_cjk(ch) else mono, ch) for ch in text)


def rich_len(text: str, pair: tuple) -> float:
    return text_width(text, pair)


def rich_draw(draw: ImageDraw.ImageDraw, x: float, y: float, text: str, fill: str, pair: tuple) -> float:
    mono, cjk = pair
    cjk_offset = mono.getmetrics()[0] - cjk.getmetrics()[0]
    run: list[str] = []
    current: bool | None = None

    def flush() -> None:
        nonlocal x
        if not run:
            return
        if current:
            draw.text((x, y + cjk_offset), "".join(run), fill=fill, font=cjk)
            x += cjk.getlength("".join(run))
        else:
            draw.text((x, y), "".join(run), fill=fill, font=mono)
            x += mono.getlength("".join(run))
        run.clear()

    for ch in text:
        flag = use_cjk(ch)
        if current is None:
            current = flag
        if flag != current:
            flush()
            current = flag
        run.append(ch)
    flush()
    return x


def fit_text(text: str, pair: tuple, max_px: float) -> str:
    mono, _ = pair
    ellipsis_w = char_w(mono, "…")
    running = 0.0
    for index, ch in enumerate(text):
        width = char_w(pair[1] if use_cjk(ch) else pair[0], ch)
        if running + width + ellipsis_w > max_px:
            return text[:index] + "…"
        running += width
    return text


def wrap_px(text: str, pair: tuple, max_px: float) -> list[str]:
    if max_px <= 0:
        return [text]
    mono, cjk = pair
    lines: list[str] = []
    current: list[str] = []
    running = 0.0
    for ch in text:
        width = char_w(cjk if use_cjk(ch) else mono, ch)
        if running + width > max_px and current:
            lines.append("".join(current))
            current = [ch]
            running = width
        else:
            current.append(ch)
            running += width
    lines.append("".join(current))
    return lines or [""]


def fmt_size(n: int | None) -> str:
    if n is None:
        return "?"
    units = ["B", "KB", "MB", "GB"]
    size = float(n)
    for unit in units:
        if size < 1024 or unit == units[-1]:
            return f"{int(size)} {unit}" if unit == "B" else f"{size:.1f} {unit}"
        size /= 1024
    return f"{n} B"


def classify(path: str) -> str:
    ext = Path(path).suffix.lower()
    if ext in TEXT_EXT:
        return "text"
    if ext in IMAGE_EXT:
        return "image"
    if ext in AUDIO_EXT:
        return "audio"
    if ext in VIDEO_EXT:
        return "video"
    if ext in FONT_EXT:
        return "font"
    return "binary"


def parse_name_status(raw: str) -> list[tuple[str, str, str | None]]:
    rows = []
    for line in raw.splitlines():
        if not line:
            continue
        parts = line.split("\t")
        status = parts[0][0]
        if status == "R" and len(parts) >= 3:
            rows.append(("R", parts[2], parts[1]))
        elif len(parts) >= 2:
            rows.append((status, parts[1], None))
    return rows


def _record_numstat(out: dict[str, tuple[int | None, int | None]], raw: str) -> None:
    for line in raw.splitlines():
        parts = line.split("\t")
        if len(parts) < 3:
            continue
        path = parts[2]
        if " => " in path:
            path = path.split(" => ")[-1].replace("{", "").replace("}", "").strip()
        out[path] = (
            int(parts[0]) if parts[0].isdigit() else None,
            int(parts[1]) if parts[1].isdigit() else None,
        )


def numstat_map(paths: list[str]) -> dict[str, tuple[int | None, int | None]]:
    out: dict[str, tuple[int | None, int | None]] = {}
    if not paths:
        return out
    for start in range(0, len(paths), 300):
        group = paths[start : start + 300]
        _record_numstat(out, run_git("diff", "--numstat", BASE, "HEAD", "--", *group, DIFF_EXCLUDE))
    return out


def api_tree_sizes(repo: str, token: str, sha: str) -> dict[str, int]:
    url = f"https://api.github.com/repos/{repo}/git/trees/{sha}?recursive=1"
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "User-Agent": "commit-diff-renderer",
        },
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        payload = json.load(resp)
    sizes: dict[str, int] = {}
    for item in payload.get("tree", []):
        if item.get("type") == "blob" and isinstance(item.get("size"), int):
            sizes[item["path"]] = item["size"]
    return sizes


def load_sizes(entries: list[tuple[str, str, str | None]]) -> dict[tuple[str, str], int]:
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    token = os.environ.get("GITHUB_TOKEN", "")
    if not repo or not token:
        return {}
    head_sha = run_git("rev-parse", "HEAD").strip()
    base_sha = run_git("rev-parse", BASE).strip()
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            head_future = pool.submit(api_tree_sizes, repo, token, head_sha)
            base_future = pool.submit(api_tree_sizes, repo, token, base_sha)
            head_sizes = head_future.result()
            base_sizes = base_future.result()
    except Exception as exc:  # noqa: BLE001
        print(f"[render] size API failed: {exc}", flush=True)
        return {}
    sizes: dict[tuple[str, str], int] = {}
    for status, path, old_path in entries:
        if status != "D" and path in head_sizes:
            sizes[("new", path)] = head_sizes[path]
        if status != "A":
            ref = old_path or path
            if ref in base_sizes:
                sizes[("old", path)] = base_sizes[ref]
    return sizes


def batch_text_rows(paths: list[str]) -> dict[str, list[dict]]:
    result: dict[str, list[dict]] = {}
    if not paths:
        return result
    known = set(paths)
    for start in range(0, len(paths), DIFF_CHUNK):
        group = paths[start : start + DIFF_CHUNK]
        raw = run_git("diff", "--no-color", f"--unified={CONTEXT_LINES}", BASE, "HEAD", "--", *group, DIFF_EXCLUDE)
        current: str | None = None
        old_no = new_no = 0
        for line in raw.splitlines():
            if line.startswith("diff --git "):
                header = line[11:]
                cut = header.rfind(" b/")
                candidate = header[cut + 3 :] if cut >= 0 else header
                current = candidate if candidate in known else None
                if current is not None:
                    result.setdefault(current, [])
                continue
            if current is None or line.startswith(("index ", "--- ", "+++ ")):
                continue
            hunk = HUNK_RE.match(line)
            if hunk:
                old_no = int(hunk.group(1))
                old_count = int(hunk.group(2) or "1")
                new_no = int(hunk.group(3))
                new_count = int(hunk.group(4) or "1")
                result[current].append(
                    {
                        "kind": "hunk",
                        "old_start": old_no,
                        "old_count": old_count,
                        "new_start": new_no,
                        "new_count": new_count,
                    }
                )
                continue
            if line.startswith("+"):
                result[current].append(
                    {"kind": "diff", "old": "", "new": str(new_no), "sign": "+", "text": line[1:], "color": ADD, "bg": ADD_BG}
                )
                new_no += 1
            elif line.startswith("-"):
                result[current].append(
                    {"kind": "diff", "old": str(old_no), "new": "", "sign": "-", "text": line[1:], "color": DEL, "bg": DEL_BG}
                )
                old_no += 1
            else:
                body = line[1:] if line.startswith(" ") else line
                result[current].append(
                    {"kind": "diff", "old": str(old_no), "new": str(new_no), "sign": " ", "text": body, "color": FG, "bg": CARD_BG}
                )
                old_no += 1
                new_no += 1
    return result


def humanize(iso: str) -> tuple[str, str, str]:
    try:
        stamp = datetime.datetime.fromisoformat(iso.replace("Z", "+00:00"))
    except ValueError:
        return iso, iso[:16].replace("T", " "), iso[:10]
    delta = datetime.datetime.now(datetime.timezone.utc) - stamp
    seconds = max(int(delta.total_seconds()), 0)
    for size, name in (
        (31536000, "year"),
        (2592000, "month"),
        (604800, "week"),
        (86400, "day"),
        (3600, "hour"),
        (60, "minute"),
        (1, "second"),
    ):
        if seconds >= size:
            count = seconds // size
            text = f"{count} {name}{'' if count == 1 else 's'} ago"
            break
    else:
        text = "just now"
    return text, stamp.strftime("%Y-%m-%d %H:%M"), stamp.strftime("%Y-%m-%d")


def api_history(repo: str, token: str) -> list[dict]:
    url = f"https://api.github.com/repos/{repo}/commits?per_page=100"
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "User-Agent": "commit-diff-renderer",
        },
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        payload = json.load(resp)
    parsed: list[dict] = []
    for item in payload:
        commit = item.get("commit", {})
        author = commit.get("author") or {}
        when, absolute, day = humanize(author.get("date", ""))
        parsed.append(
            {
                "sha": item.get("sha", ""),
                "parents": [p.get("sha", "") for p in item.get("parents", [])],
                "author": author.get("name", ""),
                "when": when,
                "absolute": absolute,
                "day": day,
                "refs": "",
                "subject": (commit.get("message", "") or "").splitlines()[0] if commit.get("message") else "",
            }
        )
    return parsed


def collect_history() -> list[dict]:
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    token = os.environ.get("GITHUB_TOKEN", "")
    if repo and token:
        try:
            return api_history(repo, token)
        except Exception as exc:  # noqa: BLE001
            print(f"[render] history API failed: {exc}", flush=True)
    raw = run_git(
        "log",
        "-n120",
        "--date=format:%Y-%m-%d %H:%M",
        "--pretty=format:%H%x1f%P%x1f%an%x1f%ar%x1f%ad%x1f%d%x1f%s%x1e",
    )
    parsed: list[dict] = []
    for record in (part for part in raw.split("\x1e") if part.strip()):
        parts = record.split("\x1f")
        absolute = parts[4].strip() if len(parts) > 4 else ""
        parsed.append(
            {
                "sha": parts[0].strip() if parts else "",
                "parents": parts[1].split() if len(parts) > 1 and parts[1].strip() else [],
                "author": parts[2].strip() if len(parts) > 2 else "",
                "when": parts[3].strip() if len(parts) > 3 else "",
                "absolute": absolute,
                "day": absolute[:10],
                "refs": parts[5].strip().strip("()").strip() if len(parts) > 5 else "",
                "subject": parts[6].strip() if len(parts) > 6 else "",
            }
        )
    return parsed


def collect_data() -> dict:
    global BASE
    base = resolve_base()
    data: dict = {
        "parent_ok": base is not None,
        "sha": run_git("rev-parse", "--short", "HEAD").strip(),
        "subject": run_git("log", "-1", "--pretty=%s").strip(),
        "history": collect_history(),
    }
    info = run_git("log", "-1", "--pretty=%an%x1f%ad%x1f%b", "--date=format:%Y-%m-%d %H:%M %z").split("\x1f")
    data["author"] = info[0].strip() if info else ""
    data["date"] = info[1].strip() if len(info) > 1 else ""
    data["body"] = info[2].strip() if len(info) > 2 else ""
    if not data["parent_ok"]:
        return data
    BASE = base
    mark = time.perf_counter()
    entries = parse_name_status(run_git("diff", "--name-status", BASE, "HEAD", "--", DIFF_EXCLUDE))
    print(f"[render] name-status {time.perf_counter() - mark:.1f}s ({len(entries)} files)", flush=True)
    data["entries"] = entries
    text_all = [path for status, path, _ in entries if classify(path) == "text"]
    text_changed = [
        path for status, path, _ in entries if classify(path) == "text" and status in ("M", "R")
    ]
    mark = time.perf_counter()
    data["ns"] = numstat_map(text_all)
    print(f"[render] numstat(text) {time.perf_counter() - mark:.1f}s ({len(text_all)} files)", flush=True)
    mark = time.perf_counter()
    data["sizes"] = load_sizes(entries)
    print(f"[render] sizes(api) {time.perf_counter() - mark:.1f}s", flush=True)
    mark = time.perf_counter()
    data["text_rows"] = batch_text_rows(text_changed)
    print(f"[render] text diffs {time.perf_counter() - mark:.1f}s ({len(text_changed)} files)", flush=True)
    _, added, deleted = diff_summary(data["ns"])
    data["files"], data["added"], data["deleted"] = len(entries), added, deleted
    return data


def diff_summary(ns: dict[str, tuple[int | None, int | None]]) -> tuple[int, int, int]:
    files = len(ns)
    added = sum(a for a, _ in ns.values() if a is not None)
    deleted = sum(d for _, d in ns.values() if d is not None)
    return files, added, deleted


def span_label(t: dict, side: str, start: int, count: int) -> str:
    if count == 0:
        return f"{side} {t['none']}"
    if count == 1:
        return f"{side} L{start}"
    return f"{side} L{start}–{start + count - 1}"


def render_text_rows(rows: list[dict], t: dict) -> list[dict]:
    out: list[dict] = []
    for row in rows:
        if row["kind"] == "hunk":
            old_side = span_label(t, t["old"], row["old_start"], row["old_count"])
            new_side = span_label(t, t["new"], row["new_start"], row["new_count"])
            out.append({"kind": "hunk", "text": f"{old_side}  →  {new_side}", "color": LINK, "bg": HUNK_BG})
        else:
            out.append(row)
    return out


def file_card(status: str, path: str, old_path: str | None, t: dict, data: dict) -> dict:
    kind = classify(path)
    kind_label = t["kinds"][kind]
    old_ref = old_path or path
    sizes = data["sizes"]
    ns = data["ns"]
    new_size = sizes.get(("new", path)) if status != "D" else None
    old_size = sizes.get(("old", path)) if status != "A" else None
    badge = t["badge"].get(status, status)
    accent = {"A": ADD, "M": MOD, "D": DEL, "R": LINK}.get(status, FG)
    title = path if status != "R" else f"{old_path}  →  {path}"
    if status == "A":
        size_bit = fmt_size(new_size)
    elif status == "D":
        size_bit = fmt_size(old_size)
    elif old_size is not None and new_size is not None:
        delta = new_size - old_size
        sign = "+" if delta >= 0 else "-"
        size_bit = f"{fmt_size(old_size)} → {fmt_size(new_size)}  ({sign}{fmt_size(abs(delta))})"
    else:
        size_bit = ""

    rows: list[dict] = []
    if kind == "text":
        added, deleted = ns.get(path, (None, None))
        if status == "A":
            extra = f" · {added} {t['lines']}" if added is not None else ""
            rows.append({"kind": "meta", "text": t["new_text"].format(extra=extra), "color": MUTED})
        elif status == "D":
            extra = f" · {deleted} {t['lines']}" if deleted is not None else ""
            rows.append({"kind": "meta", "text": t["del_text"].format(extra=extra), "color": MUTED})
        elif status == "R" and old_size == new_size:
            rows.append({"kind": "meta", "text": t["rename_only"], "color": MUTED})
        else:
            rows = render_text_rows(data["text_rows"].get(path, []), t)
            if not rows:
                rows = [{"kind": "meta", "text": t["no_text_changes"], "color": MUTED}]
    else:
        detail = kind_label
        if status == "A":
            rows.append({"kind": "meta", "text": t["new_x"].format(detail=detail), "color": MUTED})
        elif status == "D":
            rows.append({"kind": "meta", "text": t["del_x"].format(detail=detail), "color": MUTED})
        else:
            rows.append({"kind": "meta", "text": t["changed_x"].format(detail=detail), "color": MUTED})

    return {
        "title": title,
        "badge": badge,
        "accent": accent,
        "meta": f"{kind_label}  ·  {size_bit}".strip(" ·"),
        "rows": rows,
    }


def bar_row(label: str, value: str, ratio: float, color: str) -> dict:
    return {"kind": "bar", "text": label, "value": value, "ratio": max(0.0, min(ratio, 1.0)), "color": color}


def stats_card(t: dict, data: dict) -> dict:
    entries = data["entries"]
    kind_counts: dict[str, int] = {}
    status_counts: dict[str, int] = {}
    for status, path, old_path in entries:
        kind = classify(path)
        kind_counts[kind] = kind_counts.get(kind, 0) + 1
        status_counts[status] = status_counts.get(status, 0) + 1
    added, deleted = data["added"], data["deleted"]

    rows: list[dict] = [{"kind": "section", "text": t["stats_types"], "color": LINK}]
    if kind_counts:
        top = max(kind_counts.values())
        for kind in ("text", "image", "audio", "video", "font", "binary"):
            if kind in kind_counts:
                rows.append(bar_row(t["kinds"][kind], str(kind_counts[kind]), kind_counts[kind] / top, KIND_COLOR[kind]))
    else:
        rows.append({"kind": "meta", "text": t["none"], "color": MUTED})

    rows.append({"kind": "section", "text": f"{t['stats_status']}  ·  {t['stat_files']} {len(entries)}", "color": MOD})
    if status_counts:
        top = max(status_counts.values())
        status_color = {"A": ADD, "M": MOD, "D": DEL, "R": LINK}
        for status in ("A", "M", "D", "R"):
            if status in status_counts:
                rows.append(
                    bar_row(
                        t["badge"].get(status, status),
                        str(status_counts[status]),
                        status_counts[status] / top,
                        status_color.get(status, FG),
                    )
                )
    else:
        rows.append({"kind": "meta", "text": t["none"], "color": MUTED})

    rows.append({"kind": "section", "text": t["stats_lines"], "color": ADD})
    if added or deleted:
        top = max(added, deleted, 1)
        rows.append(bar_row(t["stat_added"], f"+{added}", added / top, ADD))
        rows.append(bar_row(t["stat_deleted"], f"-{deleted}", deleted / top, DEL))
    else:
        rows.append({"kind": "meta", "text": t["no_lines"], "color": MUTED})

    return {"title": t["stats_title"], "badge": t["stats_badge"], "accent": LINK, "meta": "", "rows": rows}


def top_files_card(t: dict, data: dict) -> dict:
    ns = data["ns"]
    sizes = data["sizes"]
    items: list[tuple[float, str, str, str]] = []
    for status, path, old_path in data["entries"]:
        added, deleted = ns.get(path, (None, None))
        if added is not None and deleted is not None and (added + deleted) > 0:
            metric = float(added + deleted)
            value = f"+{added} -{deleted}"
        else:
            new_size = sizes.get(("new", path))
            old_size = sizes.get(("old", path))
            if new_size is None and old_size is None:
                continue
            delta = (new_size or 0) - (old_size or 0)
            metric = float(abs(delta) or 1)
            value = ("+" if delta >= 0 else "-") + fmt_size(abs(delta))
        items.append((metric, path, value, classify(path)))
    items.sort(key=lambda item: item[0], reverse=True)
    rows: list[dict] = [{"kind": "section", "text": t["top_hint"], "color": MOD}]
    if items:
        top = items[0][0] or 1.0
        for metric, path, value, kind in items[:8]:
            rows.append(bar_row(os.path.basename(path), value, metric / top, KIND_COLOR[kind]))
    else:
        rows.append({"kind": "meta", "text": t["none"], "color": MUTED})
    return {"title": t["top_title"], "badge": t["top_badge"], "accent": MOD, "meta": "", "rows": rows}


def dirs_card(t: dict, data: dict) -> dict:
    counts: dict[str, int] = {}
    for status, path, old_path in data["entries"]:
        top = path.split("/")[0] if "/" in path else t["root_dir"]
        counts[top] = counts.get(top, 0) + 1
    rows: list[dict] = [{"kind": "section", "text": t["dirs_hint"], "color": PURPLE}]
    if counts:
        top = max(counts.values())
        for i, (name, count) in enumerate(sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:8]):
            rows.append(bar_row(name, str(count), count / top, PALETTE[i % len(PALETTE)]))
    else:
        rows.append({"kind": "meta", "text": t["none"], "color": MUTED})
    return {"title": t["dirs_title"], "badge": t["dirs_badge"], "accent": PURPLE, "meta": "", "rows": rows}


REL_UNITS = {
    "second": "秒",
    "minute": "分钟",
    "hour": "小时",
    "day": "天",
    "week": "周",
    "month": "个月",
    "year": "年",
}


def localize_when(text: str, t: dict, absolute: str) -> str:
    if t.get("lang") != "zh":
        return text
    raw = text.strip()
    if raw in ("just now", "now"):
        return "刚刚"
    if raw == "yesterday":
        return "昨天"
    translated, count = re.subn(
        r"(\d+)\s+(second|minute|hour|day|week|month|year)s?",
        lambda m: f"{m.group(1)} {REL_UNITS.get(m.group(2), m.group(2))}",
        raw,
    )
    if count == 0:
        return absolute or raw
    translated = translated.replace(" ago", "前").replace(", ", " ").strip()
    if "前" not in translated:
        return absolute or raw
    return translated


def day_label(t: dict, day: str) -> str:
    try:
        parsed = datetime.datetime.strptime(day, "%Y-%m-%d")
        return f"{day}  {t['weekdays'][parsed.weekday()]}"
    except ValueError:
        return day


def is_bot_commit(commit: dict) -> bool:
    subject = commit.get("subject", "").lower()
    return commit.get("author", "").endswith("[bot]") or any(marker in subject for marker in BOT_MARKERS)


def history_card(t: dict, data: dict, max_rows: int = 20) -> dict:
    parsed = data["history"]
    all_parents = {c["sha"]: c["parents"] for c in parsed}
    bot_shas = {c["sha"] for c in parsed if is_bot_commit(c)}

    def resolve_parent(sha: str) -> str | None:
        seen: set[str] = set()
        while sha in bot_shas:
            if sha in seen:
                return None
            seen.add(sha)
            parents = all_parents.get(sha) or []
            if not parents:
                return None
            sha = parents[0]
        return sha

    commits: list[dict] = []
    for commit in parsed:
        if commit["sha"] in bot_shas:
            continue
        commit = dict(commit)
        commit["parents"] = [r for r in (resolve_parent(p) for p in commit["parents"]) if r]
        commits.append(commit)
        if len(commits) >= max_rows:
            break

    order = {c["sha"]: i for i, c in enumerate(commits)}
    active: list[str | None] = []
    row_lane: dict[int, int] = {}
    for i, commit in enumerate(commits):
        matching = [j for j, sha in enumerate(active) if sha == commit["sha"]]
        if matching:
            lane = matching[0]
            for j in reversed(matching[1:]):
                active[j] = None
        else:
            if None in active:
                lane = active.index(None)
                active[lane] = commit["sha"]
            else:
                lane = len(active)
                active.append(commit["sha"])
        row_lane[i] = lane
        if commit["parents"]:
            active[lane] = commit["parents"][0]
            for parent in commit["parents"][1:]:
                if None in active:
                    active[active.index(None)] = parent
                else:
                    active.append(parent)
        else:
            active[lane] = None
        while active and active[-1] is None:
            active.pop()

    edges: list[tuple[int, int, int, int]] = []
    for i, commit in enumerate(commits):
        for parent in commit["parents"]:
            if parent in order:
                pi = order[parent]
                edges.append((i, row_lane[i], pi, row_lane[pi]))

    rows: list[dict] = []
    prev_day = None
    for i, commit in enumerate(commits):
        lane = row_lane[i]
        if commit["day"] and commit["day"] != prev_day:
            rows.append({"kind": "divider", "text": day_label(t, commit["day"]), "color": MUTED})
            prev_day = commit["day"]
        rows.append(
            {
                "kind": "commit",
                "index": i,
                "lane": lane,
                "sha": commit["sha"][:7],
                "subject": commit["subject"],
                "author": commit["author"],
                "when": localize_when(commit["when"], t, commit["absolute"]),
                "refs": commit["refs"],
                "merge": len(commit["parents"]) > 1,
                "head": i == 0,
                # A parent outside the rendered window means the graph should
                # keep drawing downward instead of looking like a dead end.
                "continues": any(parent not in order for parent in commit["parents"]),
                "color": PALETTE[lane % len(PALETTE)],
            }
        )
    lane_count = max(row_lane.values(), default=0) + 1
    return {
        "title": t["history_title"],
        "badge": t["history_badge"],
        "accent": PURPLE,
        "meta": "",
        "rows": rows,
        "graph": {"edges": edges, "lane_count": lane_count},
    }


def card_header_h() -> int:
    return 52


def layout_card(card: dict, w: int, fonts: dict, t: dict) -> dict:
    inner_l = 16
    inner_r = w - 16
    inner_w = inner_r - inner_l
    header_h = card_header_h()
    has_diff = any(row["kind"] == "diff" for row in card["rows"])
    body_top = header_h + 8 + (LINE_H if has_diff else 0)

    laid: list[dict] = []
    y = body_top
    for row in card["rows"]:
        kind = row["kind"]
        if kind == "diff":
            text_x = 16 + 40 * 2 + 18 + 6 + 16
            pieces = wrap_px(row.get("text") or " ", fonts["regular"], inner_r - text_x)
            h = LINE_H * len(pieces)
            laid.append({"row": row, "y": y, "h": h, "pieces": pieces, "text_x": text_x})
            y += h
        elif kind == "commit":
            lane_count = card.get("graph", {}).get("lane_count", 1)
            text_x = 16 + GRAPH_LEFT + lane_count * LANE_W + 12
            max_px = inner_r - text_x
            subject = fit_text(row["subject"], fonts["regular"], max_px)
            meta_parts = [row["sha"]]
            if row["refs"]:
                meta_parts.append(row["refs"])
            if row["author"]:
                meta_parts.append(row["author"])
            if row["when"]:
                meta_parts.append(row["when"])
            meta_text = fit_text("  ·  ".join(meta_parts), fonts["small"], max_px)
            laid.append({"row": row, "y": y, "h": COMMIT_ROW_H, "subject": subject, "meta": meta_text, "text_x": text_x})
            y += COMMIT_ROW_H
        elif kind == "divider":
            laid.append({"row": row, "y": y, "h": 30})
            y += 30
        elif kind == "section":
            laid.append({"row": row, "y": y, "h": 34})
            y += 34
        elif kind == "bar":
            laid.append({"row": row, "y": y, "h": LINE_H + 10})
            y += LINE_H + 10
        else:
            pieces = wrap_px(row.get("text") or " ", fonts["small"], inner_w)
            h = LINE_H * len(pieces)
            laid.append({"row": row, "y": y, "h": h, "pieces": pieces})
            y += h

    return {"card": card, "laid": laid, "height": y + 12, "inner_l": inner_l, "inner_r": inner_r}


def bezier_points(p0, p1, p2, p3, steps: int = 32):
    points = []
    for i in range(steps + 1):
        t = i / steps
        mt = 1 - t
        x = mt**3 * p0[0] + 3 * mt * mt * t * p1[0] + 3 * mt * t * t * p2[0] + t**3 * p3[0]
        y = mt**3 * p0[1] + 3 * mt * mt * t * p1[1] + 3 * mt * t * t * p2[1] + t**3 * p3[1]
        points.append((x, y))
    return points


def draw_card(img, draw: ImageDraw.ImageDraw, x: int, y: int, w: int, layout: dict, fonts: dict, t: dict) -> None:
    card = layout["card"]
    ch = layout["height"]
    inner_l = x + layout["inner_l"]
    inner_r = x + layout["inner_r"]
    header_h = card_header_h()
    rounded_rect(draw, (x, y, x + w, y + ch), CARD_BG, BORDER, radius=12)
    rounded_rect(draw, (x, y, x + w, y + header_h), HEADER_BG, BORDER, radius=12)
    draw.rectangle((x, y + header_h - 12, x + w, y + header_h), fill=HEADER_BG)

    strip_w = 6
    strip_h = int(ch)
    strip = Image.new("RGB", (strip_w, max(strip_h, 1)), card["accent"])
    shape = Image.new("L", (max(int(w), 1), max(strip_h, 1)), 0)
    ImageDraw.Draw(shape).rounded_rectangle((0, 0, int(w) - 1, strip_h - 1), radius=12, fill=255)
    img.paste(strip, (int(x), int(y)), shape.crop((0, 0, strip_w, strip_h)))

    badge = card["badge"]
    bw = 18 + int(rich_len(badge, fonts["small"]))
    bx0, by0 = x + 18, y + 14
    rounded_rect(draw, (bx0, by0, bx0 + bw, by0 + 22), card["accent"], card["accent"], radius=6)
    rich_draw(draw, bx0 + 8, by0 + 3, badge, PAGE_BG, fonts["small"])
    title_px = inner_r - (bx0 + bw + 12)
    rich_draw(draw, bx0 + bw + 12, y + 12, fit_text(card["title"], fonts["regular"], title_px), FG, fonts["regular"])
    if card["meta"]:
        rich_draw(draw, bx0 + bw + 12, y + 32, fit_text(card["meta"], fonts["small"], title_px), MUTED, fonts["small"])

    graph = card.get("graph")
    if graph:
        ss = 3
        commits = [it for it in layout["laid"] if it["row"]["kind"] == "commit"]
        y_by_index = {it["row"]["index"]: it["y"] for it in commits}

        def lane_x(lane: int) -> float:
            return inner_l + GRAPH_LEFT + lane * LANE_W

        overlay_w = layout["inner_l"] + GRAPH_LEFT + graph["lane_count"] * LANE_W + 14
        overlay = Image.new("RGBA", (int(overlay_w * ss), int(ch * ss)), (0, 0, 0, 0))
        od = ImageDraw.Draw(overlay)

        def ox(value: float) -> float:
            return (value - x) * ss

        def oy(value: float) -> float:
            return (value - y) * ss

        def oedge(x1: float, y1: float, x2: float, y2: float, color: str) -> None:
            if abs(x1 - x2) < 0.5 or y2 - y1 < 2:
                od.line((ox(x1), oy(y1), ox(x2), oy(y2)), fill=color, width=2 * ss)
                return
            span = min(float(COMMIT_ROW_H), y2 - y1)
            y_curve = y2 - span
            if y_curve > y1 + 0.5:
                od.line((ox(x1), oy(y1), ox(x1), oy(y_curve)), fill=color, width=2 * ss)
            mid = (y_curve + y2) / 2
            points = [(ox(px), oy(py)) for px, py in bezier_points((x1, y_curve), (x1, mid), (x2, mid), (x2, y2))]
            od.line(points, fill=color, width=2 * ss, joint="curve")

        for ci, cl, pi, pl in graph["edges"]:
            if ci not in y_by_index or pi not in y_by_index:
                continue
            oedge(
                lane_x(cl),
                y + y_by_index[ci] + COMMIT_DOT_Y,
                lane_x(pl),
                y + y_by_index[pi] + COMMIT_DOT_Y,
                PALETTE[cl % len(PALETTE)],
            )
        if commits:
            first = commits[0]
            fx = lane_x(first["row"]["lane"])
            od.line(
                (ox(fx), oy(y + first["y"] - 8), ox(fx), oy(y + first["y"] + COMMIT_DOT_Y)),
                fill=PALETTE[first["row"]["lane"] % len(PALETTE)],
                width=2 * ss,
            )
        edge_sources = {ci for ci, _, _, _ in graph["edges"]}
        for item in commits:
            row = item["row"]
            node_x = lane_x(row["lane"])
            dot_y = y + item["y"] + COMMIT_DOT_Y
            r = 6 if row["head"] else 5
            if row.get("continues") and row["index"] not in edge_sources:
                od.line(
                    (ox(node_x), oy(dot_y)),
                    (ox(node_x), oy(dot_y + COMMIT_ROW_H * 0.55)),
                    fill=row["color"],
                    width=2 * ss,
                )
            if row["head"]:
                od.ellipse(
                    (round(ox(node_x - r - 3)), round(oy(dot_y - r - 3)), round(ox(node_x + r + 3)), round(oy(dot_y + r + 3))),
                    outline=row["color"],
                    width=2 * ss,
                )
            od.ellipse(
                (round(ox(node_x - r)), round(oy(dot_y - r)), round(ox(node_x + r)), round(oy(dot_y + r))),
                fill=row["color"],
            )
        overlay = overlay.resize((int(overlay_w), int(ch)), Image.LANCZOS)
        img.paste(overlay, (int(x), int(y)), overlay)

    gutter_w = 40 * 2 + 18
    for item in layout["laid"]:
        row = item["row"]
        cy = y + item["y"]
        kind = row["kind"]
        if kind == "diff":
            pieces = item["pieces"]
            block_h = item["h"]
            draw.rectangle((inner_l - 8, cy - 1, inner_r + 8, cy + block_h - 1), fill=row.get("bg", CARD_BG))
            draw.line((inner_l + gutter_w, cy - 1, inner_l + gutter_w, cy + block_h - 1), fill=BORDER, width=1)
            old_color = DEL if row.get("sign") == "-" else NUM_FG
            new_color = ADD if row.get("sign") == "+" else NUM_FG
            old_val = row.get("old") or ""
            new_val = row.get("new") or ""
            mono = fonts["small"][0]
            draw.text((inner_l + 4, cy + 2), f"{old_val:>4}" if old_val else "   ·", fill=old_color, font=mono)
            draw.text((inner_l + 46, cy + 2), f"{new_val:>4}" if new_val else "   ·", fill=new_color, font=mono)
            rich_draw(draw, inner_l + gutter_w + 6, cy + 1, row.get("sign", " "), row["color"], fonts["regular"])
            tx = x + item["text_x"]
            for i, piece in enumerate(pieces):
                rich_draw(draw, tx, cy + i * LINE_H + 1, piece, row["color"], fonts["regular"])
        elif kind == "divider":
            lane_count = card.get("graph", {}).get("lane_count", 1)
            x0 = inner_l + GRAPH_LEFT + lane_count * LANE_W + 12
            label = row["text"]
            lw = rich_len(label, fonts["small"])
            rich_draw(draw, x0, cy + 7, label, row.get("color", MUTED), fonts["small"])
            if x0 + lw + 10 < inner_r:
                draw.line((x0 + lw + 10, cy + 15, inner_r, cy + 15), fill=BORDER, width=1)
        elif kind == "commit":
            tx = x + item["text_x"]
            rich_draw(draw, tx, cy + 3, item["subject"], FG, fonts["regular"])
            rich_draw(draw, tx, cy + 25, item["meta"], LINK if row["refs"] else MUTED, fonts["small"])
        elif kind == "section":
            band_top = cy + 4
            draw.rounded_rectangle((inner_l - 8, band_top, inner_r + 8, band_top + 24), radius=6, fill=HEADER_BG)
            tw = rich_len(row["text"], fonts["small"])
            rich_draw(draw, (inner_l + inner_r) / 2 - tw / 2, band_top + 5, row["text"], row.get("color", LINK), fonts["small"])
        elif kind == "bar":
            track_x0 = inner_l + 138
            track_x1 = inner_r - 64
            mid = cy + (LINE_H + 10) / 2 - 5
            rich_draw(draw, inner_l, cy + 2, fit_text(row["text"], fonts["small"], 120), FG, fonts["small"])
            draw.rounded_rectangle((track_x0, mid, track_x1, mid + 10), radius=5, fill=TRACK_BG)
            fill_w = int((track_x1 - track_x0) * row["ratio"])
            draw.rounded_rectangle(
                (track_x0, mid, track_x0 + max(fill_w, 6 if row["ratio"] > 0 else 0), mid + 10),
                radius=5,
                fill=row["color"],
            )
            vw = rich_len(row["value"], fonts["small"])
            rich_draw(draw, inner_r - vw, cy + 2, row["value"], row["color"], fonts["small"])
        else:
            pieces = item["pieces"]
            block_h = item["h"]
            for i, piece in enumerate(pieces):
                rich_draw(draw, inner_l, cy + i * LINE_H + 1, piece, row.get("color", MUTED), fonts["small"])


def rounded_rect(draw: ImageDraw.ImageDraw, box, fill, outline, radius=10, width=1):
    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def render(lang: str, out_path: str, fonts: dict, data: dict) -> None:
    t = STRINGS[lang]
    left_cards: list[dict] = []
    right_cards: list[dict] = []
    footer = ""
    if not data["parent_ok"]:
        left_cards.append(
            {
                "title": t["no_prev"],
                "badge": t["info"],
                "accent": MUTED,
                "meta": "",
                "rows": [{"kind": "meta", "text": t["nothing"], "color": MUTED}],
            }
        )
        right_cards.append(history_card(t, data))
    else:
        left_cards = [file_card(status, path, old_path, t, data) for status, path, old_path in data["entries"]]
        right_cards.append(history_card(t, data))
        right_cards.append(stats_card(t, data))
        right_cards.append(top_files_card(t, data))
        right_cards.append(dirs_card(t, data))
        if data["added"] or data["deleted"]:
            footer = t["summary"].format(files=data["files"], added=data["added"], deleted=data["deleted"])
        else:
            footer = t["summary_lines"].format(files=data["files"])

    left_w = WIDTH - PAD * 2 - SIDEBAR_W - GAP
    left_x = PAD
    right_x = PAD + left_w + GAP

    mark = time.perf_counter()
    left_layouts = [layout_card(c, left_w, fonts, t) for c in left_cards]
    right_layouts = [layout_card(c, SIDEBAR_W, fonts, t) for c in right_cards]
    print(f"[render]   layout {time.perf_counter() - mark:.1f}s", flush=True)
    left_h = sum(lay["height"] for lay in left_layouts) + GAP * max(len(left_layouts) - 1, 0)
    right_h = sum(lay["height"] for lay in right_layouts) + GAP * max(len(right_layouts) - 1, 0)

    head_rows: list[tuple[str, str, tuple, int]] = [(FG, t["title"], fonts["big"], 40)]
    for piece in wrap_px(f"{data['sha']}  {data['subject']}", fonts["regular"], left_w + SIDEBAR_W + GAP):
        head_rows.append((FG, piece, fonts["regular"], 24))
    meta = "  ·  ".join(part for part in (data["author"], data["date"]) if part)
    if meta:
        head_rows.append((LINK, meta, fonts["small"], 20))
    for body_line in data["body"].splitlines():
        for piece in wrap_px(body_line, fonts["small"], left_w + SIDEBAR_W + GAP):
            head_rows.append((MUTED, piece, fonts["small"], 20))
    head_h = sum(row_h for _, _, _, row_h in head_rows) + 14

    body_h = max(left_h, right_h)
    height = PAD + head_h + body_h + PAD + (28 if footer else 0)
    img = Image.new("RGB", (WIDTH, height), PAGE_BG)
    draw = ImageDraw.Draw(img)

    hy = PAD
    for color, text, pair, row_h in head_rows:
        rich_draw(draw, PAD, hy, text, color, pair)
        hy += row_h
    draw.line((PAD, hy + 6, WIDTH - PAD, hy + 6), fill=BORDER, width=1)

    ly = PAD + head_h
    mark = time.perf_counter()
    for lay in left_layouts:
        draw_card(img, draw, left_x, ly, left_w, lay, fonts, t)
        ly += lay["height"] + GAP
    print(f"[render]   draw-left {time.perf_counter() - mark:.1f}s", flush=True)
    ry = PAD + head_h
    mark = time.perf_counter()
    for lay in right_layouts:
        draw_card(img, draw, right_x, ry, SIDEBAR_W, lay, fonts, t)
        ry += lay["height"] + GAP
    print(f"[render]   draw-right {time.perf_counter() - mark:.1f}s", flush=True)

    if footer:
        rich_draw(draw, PAD, height - PAD - 8, footer, MUTED, fonts["small"])
    img.save(out_path, "PNG")


def main() -> None:
    os.makedirs("diff", exist_ok=True)
    mark = time.perf_counter()
    fonts = {"regular": load_pair(15), "small": load_pair(13), "big": load_pair(26)}
    print(f"[render] fonts {time.perf_counter() - mark:.1f}s", flush=True)
    mark = time.perf_counter()
    data = collect_data()
    print(f"[render] collect {time.perf_counter() - mark:.1f}s", flush=True)
    for lang in ("zh", "en"):
        mark = time.perf_counter()
        render(lang, f"diff/diff.{lang}.png", fonts, data)
        print(f"[render] draw {lang} {time.perf_counter() - mark:.1f}s", flush=True)


if __name__ == "__main__":
    main()
