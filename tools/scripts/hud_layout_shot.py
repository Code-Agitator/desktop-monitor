"""HUD 排版对比：把 hud.json 临时切到指定排版，抓一帧，再原样还原。

用法：
    python hud_layout_shot.py stacked docs/hud-stacked.png
    python hud_layout_shot.py inline  docs/hud-inline.png

抓的是 HUD 窗口区域（用 #303030 垫底），并逐格打印文字行段，
用来客观确认「上下排版=两行、左右排版=一行」。
"""
import io
import json
import os
import shutil
import subprocess
import sys
import time

CFG = os.path.join(os.environ.get("APPDATA", ""), "desktop-monitor", "hud.json")
HERE = os.path.dirname(os.path.abspath(__file__))
PROBE = os.path.join(HERE, "hud_probe.py")


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "inline"
    out = sys.argv[2] if len(sys.argv) > 2 else f"docs/hud-{mode}.png"
    if mode not in ("stacked", "inline"):
        sys.exit("mode 只能是 stacked / inline")

    with io.open(CFG, encoding="utf-8") as f:
        original = f.read()
    cfg = json.loads(original)
    try:
        for c in cfg["cards"]:
            opts = dict(c.get("options") or {})
            opts.pop("stacked", None)
            opts["layout"] = mode
            c["options"] = opts
        with io.open(CFG, "w", encoding="utf-8") as f:
            json.dump(cfg, f, ensure_ascii=False, indent=2)
        time.sleep(2.5)  # 等文件监听到热重载
        print(f"== {mode} ==")
        subprocess.run([sys.executable, PROBE, "--out", out], check=False)
    finally:
        with io.open(CFG, "w", encoding="utf-8") as f:
            f.write(original)
        time.sleep(1.5)
    print("配置已还原")


if __name__ == "__main__":
    main()
