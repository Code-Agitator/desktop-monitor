"""生成应用图标：深色圆角方块 + 青色圆环 + 三根柱状图。"""

import math
from PIL import Image, ImageDraw

S = 1024
bg = (18, 20, 24, 255)
accent = (90, 200, 255, 255)
muted = (150, 158, 172, 255)

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# 圆角底板
d.rounded_rectangle([24, 24, S - 24, S - 24], radius=210, fill=bg, outline=(40, 44, 52, 255), width=10)

# 外环（留个缺口，像仪表盘）
pad = 250
d.arc([pad, pad, S - pad, S - pad], start=200, end=340, fill=accent, width=54)
d.arc([pad, pad, S - pad, S - pad], start=-20, end=40, fill=accent, width=54)
d.arc([pad, pad, S - pad, S - pad], start=60, end=170, fill=(70, 78, 92, 255), width=54)

# 中间三根柱子
bar_w = 78
gap = 42
base_y = 690
heights = [150, 260, 360]
total = len(heights) * bar_w + (len(heights) - 1) * gap
x = (S - total) / 2
for i, h in enumerate(heights):
    top = base_y - h
    color = accent if i == len(heights) - 1 else muted
    d.rounded_rectangle([x, top, x + bar_w, base_y], radius=26, fill=color)
    x += bar_w + gap

img.save(r"D:\Work\desktop-monitor\src-tauri\icons\source.png")
print("saved source.png", img.size)
