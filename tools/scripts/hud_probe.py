"""HUD 排版探针：给 HUD 垫一块纯色底、按窗口矩形截图，然后逐格量出
标签/数值两行文字的实际像素高度与行间空隙（用于客观验收 HUD 排版）。

用法：
    python hud_probe.py --out docs/hud-probe.png
    python hud_probe.py --out docs/hud-probe.png --bg 0x303030 --json

依赖：Pillow（打包运行时自带）。窗口坐标按物理像素取，进程已声明 DPI 感知。
"""
import argparse
import ctypes
import json
import os
import sys
import time
from ctypes import wintypes

user32 = ctypes.windll.user32
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)  # PER_MONITOR_AWARE_V2
except Exception:
    user32.SetProcessDPIAware()

from PIL import Image, ImageGrab  # noqa: E402

HUD_TITLE = "desktop-monitor HUD"
HUD_PAD_X = 14  # ui/src/hudLayout.ts
HUD_PAD_Y = 4
TEXT_MIN = 120  # 文字像素亮度阈值（卡片底 ≈14，垫底 ≈48）


class RECT(ctypes.Structure):
    _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long),
                ("right", ctypes.c_long), ("bottom", ctypes.c_long)]


def hud_rect():
    """按标题前缀找 HUD 窗口（页面可能把 DIAG 数据写进标题）。"""
    hits = []

    def cb(h, _):
        n = user32.GetWindowTextLengthW(h)
        if n > 0:
            buf = ctypes.create_unicode_buffer(n + 1)
            user32.GetWindowTextW(h, buf, n + 1)
            # 页面把实测数据写进标题后，标题不再是固定的 HUD_TITLE
            if HUD_TITLE in buf.value or "HUD-DIAG" in buf.value:
                hits.append(h)
        return True

    user32.EnumWindows(ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)(cb), 0)
    if not hits:
        sys.exit("找不到 HUD 窗口（应用没在运行？）")
    hwnd = hits[0]
    r = RECT()
    if not user32.GetWindowRect(hwnd, ctypes.byref(r)):
        sys.exit("GetWindowRect 失败")
    return hwnd, (r.left, r.top, r.right, r.bottom)


def hud_config_path():
    return os.path.join(os.environ.get("APPDATA", ""), "desktop-monitor", "hud.json")


def load_cards():
    try:
        with open(hud_config_path(), "r", encoding="utf-8") as f:
            cfg = json.load(f)
        return cfg.get("cards", []), cfg.get("window", {})
    except Exception as e:  # 配置文件读不到时退回纯像素分析
        print(f"[warn] 读不到 hud.json：{e}", file=sys.stderr)
        return [], {}


def make_backdrop(rect, color):
    """在 HUD 下方铺一块纯色窗，避免壁纸亮度干扰阈值。"""
    import tkinter as tk
    l, t, r, b = rect
    m = 24
    root = tk.Tk()
    root.overrideredirect(True)
    root.geometry(f"{r - l + 2 * m}x{b - t + 2 * m}+{l - m}+{t - m}")
    root.configure(bg=color)
    root.attributes("-topmost", False)
    root.update_idletasks()
    root.update()
    return root


def luma(px):
    return (px[0] + px[1] + px[2]) / 3.0


def bands_in(img, box):
    """返回 box 内所有「有文字像素」的连续行段（含每段的列范围）。"""
    x0, y0, x1, y1 = box
    w, h = x1 - x0, y1 - y0
    rows = []
    for y in range(h):
        cols = [x for x in range(w) if luma(img.getpixel((x0 + x, y0 + y))) > TEXT_MIN]
        rows.append(cols)
    out = []
    cur = None
    for y, cols in enumerate(rows):
        if cols:
            if cur is None:
                cur = {"y0": y, "y1": y + 1, "x0": min(cols), "x1": max(cols) + 1, "n": len(cols)}
            else:
                cur["y1"] = y + 1
                cur["x0"] = min(cur["x0"], min(cols))
                cur["x1"] = max(cur["x1"], max(cols) + 1)
                cur["n"] = max(cur["n"], len(cols))
        elif cur is not None:
            out.append(cur)
            cur = None
    if cur is not None:
        out.append(cur)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="docs/hud-probe.png")
    ap.add_argument("--bg", default="#303030")
    ap.add_argument("--no-backdrop", action="store_true")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--settle", type=float, default=0.7)
    args = ap.parse_args()

    cards, win = load_cards()
    hwnd, rect = hud_rect()
    l, t, r, b = rect
    W, H = r - l, b - t

    root = None
    if not args.no_backdrop:
        root = make_backdrop(rect, args.bg)
    time.sleep(args.settle)
    if root is not None:
        root.update()

    img = ImageGrab.grab(bbox=(l, t, r, b), all_screens=True).convert("RGB")
    if root is not None:
        root.destroy()

    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    img.save(args.out)

    report = {
        "out": args.out,
        "rect": {"x": l, "y": t, "w": W, "h": H},
        "scale": user32.GetDpiForSystem() / 96.0,
        "cells": [],
    }
    for c in cards:
        cx = HUD_PAD_X + c.get("x", 0)
        cy = HUD_PAD_Y + c.get("y", 0)
        cw = c.get("w") or 0
        ch = c.get("h") or 0
        box = (int(round(cx)), int(round(cy)), int(round(cx + cw)), int(round(cy + ch)))
        box = (max(0, box[0]), max(0, box[1]), min(W, box[2]), min(H, box[3]))
        bands = bands_in(img, box)
        entry = {
            "id": c.get("id"),
            "label": (c.get("options") or {}).get("label"),
            "cell": {"x": c.get("x"), "y": c.get("y"), "w": cw, "h": ch},
            "box": box,
            "bands": bands,
        }
        if len(bands) >= 2:
            entry["row_gap"] = bands[1]["y0"] - bands[0]["y1"]
        report["cells"].append(entry)

    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return

    print(f"HUD {W}x{H} @ ({l},{t})  scale={report['scale']:.2f}  -> {args.out}")
    for e in report["cells"]:
        bs = e["bands"]
        desc = " | ".join(
            f"y{b['y0']}-{b['y1']} h={b['y1'] - b['y0']} x={b['x0']}-{b['x1']}(w={b['x1'] - b['x0']})"
            for b in bs
        )
        gap = e.get("row_gap")
        print(f"  {e['id']:<10} cell={e['cell']['w']}x{e['cell']['h']}  bands={len(bs)}  {desc}"
              + (f"  gap={gap}" if gap is not None else ""))
        if len(bs) >= 1:
            wide = bs[-1]["x1"] - bs[-1]["x0"]
            if wide >= e["cell"]["w"] - 2:
                print(f"    [warn] 数值横向贴边/溢出：宽 {wide} / 单元格 {e['cell']['w']}")
        if len(bs) > 2:
            print("    [warn] 单元格内出现多行文字（可能换行）")


if __name__ == "__main__":
    main()
