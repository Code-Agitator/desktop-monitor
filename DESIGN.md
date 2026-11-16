# desktop-monitor 设计规格 v1

Windows 11 桌面壁纸层常驻监控面板。透明叠加在**现有壁纸之上、桌面图标之下**；指标来自内置系统采集与**进程外插件**；展示由「带类型的度量 + 渲染器注册表」驱动。

## 1. 已定决策速查

| # | 议题 | 决定 |
|---|---|---|
| 1 | 定位 | 自用优先；插件协议按「将来可公开」的标准设计，安装器/签名/更新器一律后置 |
| 2 | 壁纸层语义 | 透明叠加层（A），保留「自带背景」开关 |
| 3 | 交互 | 壁纸层纯只读、不收鼠标事件；所有配置在独立设置窗口完成 |
| 4 | 技术栈 | Rust + Tauri；前端 Vue 3 + Vite |
| 5 | 插件模型 | 进程外子进程，stdin/stdout NDJSON + JSON-RPC 2.0；另有声明式插件暂缓 |
| 6 | 权限 | v1 整个主程序以管理员常驻；采集层与 UI 层在代码上物理隔离 |
| 7 | 面板范围 | v1 = 桌面层 + 置顶 HUD；任务栏挂件 v2 再做（贴在任务栏上方的置顶条）；锁屏不碰 |
| 8 | 多显示器 | 数据结构从第一天按每屏独立面板设计，以 monitor_id 为键；v1 只保证主屏开箱可用 |
| 9 | 指标抽象 | Metric 带 kind（scalar/ratio/bytes/duration/state/series/list）+ 展示提示；核心维护渲染器注册表，声明各自兼容的 kind |
| 10 | 插件与 UI | 插件只产数据不产 UI；渲染 100% 在核心侧。逃生舱：受控 `custom_html` 渲染器（iframe 沙箱），v1 不做 |
| 11 | 布局 | 自由画布 + 吸附网格；编辑在设置窗口内完成，保存即热重载 |
| 12 | 调度 | 指标级 interval，统一调度器按最近到期驱动；失败指数退避（最高 60s） |
| 13 | 历史 | 仅内存环形缓冲，默认 5 分钟窗口；插件只回当前值，核心负责累积 |
| 14 | 降频 | 锁屏/熄屏全部暂停，恢复时补采一次；v1 不做电池供电策略（台式机） |
| 15 | 采集来源 | 硬件传感器走 .NET + LibreHardwareMonitorLib 的内置插件（AMD RX 7800 XT，NVML 不可用） |
| 16 | 配置形态 | 纯文件（TOML + JSON），手可编辑，文件监听热重载；设置窗口是这些文件的图形编辑器，不是唯一入口 |
| 17 | 插件包 | 一个插件 = 一个目录（manifest.json + 入口程序）；解压即安装；protocolVersion 主版本不匹配则禁用 |
| 18 | SDK | 先发 TypeScript 版；进程外 + JSON 协议天然不限语言 |
| 19 | 设备 | v1 只做通用 BLE/HID 电量；罗技 HID++ 等品牌协议作为 v1.5 旗舰插件 |
| 20 | 自启 | 计划任务（登录触发、最高权限），开机无 UAC 弹窗；托盘为唯一操作入口；单实例 |
| 21 | 崩溃策略 | 不自动拉起整个程序；插件进程由 supervisor 按退避重启，反复失败则禁用并通知 |
| 22 | 异常视觉 | 过期=变暗+小圆点；失效=降透明度+「值 · 3m 前」；壁纸层永不弹通知 |
| 23 | 打包 | `cargo build --release` + `setup.ps1` 便携安装；不做安装器/更新器/Release |
| 24 | 本地化 | 界面中文，代码/配置键/日志英文；数值 GiB 标 GB，网络 MB/s |
| 25 | 资源预算 | 空闲 CPU < 1%、常驻内存 < 250MB（含 WebView2 与采集插件）；无持续动画 |
| 26 | 虚拟显示器 | ToDesk/向日葵虚拟屏默认可折叠、需手动启用；未配置的屏默认不显示内容 |

## 2. 进程与窗口模型

```
monitor-host.exe  (Rust / Tauri，管理员，计划任务启动，单实例)
├─ WebView2 "panel"      → SetParent 到 WorkerW（壁纸层，透明，点击穿透）
├─ WebView2 "settings"   → 普通窗口（编辑配置，按需打开）
├─ Scheduler / MetricStore / ConfigWatcher / Tray
└─ 插件子进程 × N
   └─ sys-collector.exe  (.NET 8 自包含单文件 + LibreHardwareMonitorLib)
```

- 壁纸层窗口：`WS_CHILD` + `WS_EX_NOACTIVATE`，透明背景，不改壁纸本身。
- 挂载：向 `Progman` 发 `0x052C` 让 Explorer 生成 WorkerW，枚举出「无 SHELLDLL_DefView 子窗口」的那个 WorkerW 作为父窗口；子窗口尺寸跟随 WorkerW。
- 自愈：监听 `TaskbarCreated`（`RegisterWindowMessage`），Explorer 重启后重新发现并挂载，目标 3 秒内恢复。
- 点击穿透：子窗口 + `WS_EX_TRANSPARENT`，不注册任何命中处理。
- DPI：PerMonitorV2；画布坐标用 CSS px，按屏缩放。
- HUD：普通透明 `WS_EX_TOPMOST` 窗口，检测到全屏应用即隐藏，记住位置。
- 退出时销毁所有窗口，不残留透明层。

## 3. 数据模型

### 3.1 Metric kind

| kind | 载荷 | 适用的内置渲染器 |
|---|---|---|
| `scalar` | 数值 + unit | 数字、环形（需 min/max）、横向条、文本 |
| `ratio` | 0..1 | 环形、横向条、电池、百分比数字 |
| `bytes` | 字节数 | 数字（自动 GiB）、环形、横向条 |
| `duration` | 秒 | 数字（人性化） |
| `state` | 字符串 + severity | 状态点、徽章、文本 |
| `series` | 数值序列 | 迷你折线、折线图、柱状 |
| `list` | 行数组 {label, value, kind?} | 列表、逐行条 |

每个指标可带展示提示：`unit`、`min`/`max`、`preferredView`、`severity` 阈值、`icon`、`decimals`。

### 3.2 渲染器注册表

渲染器声明 `acceptedKinds` 与所需 option；设置窗口只列出与所选指标兼容的渲染器，避免配置后无法显示。

### 3.3 面板配置（每屏一份）

```json
{
  "version": 1,
  "monitor": { "id": "DISPLAY\\XMI2701\\7&2119f498&1&UID768_0", "name": "Mi Monitor", "width": 2560, "height": 1440, "scale": 1.0 },
  "theme": "default-dark",
  "background": null,
  "cards": [
    {
      "id": "cpu-temp",
      "x": 64, "y": 80, "w": 220, "h": 120,
      "metric": "sys.cpu.temp",
      "view": "ring",
      "options": { "decimals": 0 },
      "group": "g-mem",
      "style": { "accent": "var(--accent-1)", "opacity": 0.85 }
    }
  ]
}
```

`background: null` 即透明叠加；填路径则为「自带背景」模式。卡片只引用主题 token，不写死颜色。

## 4. 插件协议 v1

### 4.1 manifest.json

```json
{
  "id": "sys-collector",
  "name": "系统采集",
  "version": "0.1.0",
  "protocolVersion": 1,
  "entry": { "command": "sys-collector.exe", "args": [] },
  "platforms": ["windows"],
  "timeoutMs": 5000
}
```

### 4.2 传输与生命周期

stdin/stdout 上 NDJSON，每条一行一个 JSON-RPC 2.0 消息；stderr 只进日志。

```
core → {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"config":{}}}
plug → {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,
          "plugin":{"id":"sys-collector","version":"0.1.0"},
          "metrics":[{"id":"sys.cpu.temp","title":"CPU 温度","kind":"scalar","unit":"°C","intervalMs":2000,"history":true}]}}

core → {"jsonrpc":"2.0","id":2,"method":"poll","params":{"ids":["sys.cpu.temp"],"ts":1730000000000}}
plug → {"jsonrpc":"2.0","id":2,"result":{"values":{"sys.cpu.temp":{"v":52,"ts":1730000000000}}}}

core → {"jsonrpc":"2.0","method":"setConfig","params":{"config":{}}}
core → {"jsonrpc":"2.0","method":"shutdown"}
```

- 指标值可为 `{"v": ...}`（按 kind 解释）或 `{"error":"device not connected"}`。
- `series` kind 允许一次返回数组，核心直接采用并停止自行累积。
- 插件可用无 id 的通知重报指标列表（设备热插拔）：`{"jsonrpc":"2.0","method":"metrics","params":{"metrics":[...]}}`；宿主按其 plugin id 整体替换指标定义并清理已移除指标的数据（协议 v1 的向后兼容扩展）。
- 超时：manifest `timeoutMs`，默认 5s；超时的 poll 结果丢弃。
- 指标级失败：该指标退避 1s→2s→4s…→60s，卡片进入过期/失效态。
- 进程级失败：连续 3 次异常或退出 → 重启（1s/5s/15s 退避）；10 分钟内 5 次重启 → 禁用并系统通知。
- 插件目录 `fs watch` 感知增删，支持「重新加载此插件」而不重启主程序。

## 5. 目录结构

```
desktop-monitor/
├─ DESIGN.md
├─ package.json                 # 根：@tauri-apps/cli 与脚本
├─ setup.ps1                    # 安装到 %LOCALAPPDATA%、注册计划任务、快捷方式
├─ src-tauri/                   # Tauri + Rust 主程序
│  ├─ src/
│  │  ├─ main.rs  app.rs  config.rs  metrics.rs  scheduler.rs  source.rs
│  │  ├─ overlay.rs             # 面板承载：置底透明窗口、点击穿透、保持置底
│  │  ├─ hud.rs                 # 置顶 HUD：位置记忆、全屏检测、启用开关
│  │  ├─ windows.rs             # 设置窗口
│  │  ├─ tray.rs  sink.rs  instance.rs
│  │  └─ plugins.rs             # 发现、manifest、supervisor、NDJSON 传输
│  ├─ capabilities/default.json
│  └─ tauri.conf.json
├─ ui/                          # Vue 3 + Vite（index / settings / hud 三个入口）
│  └─ src/{App.vue, HudApp.vue, Stage.vue, themes.ts, renderers/, settings/, hud/}
├─ plugins/builtin/sys-collector/   # .NET 8 + LibreHardwareMonitorLib（M4）
├─ plugins/builtin/device-battery/  # .NET 8 + WinRT BLE / HID 电量（M6）
└─ sdk/ts/                      # 插件 SDK（M3）
```

配置与运行时数据：

```
%APPDATA%\desktop-monitor\
├─ panel.json                   # 桌面面板（v1 单屏；每屏一份留到多屏）
├─ hud.json                     # HUD：window{x,y,w,h,enabled} + cards，主题同面板
├─ plugins\<plugin-id>\         # 插件；内置插件首次启动时复制进来
└─ （v1 主题内置在 ui/src/themes.ts；themes\<name>\theme.css 留到 v1.5 自定义主题）
%LOCALAPPDATA%\desktop-monitor\logs\   # 按天滚动，保留 7 天
```

## 6. 里程碑

| 里程碑 | 内容 | 肉眼可见的验收 |
|---|---|---|
| M1 面板宿主 ✅ | 置底透明覆盖层、点击穿透、托盘、单实例、计划任务脚本、假数据 IPC | 桌面出现卡片；Explorer 重启后面板不丢；点击穿透到桌面 |
| M2 指标内核 ✅ | kind 类型、渲染器注册表、调度器、面板配置与热重载、7 种渲染器 | 假数据渲染出环形/条/折线/状态/列表/文本 |
| M3 插件协议 ✅ | NDJSON JSON-RPC、supervisor、TS SDK、模拟插件（随机值/故意失败/崩溃） | 插件指标上墙；杀进程后按 1/5/15s 退避恢复，超过 5 次/10 分钟熔断 |
| M4 系统采集 ✅ | .NET + LHM 自包含插件，CPU/GPU/内存/磁盘/网络 | 真实指标上桌面（CPU 温度/频率受开发环境限制，见实测） |
| M5 设置窗口 ◐ | 画布编辑 + 属性面板 + 即时保存；插件管理/诊断暂由托盘承担 | 改配置 1 秒内生效，无需保存按钮 |
| M6 HUD + 主题 + 电量 ✅ | 置顶 HUD、全屏隐藏、主题切换、BLE/HID 电量插件 | HUD 可拖拽；蓝牙键鼠电量上墙（本机无标准上报设备，见实测） |
| v1 收尾 ✅ | 计划任务安装、设置内插件管理/诊断、验收清单 | 任务启动即面板 + 插件；9 项验收通过（见实测） |

**M1 实测（2026-11-05）**：`src-tauri` + `ui` 骨架已跑通。面板为 `HWND_BOTTOM` + `WS_EX_TRANSPARENT` + `WS_EX_NOACTIVATE` + `WS_EX_TOOLWINDOW` 的 Tauri 透明窗口；WebView2 透明合成正常。

- 杀 explorer.exe 并重启：面板进程与窗口均存活，数据继续刷新（覆盖层方案不依赖桌面合成）
- 点击穿透：点击卡片位置后前台窗口变为 `Progman`
- 单实例：二次启动进程立即退出，仅保留一个实例
- 空闲 CPU：0.55%（20 核总算，12s 均值）
- **空闲内存：373MB（主进程 46MB + 5 个 WebView2 进程）**，超出 §1 的 250MB 预算，待 M6 用 release 构建复测并优化（可尝试关闭 devtools、精简 WebView2 进程参数）
- 计划任务脚本 `setup.ps1` 已写好但尚未安装（避免开发期自启）

**M2 实测（2026-11-05）**：

- 指标内核：`metrics.rs`（7 种 kind + 环形历史缓冲）、`scheduler.rs`（指标级 interval、200ms 合批推送）、`source.rs`（12 个假指标）
- 渲染器注册表：`ui/src/renderers/index.ts`，7 个渲染器（数字/环形/条形/折线/状态/列表/文本），每个声明 `accepts` 与默认尺寸
- 配置：`%APPDATA%\desktop-monitor\panel.json`，首次运行自动生成；文件监听（notify，200ms 去抖）热重载，实测改标签/坐标后 3 秒内生效、无需重启
- 回退验证：未知 `view` 按 kind 退回兼容渲染器；未知 `metric` 显示 `--`
- 资源：主进程 46MB + WebView2 357MB = 403MB，仍高于 250MB 预算

**M3 实测（2026-11-05）**：

- 协议：stdin/stdout NDJSON + JSON-RPC 2.0（initialize / poll / setConfig / shutdown）；协议主版本不匹配的插件拒绝加载
- SDK：`sdk/ts/src/index.ts` 提供 `serve(plugin, info)`，插件只实现 `metrics` 与 `poll(ids)`
- 模拟插件：`plugins/mock`（随机值 / 计数器 / 每 3 次故意失败 / 状态文本），esbuild 打包成单文件后安装到插件目录
- 监管：poll 超时（manifest `timeoutMs`，默认 5s）；失败时**保留最后一次的值与时间戳**，卡片变暗 + 红边 + 右下角显示「4s 前」；进程退出按 1s / 5s / 15s 退避重启；10 分钟内 5 次重启则熔断禁用并保留失效数据；托盘提供「重新加载插件」「打开插件目录」
- 目录监听：notify 400ms 去抖，增删插件无需重启宿主
- 实测：杀死 node 插件进程 → 卡片进入失效态并显示时长，约 1s 后自动恢复；连续杀死触发 5s / 15s 退避；第 6 次杀死后熔断，20s 内未再拉起
- 修过的两个真实问题：① 插件在 UI 挂监听之前注册指标会丢 def，改为先挂监听再取 bootstrap；② 重启计数被每次成功启动清空，导致退避阶梯失效，已改为 10 分钟滑动窗口

**M4 实测（2026-11-05）**：

- `plugins/builtin/sys-collector`：.NET 8 + LibreHardwareMonitorLib 0.9.4，自包含单文件 68MB，走与模拟插件相同的协议（19 个指标）
- 真实数据已上桌面：CPU 使用率/每核负载、内存占用/已用/提交、GPU 使用率/温度/功耗/显存/风扇、磁盘分区、磁盘读写、网络上下行、开机时长、状态
- 本机硬件：Intel i5-13600KF（14 核）+ AMD RX 7800 XT；LHM 传感器名与预期一致（CPU Total / CPU Package / GPU Core 等）
- 插件在「传感器读不到」时**不声明该指标**（卡片显示 `--`），不会用 0 误导；状态里标注「传感器正常/受限」
- **已知环境限制**：在 Claude 桌面应用（MSIX 打包）的开发环境里，由宿主启动的插件子进程无法通过 LHM 打开 ring0 驱动，导致 CPU 温度/频率不可读；同一 exe 由 bash 直接启动则正常。已用 A/B 实验确认与「立即父进程」有关（新建计划任务启动被权限策略拦截，未能在包外验证）。正式部署（资源管理器/计划任务启动）需实测确认
- 宿主侧修复：插件入口按插件目录解析（Command 不用 current_dir 解析程序路径）；initialize 用 15s 独立超时，失败时回收子进程；rescan 幂等（避免目录事件高频触发产生多个监管线程）

**M5 实测（2026-11-05）**：

- 设置窗口：托盘「设置…」或二次启动唤起；独立 Tauri 窗口（1180×800），不遮挡面板
- 布局：左侧卡片列表（含渲染器名）、中间按屏幕比例缩放的真实数据画布、右侧属性面板
- 属性面板：指标（全量列表）、渲染器（**只列与当前 kind 兼容的**）、标签、X/Y/宽/高、删除
- 编辑即保存：改动经 `save_panel_config` 落盘 → 文件监听 → 面板热重载；无保存按钮
- 撤销：本地快照栈（上限 50），Ctrl+Z
- **已自动化验证**：通过 UI Automation 点击「新增」→ 配置文件 15→16 张卡 → 面板出现新卡片；通过 Spinner 模式把 X 设为 300 → 落盘并热重载
- 未机器验证：画布内的拖拽/缩放（指针交互），代码已实现，需人工确认
- 冲突保护：`save` 前比对「本进程最后读到的文本」与磁盘内容，不一致则拒绝覆盖并让前端重新载入（防止手工编辑与设置窗口同时保存时静默覆盖）

**M6 实测（2026-11-06）**：

- **HUD**：`WS_EX_TOPMOST` + `WS_EX_NOACTIVATE` + `WS_EX_TOOLWINDOW` 的透明窗口，默认 340×140 放右上角；CPU/GPU 环形 + 内存条 + 电量列表。整面可拖拽（`data-tauri-drag-region`，卡片 `pointer-events: none`）
- 拖拽记忆：合成鼠标拖动 150×100 后窗口到 (910,480)，`hud.json` 同步为 `x=910,y=480`；外部改 `hud.json` 也会在 ~1s 内应用
- 全屏隐藏：前台窗口铺满所在显示器（排除桌面/任务栏/自身窗口）即隐藏，实测 46–256ms 隐藏、退出全屏 140–225ms 恢复；隐藏用 Win32 `ShowWindow(SW_HIDE)`（Tauri `hide()` 对这类窗口不生效）
- **设置窗口双面**：顶部切换「桌面面板 / 置顶 HUD」；HUD 面可改启用开关、窗口 X/Y/宽/高（改宽 340→380 即时生效并落盘）；三套内置主题（暗色玻璃/午夜蓝/浅色磨砂）按面保存、即时热更新
- **主题**：`ui/src/themes.ts` 里是纯 CSS 变量（`--accent/--text/--card-bg/--track/...`），卡片与渲染器只引用变量；面板、HUD、设置预览共用一套 token
- **电量插件** `device-battery`：三条数据源——Windows 设备属性 `System.Devices.BatteryLife`、BLE GATT 0x180F/0x2A19、HID 标准 Battery Strength（Usage Page 0x06 / Usage 0x20，逐 Feature 报表用 `HidP_GetUsageValue` 探测）；60s 采集；设备集合变化时用协议 `metrics` 通知重报指标（每设备一个 `ratio` 电量 + 一个 `state` 状态，外加 `battery.devices` 列表和 `battery.status`）；新增「电池」渲染器
- **硬件验证限制**：本机只有 OPPO Enco X3 耳机（Windows 不汇总其电量）与飞智 2.4G 鼠标（HID 只暴露厂商页 0xFF04，无标准电量），故没有真实设备可读数。用插件 `--demo` 参数造了一个 82% 的演示设备，验证了指标动态声明 → 面板/HUD 电池卡与列表的完整链路；正式设备（蓝牙键鼠）需接上后确认
- **资源**：release 空闲 652MB / 1.13%（20 核，60s 均值）。构成：WebView2 浏览器 151MB + GPU 88MB + 两个渲染器 ~160MB + 工具进程 60MB + SysCollector 66MB + DeviceBattery 55MB + 主进程 32MB。关掉面板卡片的过渡动画后 CPU 从 2.08% 降到 1.13%（透明全屏 WebView2 的持续重绘成本）；内存仍达不到原 250MB 预算，后续可做：HUD 关闭时销毁窗口、.NET 插件裁剪、单面模式
- **修过的真实问题**：① 同一目录上的两个 notify 监听导致 HUD 的那个收不到事件 → 合并为单监听线程；② 程序化移动窗口被当成用户拖拽，位置自我循环漂移 → 只在按住左键时才写回位置；③ 设置窗口在无实际改动的 pointerup 时也保存 → 只在真正拖动过时保存；④ `cargo build` 的 debug 二进制会去连 devUrl（localhost:1420），测试必须用 `pnpm tauri build --debug`
- 仍未机器验证：设置画布内的拖拽/缩放（与 M5 相同，需人工确认）

**v1 收尾实测（2026-11-06）**：

- **计划任务**：`setup.ps1` 增加 `-InstallDir`；脚本需保存为 UTF-8 BOM（Windows PowerShell 5.1 会把无 BOM 的中文按 ANSI 解码，导致字符串解析错误）。开发环境是 MSIX 包，`%LOCALAPPDATA%` 被重定向到包内缓存，任务调度器访问不到包内路径；本次验证装到 `C:\Program Files\desktop-monitor`。任务为登录触发、最高权限、交互式，`schtasks /Run` 手动触发成功（LastTaskResult 0x41301 = 正在运行）。
- **CPU 温度/频率问题已解决**：由计划任务启动的进程（包外、提权）下，sys-collector 声明 19 个指标，面板显示 CPU 温度 42–47°C、频率 3295MHz。M4 的「传感器受限」确认为开发环境父进程（MSIX）导致，正式部署没有该问题。
- **面板 Z 序修复**：Win+D（显示桌面）会把 `Progman` 抬到面板之上；原 `HWND_BOTTOM` 让面板落到壁纸下方——窗口 API 仍报 visible，但屏幕上看不到。现在每次保持置底时沿 Z 序找到 `Progman` 正上方的窗口，把面板插到两者之间（Progman 之上、普通窗口之下），找不到 Progman 才退回最底层。实测 Win+D 后面板正常可见，普通窗口（设置/微信）仍能盖住面板。
- **设置窗口新增「插件与诊断」页**：插件列表（名称/版本/状态/指标数/错误详情，2s 自动刷新）、重新加载全部、打开插件目录；诊断信息（配置目录、插件目录、指标总数、版本）；退出程序按钮。宿主新增 `list_plugins` / `reload_plugins` / `open_plugins_dir` / `get_diag` / `quit_app` 命令，插件管理器维护已发现 manifest 与最近状态。
- **验收清单（§8）**：
  1. 计划任务注册成功，手动运行拉起面板 + 两个插件（真实登录未注销验证，触发方式为 AtLogOn）。
  2. 杀 explorer.exe 并重启：面板/HUD 窗口存活，数据继续刷新。
  3. 点击穿透：Win+D 后点击卡片位置，前台窗口为 `Progman`。
  4. 设置改主题：面板 1.2s 内切换（截图 `docs/v1-hotreload.png`）。
  5. kill SysCollector：4s 内拉起新进程并恢复 running；卡片失效态在 M3 已验证。
  6. 第三方插件热插拔：mock 放入插件目录后 1s 内出现在设置插件页（4 指标，总数 26），删除后指标清理（dev 构建验证，与正式包同一份代码）。
  7. 10 分钟空闲：403MB / 1.26%（20 核、600s 均值）；构成：WebView2 ~300MB、SysCollector 41MB、DeviceBattery 34MB、主进程 17MB。CPU 主要花在透明全屏 WebView2 的 GPU 进程（87.6s/600s）。
  8. 全屏：HUD 251ms 隐藏、85ms 恢复。
  9. 退出程序：设置页「退出 desktop-monitor」→ 进程与插件全部退出（任务 LastTaskResult=0），桌面无残留透明窗口（docs/v1-exit.png）。
- **插件控制台弹窗修复**：release 是 GUI 子系统（无控制台），插件是控制台程序且 stderr 继承，Windows 会给每个插件分配一个可见控制台窗口（用户截图复现）。现在插件以 `CREATE_NO_WINDOW` 启动，stderr 改接管道并写入 `%LOCALAPPDATA%\desktop-monitor\logs\<plugin>.log`。实测无 ConsoleWindowClass 窗口，`sys-collector.log` 正常落盘。
- **HUD 背景修复**：HUD 卡片的大号 `box-shadow` 在浅色内容上会在卡片间隙叠出一整块灰色底（用户截图复现）。HUD 卡片改为 `box-shadow: none`，只保留卡片本身的半透明背景；用白色底窗复现验证（docs/hud-white-bg.png）。
- 已知限制：真实蓝牙键鼠电量仍未接设备验证；本机计划任务只能装到 Program Files（MSIX 包内环境所致），普通 PowerShell 里 `setup.ps1` 默认仍装到 `%LOCALAPPDATA%`。

**v1.1 设置增强（2026-11-06）**：

- **缩放手柄修复**：原来的手柄画在卡片内部，被卡片的 `overflow: hidden` 裁得只剩角落几个像素，几乎点不到。现在手柄移到共享舞台层（`Stage.vue`），独立于卡片、14px、始终绘制在选中卡片右下角；拖动按 8px 吸附修改宽高并即时落盘。实测把 CPU 卡从 170×170 拖到 440×344，`panel.json` 同步。
- **融合块**：`CardConfig.group`（可选字符串）。同组卡片渲染成一个视觉块——背景/边框/阴影只画一次，矩形取成员并集，成员之间以及成员与组边缘之间的间隙都包含在内；成员卡片自身背景透明。设置页卡片列表每行有勾选框，勾选多个后出现「融合 / 解除融合」工具条；融合组可以手工写 JSON（`"group": "g-mem"`），留空/`null` 即独立。
- 面板、HUD、设置预览共用同一个 `Stage.vue`，三处的融合与尺寸表现一致；HUD 的融合块同样不画阴影。
- 实测：勾选「内存 + 已用内存」融合，面板上显示为一块（截图 `docs/fuse-panel-zoom.png`），设置预览同步（`docs/fuse-preview.png`）；融合操作可 Ctrl+Z 撤销。

**数字渲染器自适应（2026-11-06）**：

- 问题：数字卡片的字号/内边距固定，HUD 里把卡片拖小后文字被裁没。
- 内边距随卡片尺寸收缩：`padY = clamp(5, 10%·h, 16)`、`padX = clamp(7, 9%·w, 18)`；卡片设为 CSS 尺寸容器（`container-type: size`）。
- 渲染器用容器查询 + `cqh` 单位缩放字号：大卡片保持「标签在上、数值在下」；内容盒高 ≤58px 或宽 ≤112px 时切换为单行「标签 + 数值居中」（参考 HUD 常见样式）；极窄（≤56px）只留数值；标签或数值过长时优先保证数值可读。
- 实测：80×48 显示 `CPU 17%`，60×40、44×40 显示居中的数值，150×84 恢复「CPU 温度 / 47.0°C」；带单位的字节值在 80×48 正常显示。截图 `docs/number-hud-80x48.png`、`docs/number-large.png`。
- **HUD 条形排版**：按设计稿的陶瓷排布把实际悬浮窗改成一条等宽的指标条——CPU / GPU / 内存 / 网络上行 四个单元格等宽（68px），标签在上、数值在下，两者同字号（随高度 25cqh，封顶 13px），无竖分割线；四张卡共用一个融合组，背景只画一块。数字渲染器新增 `options.stacked` 固定纵向排版，只在 HUD 卡片启用；主题、配色、字体族都不变。截图 `docs/hud-autosize.png`。网络速率文字偏长时会接近单元格边缘，后续可加紧凑格式。
- **HUD 宽高自适应**：HUD 页面按所有卡片的外接矩形（含渲染器默认尺寸）计算内容尺寸，变化时调用 `resize_hud` 调整窗口并写回 `window.w/h`；设置里的宽/高铁框已移除，改为显示「宽高随内容自动 · W × H」。窗口左上角位置不变，内容始终完整显示。窗口尺寸 = 内容 + 8px（四周各 4px 内边距），避免融合块边框贴边被窗口裁掉；设置预览同步留白。实测把内存卡右移 16px，窗口从 292×48 长到 308×48，移回后自动缩回 292×48。
- **HUD 单元格紧凑化**：HUD 卡片内边距改为 3/4px（面板卡片不变），`stacked` 单元格铺满整格高度，标签左上、数值右下、两者同字号；内容贴近边缘，减少中间无效留白。截图 `docs/hud-compact.png`。
- **HUD 数字排版重做（2026-11-08）**：上一版 `stacked` 把标签和数值设成同一字号（13px 封顶），实际观感是数字太小、标签与数值上下离得太远。现在改成真正的两级排版：
  - **字号**：`Card.vue` 把卡片内容盒的像素尺寸以 `--cw/--ch` 传给渲染器，`stacked` 直接按像素算字号——数值取内容高的 60%（封顶 32px），标签取 20%（封顶 11px）；两者再用「估算文字宽度」压一档，长数值（`1.5 MB/s`、`12.3 GB`）不会撑破格子。76×48 单元格实测数值 25px、标签 8.4px，数值墨迹高度从原来的 7px 涨到 ~18px。
  - **间距**：标签与数值的行间 gap 固定 1px、两者 `line-height: 1`，整组垂直居中（原来用 `space-between` 把两行顶到格子上下边缘，所以中间空出 20px）。实测标签墨迹底到数值墨迹顶只剩 5–7px（原来 21px）。
  - **数字格式**：`formatBytes` 拆成「数字 + 单位」两段（`formatBytesParts`），单位以 0.5em 小字号跟在数字后面，并去掉多余的尾零（`12.30 GB` → `12.3 GB`）；百分比去掉小数（`33%`）。条形、折线渲染器同步受益。
  - **默认 HUD 布局**：`default_hud_config` 改为等宽等高的 2×2 指标网格（4 × 76×48，窗口 160×104），CPU / 网络上行 在左列，GPU / 内存 在右列，四格共用一个融合组。
  - **量测工具**：`tools/scripts/hud_probe.py` 按 `hud.json` 的卡片矩形逐格扫描文字像素行段，输出每个单元格的「标签行高 / 数值行高 / 行间距」，用来客观验收排版。本次前后对比（76×48 格子）：标签墨迹 7px、数值墨迹 7px、行间 21px → 标签 7–9px、数值 19px、行间 5–7px；数值字号 13px → 26.9px。
  - **踩坑记录（重要）**：直接用 `cargo build --release` 构建时，Tauri 会把二进制当成 dev 模式，窗口去连 `localhost:1420` 的 Vite dev server；dev server 没开就只剩一片空白和「后端无法访问」——改了前端也看不到效果。**必须用 `pnpm tauri build --no-bundle`（或 `tools/scripts/build_install_run.ps1`）**，它才会把 `ui/dist` 打进 exe。另外 `build.rs` 补了 `cargo:rerun-if-changed=../ui/dist`，否则只改前端时 cargo 会跳过重编译。
- **HUD 第二种排版：左右（2026-11-08）**：数字渲染器新增 `options.layout`，取值 `"stacked"`（标签在上、数值在下）/ `"inline"`（标签在左、数值在右）；不写则由卡片尺寸自动决定，旧的 `options.stacked: true` 仍按 `stacked` 处理（设置窗口保存时会把它迁移成 `layout`）。
  - **左右排版规则**：整组水平居中、按基线对齐，标签 8–11px（随高度 `0.28×内容高`），数值 `0.62×内容高` 封顶 32px；数值宽度预算 = 内容宽 − min(标签估算宽, 内容宽×36%) − 6px 间隙，超出时标签省略号截断，**绝不把数值挤出格子**。
  - **设置窗口**：卡片属性里新增「排版」下拉（自动 / 上下·标签在上 / 左右·标签在左），只对数字渲染器显示，改完即时落盘热重载。
  - 实测（80×48 格子）：左右排版时标签与数值合成一行（像素行段从「两段」变成「一段」），数值墨迹 13–16px；网络上行 + `1.5 MB/s` 在 80px 宽度里偏挤，这类长标签/长数值建议把格子放宽或把标签缩短（如「上行」）。截图 `docs/hud-stacked.png` / `docs/hud-inline.png`。
  - **版面约束（数值变动不能带动排版）**：数值是每秒刷新的，所以字号和「标签占的位置」都必须跟具体数值无关——
    - **字号用最坏情况估宽**：`--den` 不再是当前文本的宽度，而是按 kind 取最坏样本——ratio 用 `100%`、bytes 用 `999.9` + 单位阶梯里最长的一档（`MB/s`、`GB`）、scalar 用 `999.9` + 单位；`9% → 13% → 100%` 字号完全不变。
    - **左右排版用两列栅格**：`grid-template-columns: minmax(0, 30%) minmax(0, 1fr)`，标签固定占左边 30% 一列（左对齐、超出省略号），数值占右边一列并右对齐；数值位数变化只会让数字在右列里向左伸展，标签那一列的位置和字号纹丝不动。
    - **`long`/`longLabel` 只保留给「自动」排版**：这两个判断原本会让长数值触发标签隐藏、字号切换，在 HUD 里会造成比数值本身还大的跳动。
    - 实测：两种排版各连续抓 4 帧（间隔 3.5s，数值一直在变），标签行段坐标、数值行顶边全部完全一致。
    - 代价：80×48 的格子里做左右排版，一行要同时放标签和数值，数值只能到 ~18px（上下排版是 26.9px）；把格子放宽到 ~110px，左右排版的数值就能回到 26px 左右（标签列 30%、余下 64px 给数值）。
  - **字号比例再调 + 标签最少 3 个字符（2026-11-08 二次调整）**：数值略降、标签增大——数值 `0.62→0.57 × 内容高`（封顶 32px）、标签 `0.26→0.30 × 内容高`（封顶 10→12px）。
    - 标签列改成 `max(内容宽×30%, min(内容宽×52%, 3.6em))`：至少放得下 3 个字符；标签超过 3 个字符时再额外留 1em 给省略号，保证**永远能看到 3 个字 + …**，不会只剩 2 个字。
    - 数值分到的宽度 = 内容宽 − 标签列 − 6px，再按最坏情况估宽收字号，所以标签列被撑宽时数值只是变小、不会挤压或跳动。
    - 格子相应放宽到 130×48（窗口 268×104），否则 3 字标签会把数值压到 10px 级。
    - 实测（130×48）：标签字号 12px（CJK 墨迹 11px、拉丁 8px），数值 23.9px（墨迹 17px）；`CPU/GPU/内存` 三个字的标签完整显示，`网络上行` 显示「网络上…」正好 3 字 + 省略号；网络那格因为最坏情况是 `999.9 MB/s`，数值字号会落到 ~16px。截图 `docs/hud-final2.png`。
  - **整体缩到 2/3 + 压缩边距（2026-11-08 三次调整）**：用户要求「2×2 按现比例再缩 1/3，并提高空间利用率」。
    - 格子 `130×48 → 87×32`，窗口 `268×104 → 178×68`（正好 2/3）。
    - 外边距：`HUD_PADDING 4 → 2`，且 `HudApp` 的内边距改由该常量驱动（之前是写死的 4px，容易和窗口自适应算出的尺寸不一致）。
    - 卡片内边距（仅 HUD 的 compact）：`padY max(3, 6%h) → max(1, 3%h)`、`padX max(4, 6%w) → max(2, 3.5%w)`，87×32 时是 1/3px；内容盒因此从 79×28 变成 **81×30**（占格子 93%×94%）。
    - 融合块圆角 `18px → 10px`（HUD 内），窗口圆角 `18px → 12px`，否则小尺寸下四角会被啃掉一块。
    - 左右排版的列间隙跟着高度走：`--lgap = max(3px, 0.11 × 内容高)`。
    - 实测（178×68）：标签 9px（拉丁墨迹 6px、CJK 8–9px），数值 17px（墨迹 12px）；`网络上行` 仍是「网络上…」3 字 + 省略号；网络那格数值 ~12px（最坏情况 `999.9 MB/s` 撑不下）。截图 `docs/hud-small@5x.png`。
  - **卡片最小尺寸放宽（2026-11-08）**：设置窗口里把卡片拖小，高度最低只能到 48，回不到 32。原因是两处写死的下限：
    - 画布拖拽缩放：`max(80, ...)` / `max(44, ...)` 再按 8px 吸附，`snap(44) = 48`，所以实际下限是 80×48；
    - 属性面板宽/高输入框：统一 `max(40, ...)`。
    - 现在改为宽 ≥ 40、高 ≥ **24**（拖拽与输入框一致，仍按 8px 吸附），`40×24` 足以让 HUD 单元格做到 32 甚至更矮；`hud.rs` 创建窗口时的 `inner_size` 下限也同步从 80×48 放宽到 40×24，否则重启后窗口会被先撑到 48 再被自适应缩回去。
    - 实测：把 4 张卡从 88×48 拖回 87×32，窗口随即从 180×100 缩到 178×68。
  - **默认 HUD 布局改为「CPU / 内存 / ↑ / ↓」（2026-11-08）**：`default_hud_config` 换成左列 CPU（上）/ 内存（下）、右列网络上行 `/` 下行，网络两格标签直接用箭头 `↑` `↓`；全部 `layout: "inline"`、87×32、共用一个融合组，默认窗口 178×68。
    - 标签列宽度不再固定留 30%：改成「刚好放下标签本身，超过 3 字时按 3 字 + 省略号余量」，上限仍是内容宽 52%。这样 `↑` 这种单字符标签只占 ~15px，把宽度让给数值（网络格数值从 ~12px 回到 ~17px）。
    - 纯符号标签（`↑ ↓ → ←` 等）单独放大一档字号（`0.42×内容高`，11–16px），并修正宽度估算（符号按 1.05em 计），否则 9px 下箭头会被省略号裁成一条竖线。
    - HUD 标签颜色从 `--text-dim` 提到 `--text-strong`：9px 的小字用最暗那档几乎看不清。
  - **标签与数值贴紧（2026-11-08）**：之前左右排版是「标签贴左、数值贴右」，中间空出一大截（87px 格子里有 30px 左右），看着就是「间距太大」。改成**左对齐紧跟**：
    - 数值 `justify-self: start`，紧跟标签列，列间隙 `--lgap` 从 `max(3px, 0.11×高)` 收到 `max(2px, 0.06×高)`；
    - 标签列宽改成 `min(52%, 标签估算宽 + 1px)`（原来多留 2px，且符号按 1.05em 算，箭头后面又是空档），符号类标签按 0.6em 估宽；
    - 实测标签墨迹到数值墨迹的间隙：`CPU` 4px、`内存` 2px、`↑ / ↓` 6px（改之前 CPU/内存 ~5px 但数值贴在右边缘，视觉中间空 30px；箭头 ~10px）。
    - 代价：数值不再与右边缘对齐，格子右侧会留一段空白；想收紧就把格子宽度改小（窗口会跟着内容自适应）。截图 `docs/hud-tight@5x.png`。
  - **按宽度决定小数位（2026-11-08）**：HUD 单元格里的百分比/标量数值，宽度够就输出两位小数，不够就退到一位、零位。
    - `Card.vue` 把内容盒像素尺寸（`box = { cw, ch }`）传给数字渲染器，渲染器用和 CSS 完全相同的公式（标签列宽、间隙、高度封顶）算出「数值还剩多少宽度」。
    - 判定用**最坏情况**的整数部分（百分比 `100.99`、标量 `999.99`），所以小数位数是**每张卡固定**的，不会因为数值变化在 12.85% / 12.9% 之间跳。
    - 只有「为了多一位小数把字号压掉 12% 以上」时才退位：例如 87px 格子里百分比取两位后字号 17.1 → 15.6px（-9%），可以接受；再挤就退成一位。
    - 字节类（`B/s`、`GB`）保持原来的量级规则：<10 两位、<100 一位、更大零位，本身已经是两位小数。
    - 手动在 `options.decimals` 里写死的值仍然优先。
    - 实测：`CPU / 内存` 现在显示 `12.85%` 这种两位小数，字号 15.6px；连续 4 帧墨迹高、标签左沿完全一致（没有跳动）。截图 `docs/hud-decimals@5x.png`。
  - **默认 HUD 拆成三块、各带主题（2026-11-08）**：`default_hud_config` 现在是「左块 + 右上块 + 右下块」：
    - 左块 `g-hud-left`：CPU / 内存 → `default-dark`；右上块 `g-hud-up`：↑ → `midnight`；右下块 `g-hud-down`：↓ → 新增的 `violet`（暗紫）；
    - 主题色板从 3 套扩到 4 套（新增 `violet`，深紫底 `rgba(24,18,36,.78)`）；
    - 三块**全部贴边不留缝**（`HUD_BLOCK_GAP = 0`，窗口 178×68）；`Stage` 用「角点/边中点是否落在别的块上」来判断贴边，共享边的圆角归零、边框去掉，所以三块拼起来仍是一个连续形状（外圈圆角保留）；
    - 实测块底色：左 `(26,26,29)`、右上 `(20,25,43)`、右下 `(30,25,39)`；接缝处只有 1px 过渡色，没有缝、没有双边框。截图 `docs/hud-3blocks@6x.png`。
    - **对齐修复（同日）**：三块拼起来后底边差了 1px，根因有两个——
      1. `.card-group` 没有 `box-sizing: border-box`，边框是加在宽高之外的；融合块会按需去掉共享边的边框，边框条数不同的块外尺寸就差 1~2px。加 `border-box` 后，块的外尺寸恒等于矩形尺寸。
      2. 绝对定位的卡片是相对「定位祖先的 padding box」定位的，`.hud` 上的 `padding` **不会**把它们往里推（所以左边/顶部贴边、右边/底部多出 4px）。改成在 `.hud` 里套一层 `inset: HUD_PADDING` 的 `.hud-inner`，留白才真正对称。
      - 另外把融合块的外轮廓色统一成整体主题的 `--frame-border`，避免三块各带主题时外框颜色断成几截。
      - 实测：顶/底边都在 `y=2` 与 `y=65`，左/右边都在 `x=2` 与 `x=175`，四边留白都是 2px。截图 `docs/hud-align@6x.png`。
      - **回归修复（同日）**：加了 `.hud-inner` 内边距容器之后 HUD 拖不动了。Tauri 的拖拽只认「鼠标事件的 `e.target` 自己带 `data-tauri-drag-region`」——内层容器盖住了整块区域，target 变成它，外层 `.hud` 上的属性就永远收不到事件。修法是把 `data-tauri-drag-region` 也挂到 `.hud-inner` 上（两层都挂），实测拖动位移与鼠标一致。
  - **行距收紧（2026-11-08）**：上下两行之间的空隙偏大（32px 格子 + 只有 ~10px 墨迹）。做两件事：
    - 格子高度 `32 → 22`（窗口 68 → **48** 高），行的物理间距直接变小；
    - 横向排版的「高度封顶」系数 `0.57 → 0.68`，让字号仍由**宽度**决定（13.5px 未变），但字形占格子的比例变大——所以是「行变紧」而不是「字变小」。
    - 实测：两行墨迹之间的间隙 `20px → 11px`，字形墨迹仍是 10–12px；块底边左右两半仍对齐在 `y=45`。
  - **HUD 右键菜单 = 托盘菜单（2026-11-08）**：HUD 上右键原来弹的是 WebView2 的浏览器菜单，现在改成和托盘图标一致的原生菜单。
    - `tray.rs` 把菜单项抽成 `build_menu(app)`，托盘和 HUD 共用；菜单事件的处理抽成 `dispatch(app, id)`——Tauri 的 `on_menu_event` 是**全局**的（官方注释：任何菜单事件都会调用，不论来自窗口还是托盘），所以托盘那边注册一次，HUD 弹出来的菜单点选也会走同一套逻辑。
    - 新增命令 `popup_hud_menu`，用 `WebviewWindow::popup_menu(&menu)` 在光标位置弹出；HUD 页面 `contextmenu` 事件里 `preventDefault()` 并调用该命令。
    - 实测：右键后出现 `#32768`（Win32 菜单窗口）182×138（6 项 × 23px，与托盘菜单一致）；点第 5 项「打开插件目录」确实拉起了资源管理器，说明事件派发打通。
## 10.5 视觉扁平化（2026-11-09）

- 按要求**去掉所有视觉效果**：卡片的磨砂颗粒（SVG/PNG 噪声）、顶部反光渐变、内阴影、投影、以及跟随光标的玻璃高光（.glow 元素 + pointer.rs 轮询线程）全部删除。
- 卡片现在只有「底色 + 1px 外框 + 圆角」；背景拆到 ::before 一层，透明度由 --card-alpha 控制（**唯一旋钮**），文字不受影响。融合块同理。
- 配置：PanelConfig.glow: bool → opacity: f64（默认 1.0，即主题原本的 0.72 透明度）；设置页工具条的「光标高光」复选框换成「不透明度」滑块（20%–100%，实时生效）。
- 实测：HUD 背景区亮度 σ=0.02（完全平面，无噪声/渐变）；把不透明度从 100% 调到 35%，背景像素从 25.3 变到 28.3（与桌面混合变多），文字保持不变。
- 顺带：pointer.rs 整个删掉，面板不再有 16Hz 的光标事件与局部重绘。

- **HUD 一键居中（2026-11-09）**：设置页 HUD 面新增「一键居中」按钮，做两件事——① 把所有卡片的外接矩形平移到 (0,0)，去掉拖动留下的外圈空边；② 把整个 HUD 窗口摆到屏幕正中（x = (屏宽 − 窗口宽) / 2，y 同理，四舍五入到整像素）。实测：先把布局搅乱（窗口 (100,100)、卡片整体偏移 (30,20)），点一下后卡片回到 (0,0)/(0,22)/(87,0)/(88,24)，窗口变成 (1191,695) 179×50，中心点正好是屏幕中心 (1280,720)。

- **融合块支持逐格主题（马赛克，2026-11-09）**：以前融合块只有一个背景、主题按整块设置，所以「四格融合 + 每格不同颜色」做不出来。改法：
  1. 每张卡片自己画底色（ackground-color: color-mix(...)，透明度仍由 --card-alpha 一个旋钮控制），融合块 .card-group 只保留外框和圆角；
  2. cardCorners() 给融合成员算「只在整块外沿才圆」的圆角，内部接缝保持直角，整块仍是一个圆角矩形；
  3. 设置页的卡片主题下拉只写当前卡片（原来会写到整组）。
  - 结果：左上/左下暗色玻璃、右上蓝色、右下紫色，四格拼成一块连续形状。
  - **踩坑**：改完发现两列之间有 1px 桌面漏光，查了半天（红线描边、纯色底、DOM 几何都上了）——真正原因是设置窗口还开着，它内存里是旧坐标（右列 x=88 而不是 87），一次保存把手改的坐标覆盖了。关掉设置窗口重新写配置，缝就消失了。**教训：手改 hud.json 前先关设置窗口。**
- **外设电量调研（2026-11-09）**：现有 device-battery 插件走三条通用通道——System.Devices.BatteryLife、BLE GATT 0x180F、HID Battery Strength（Usage Page 0x06 / Usage 0x20）。
  - 用户的罗技鼠标是 **046D:C539 的 2.4G 接收器**（非蓝牙）：三条通道全都拿不到（HID 没有标准电量 Usage，系统属性为 null）。
  - 接收器的 HID++ 接口（mi_02&col01，7 字节报告）**会回包**——实测发根特征请求得到 10 FF 8F 00 F1 01 00（HID++ 1.0 错误帧），说明通道可用，差的是正确的寄存器/特征序列。
  - G HUB 已安装（lghub_agent 监听 127.0.0.1:9010），但它的本地 API 对所有试过的 Origin 都回 403，不能直接借用。
  - 结论：**要读这类接收器鼠标的电量，必须实现罗技私有 HID++ 2.0**。探测工具留在 	ools/hidpp-probe、	ools/ghub-probe。
  - **照着开源实现改对了帧格式**：参考 Ithilias/logitray（MIT，Rust，Windows 托盘小工具，不走 G HUB）——帧是 [0x11, dev, feature, (func<<4)|swid, p0,p1,p2]，注意 **(func<<4)|swid**，我最初把两个半字节写反了，所以一直收到错误帧。
  - 修正后实测：10|11 01 00 1A 04 02 5A → **device 1 存在，HID++ 协议 4.2**，IFeatureSet 报告 **30 个特征**（枚举耗时较长，探测工具里已改成常驻读线程 + 队列）。
  - 同类开源项目（都读 HID++）：pwr-Solaar/Solaar（Linux/Python，协议参考实现）、o0Zz/PeripheralBatteryMonitor（C#）、Fan4Metal/mouse_tray（Python）、JKWTCN/BatteryMonitor（C++/Qt）、PixlOne/logiops（Linux/C++）。
  - **已实现（2026-11-09）**：`plugins/builtin/device-battery/LogitechHidpp.cs` 用 HID++ 2.0 读罗技接收器上鼠标的电量。
    - 帧格式 `[0x11, dev, feature, (func<<4)|swid, p0..p2]`（swid=0x0A）；用 **overlapped I/O + 超时 + CancelIoEx**，避免 HID 读写把插件线程卡死。
    - 这只鼠标是 **G903 LIGHTSPEED**（046D:C539 接收器，共 30 个特征）。它**不支持 0x1000 / 0x1004**，唯一电量类特征是 **0x1001 电池电压**（索引 0x06）。
    - **百分比换算**：先用 logitray 的阈值表得到 68%，但 G HUB 显示 72%（同一时刻 `0F 48` = 3912 mV）。改用 **Solaar 的插值曲线**（`estimate_battery_level_percentage`）并校准 **+23 mV**，实测 **71–72%**，与 G HUB 对齐（电压本身有 ±6 mV 抖动 ≈ 1%）。另外加了指数平滑（0.55/0.45），数字不会乱跳。
    - 特征表（`--features` 可查）：0x0000/0001/0003/0005/0020/**0x1001**/0x180x…0x1E22 等，没有其它电量类特征。
    - **休眠问题**：鼠标睡着时接收器不转发，读不到；所以读失败就沿用上一次读数（否则指标忽有忽无），并每 60s 重试。
    - 调试入口：`DeviceBattery.exe --logi`（逐条收发日志）、`--dump`（各来源汇总）。

  - **数字卡片字号缩放（2026-11-10）**：数字块可以单独调字号，按比例缩放，卡片尺寸不变。
    - 配置：card.options.fontScale（0.6–2.0，缺省 1）；Card.vue 把它写成 CSS 变量 --font-scale。
    - 渲染：NumberRenderer.vue 里所有字号表达式统一乘 ar(--font-scale, 1)（含 --fsize 的 HUD 分支与容器查询的 clamp 分支），单位 em 会自动跟着走。
    - 设置页：卡片属性里新增「字号 · N%」滑块（60%–200%，双击恢复 100%），只对数字渲染器显示。
    - 实测（HUD 上直接量墨迹像素）：100% → 1001，60% → 435（0.43×），200% → 2017（2.01×），说明确实是等比缩放；放大到超过格子可用宽度时会被卡片的 overflow: hidden 裁掉。

  - **融合块整体移动（2026-11-10）**：桌面编辑里拖融合块的任意成员，整块一起走。
    - 抓取时把同组所有成员的位置记下来，用它们的**并集矩形**参与吸附；算出位移后统一加到每个成员上（而不是只动被点中的那张）。
    - snap.ts 的排除参数从「单个下标」扩成「下标集合」，整块移动时把本组所有卡片都排除在吸附基准外，否则自己的成员会被当成邻居。
    - 整块移动时保证最左/最上成员不出界；融合块的单卡缩放仍按原逻辑（未融合的卡片才有右下角手柄）。
    - 实测：拖动 cpu 卡片 (+100,+44)，同组 mem、cpu-temp 的位移完全一致。

  - **Win+D 之后立刻恢复（2026-11-10）**：按 Win+D 显示桌面时，系统会把面板窗口最小化，原来靠 800ms 轮询把它拉回来，所以桌面上要等一会儿才看到指标。
    - 修法：给面板窗口装一个常驻子类，拦 WM_SIZE(SIZE_MINIMIZED) 和 WM_SHOWWINDOW(FALSE)，**当场** ShowWindow(SW_SHOWNOACTIVATE) + 重新插回 Progman 之上。
    - 兜底轮询从 800ms 收紧到 **120ms**（只读窗口状态，开销可以忽略：实测空闲 CPU 仍是 0.57% 整机）。
    - 实测：手动 SW_MINIMIZE 后 **0ms** 恢复；真实 Win+D（前台浏览器被最小化、前台变成 Program Manager）面板 50ms 内就绪，之后一直可见。

  - **HUD 卡片内边距（2026-11-10 第二次）**：先加的是「窗口留白」（HUD_PAD_X/Y），但窗口本身是透明的——留白只是不可见的窗口面积，视觉上没有任何变化（用户反馈「没看出来有变化」，判断正确）。
    - 真正影响观感的是**卡片内边距**：Card.padding() 的 compact 分支从 max(2, min(8, w*0.035)) 改成 max(4, min(12, w*0.075))，87 宽的格子由 3px → **6px**，上下 1px → 2px。
    - 实测：方块内文字距块左边 7px、右边 8px（改之前是 3px），文字范围 21..180 落在 14..188 的块内，没有溢出。

  - **HUD 留白（2026-11-10）**：块和块之间没问题，但整块贴着窗口边框太紧。
    - HUD_PADDING = 2 拆成 HUD_PAD_X / HUD_PAD_Y：先试 8/3（反馈还是太紧，没有呼吸感），最终定 **14 / 4**——左右 14px 明显放松，上下 4px 保持紧凑。
    - 窗口尺寸随之从 178×48 → 190×50 → **202×52**（内容 174×44 + 28/8），Rust 里的 default_hud_w/h 同步；窗口尺寸本来就由前端 
esize_hud 按内容算，旧配置打开后会自动修正。
    - **最终定回 2px**：既然窗口透明、留白不可见，就没必要撑大窗口（只影响拖拽区域）。HUD_PAD_X/Y = 2，窗口回到 **178×48**；视觉上的呼吸感完全由卡片内边距负责（见下一条）。

  - **桌面块投影（2026-11-10）**：扁平化之后卡片完全贴平，加一层很轻的投影让块「浮」起来。
    - 变量：--card-elev: 0 10px 24px rgba(0,0,0,0.45), 0 2px 5px rgba(0,0,0,0.35)，写在 :root，想调浓淡改这一行即可。
    - 只给**桌面面板**：.card 用投影；融合块由 .card-group 投一次（成员 .card.fused 设 ox-shadow: none，否则块内每格都会投出互相压暗的阴影）；HUD 仍然全平（.hud .card / .hud .card-group 都已显式 
one）。
    - 验证：把投影临时染成红色能扫到 277 个红像素（说明确实在绘制），换回黑色后在你深色壁纸上很含蓄——这是「稍微加点」的预期效果。
    - 开销：空闲 CPU 仍是 **0.35% 整机**（投影是静态绘制，值刷新时随卡片一起重绘，没有额外合成层）。

  - **更新走系统代理 / 手动代理（2026-11-16）**：检查更新和下载安装包都要能过代理。
    - tauri-plugin-updater 默认启用了 `system-proxy` 特性（→ `reqwest/system-proxy` → `hyper-util/client-proxy-system`），会自动读 Windows 的系统代理设置；`check()` 的 JS/Rust 接口另支持手动 `proxy`。
    - 新增 `prefs.json` 的 `proxy` 字段（空 = 跟随系统）：Rust 侧 `build_updater()` 用 `UpdaterBuilder::proxy(url)`，前端 `check({proxy})` / `download(cb, {proxy})` 也传同一个值；设置页更新区块加了「代理」输入框。
    - 实测（stderr 里看后台检查结果，直连当时是通的）：
      A 手动代理=死端口 `127.0.0.1:9` → 检查失败（证明走了手动代理）
      B 手动代理=真实 `127.0.0.1:10809` → 检查成功
      C 手动为空 + 系统代理=死端口 → 检查失败（证明跟随系统代理）
      D 手动为空 + 系统代理=真实 → 检查成功
    - 结论：系统代理本来就是跟着走的（之前报错是因为当时系统代理是关的、而直连又被墙）。

  - **低显存模式（2026-11-16，核显机器优化）**：WebView2 的 GPU 进程在核显机器上占的是系统内存，实测关掉后收益很大。
    - 实测（面板 + HUD + 任务栏都在、13 进程）：GPU 进程私有 **131 MB → 28 MB**，显存 **95 MB → 0**（连共享显存也归零），整树私有 **358 MB → 257 MB**，CPU 5.0% → 5.6%（噪声级）。
    - 参数：`--disable-gpu --disable-gpu-compositing --disable-gpu-rasterization --js-flags=--max-old-space-size=64`。
    - **关键坑**：WebView2 的浏览器参数只能启动时确定，而且同一应用的所有窗口必须用**完全一样**的参数——只给一个窗口加参数，其它窗口会创建失败（`0x8007139F ERROR_INVALID_STATE`）。所以：`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` 环境变量无效（Tauri 会显式传自己的默认参数覆盖它），改成 `prefs::browser_args()` 统一传给每个窗口（面板也从 `tauri.conf.json` 搬到代码里创建，否则配置里写死的参数改不了）。
    - 开关放在设置页「插件与诊断 → 性能」，写入 `prefs.json`（`{"lowGpuMemory":true}`），**重启后生效**，界面提供「立即重启」。
    - 代价：软件渲染，文字/圆角/阴影略微变糙；设置页滚动不如硬件渲染顺滑。

  - **自动更新（2026-11-16）**：接 tauri-plugin-updater，从 GitHub Release 拉 `latest.json` 自更新。
    - 后端 `updater.rs`：启动 20s 后检查一次，之后每 6 小时；发现新版本缓存并广播 `update-available`。命令 `update_check` / `update_state` / `app_version`。
    - 前端：设置页「插件与诊断 → 更新」区块，当前版本 / 新版本 / [检查更新] / [下载更新] / [重启并安装]；下载走 `plugin-updater` 的 `download()`（结束时自动验签），安装走 `install()`（Windows 上会退出应用并拉起 NSIS）。
    - 签名：`tauri signer generate` 生成密钥，**公钥写进 `src-tauri/tauri.release.conf.json`，私钥只在本地 `~/.tauri/` 和 GitHub Secrets**（`TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`），仓库里不出现私钥。
    - 构建分两套：本地用 `tauri.conf.json`（不签名，避免没私钥时构建失败）；发布用 `--config src-tauri/tauri.release.conf.json`（`createUpdaterArtifacts: true` + pubkey + 端点 + `installMode: passive`）。
    - CI：带签名环境变量构建，然后用安装包的 `.sig` 生成 `latest.json`（**Tauri v2 不再产出额外 zip，直接签 NSIS 安装包**），随 Release 一起上传；`notes` 用 ASCII，避免 runner 上 PowerShell 按 ANSI 解析导致乱码。
    - 验证：用本地 HTTP 服务 + 改端点的临时构建跑通了「检查 → 下载 → 验签」；期间 Tauri 的防篡改检查还正确拦住了「清单版本 9.9.9 但签名版本 0.2.2」的情况。最后一步「重启并安装」要 UAC，留给用户点。
    - 注意：**0.2.0 及更早的包不含 updater**，只能手动装 0.2.1+；之后再升级就能全自动。另外本机当前直连 github.com 会超时（`curl` 21s 失败），更新会报 `error sending request`，需要代理/网络就绪。

  - **内存优化第一档：关掉的窗口不再占内存（2026-11-16）**：HUD / 任务栏条原来关闭时只是 `SW_HIDE`，webview 和它的 renderer 进程都还在（每个 ~80 MB WS / ~32 MB 私有）。
    - 改成**关闭即销毁**：HUD 监管线程发现 `enabled=false` 就 `destroy()`，重新开启时回主线程重建（约 0.3 s）；任务栏条同理，并且每次都换新 label（`taskbar-<ms>`）再挂回 `Shell_TrayWnd`。
    - 实测（13 进程那套）：两者都开 709–724 MB WS / 346–393 MB 私有 → 都关 551 MB WS / 295 MB 私有，**-158 MB WS / -65 MB 私有**，进程 13 → 11、renderer 3 → 1。运行时逐个关闭也能立刻回收（13→12 进程、3→2 renderer，约 1 s 内），重新开启会重建窗口。
    - 顺手：`foreground_fullscreen` 判自身窗口时把 `taskbar` 前缀的动态 label 也算进去，避免任务栏条被误判成"全屏应用"。
    - 试过但**放弃**：给插件设 `DOTNET_GCHeapHardLimit=64MB` + `DOTNET_TieredPGO=0`，A/B 实测插件私有内存 33.9 → 31.0 MB（-2.9 MB），收益太小且硬上限有 OOM 崩溃风险，不引入。

  - **打包安装程序（2026-11-16）**：`pnpm tauri build` 走 NSIS，产出 `desktop-monitor_0.1.0_x64-setup.exe`（12.4 MB）。
    - 图标：`tools/make_icon.py` 画 1024px 源图（深色圆角底 + 青色环 + 三根柱），再 `tauri icon` 生成全套（ico/png/Store 尺寸）。
    - 内置插件随安装包走：`tools/scripts/prepare_bundle.ps1` 把 `build/*` 的 exe + `manifest.json` 铺到 `src-tauri/resources/plugins/`，`bundle.resources` 带上；运行时 `plugins::install_bundled()` 从 `resource_dir()/resources/plugins` 复制到用户插件目录（缺失或版本不同才覆盖）。实测：清空插件目录后启动，两个插件自动铺好。
    - 安装器：`perMachine`（装到 `C:\Program Files\desktop-monitor`，与开发期的目录一致）、简体中文/英文、无语言选择页。
    - 一键出包：`tools/scripts/make_installer.ps1`（stage → tauri build → 打印路径/大小/SHA256）。
    - CI：`.github/workflows/release.yml`（手动 `workflow_dispatch`，windows-latest）。版本号只认 `package.json`，构建前用 `tools/scripts/set-version.mjs` 同步到 `tauri.conf.json` / `Cargo.toml` / `ui/package.json`；自动发布成 GitHub Release（`v<version>` tag + 安装包 + `.sha256` 附件，自动生成 release notes），同时留一份 Actions artifact；权限 `contents: write`。tag 归流水线管：同版本**已有 Release 就失败**（请升版本号），只有残留 tag 没有 Release 时会把 tag 强制移到当前提交再发布。
    - CI 细节：`pnpm/action-setup` 用 10（和本地一致），`--frozen-lockfile` 两份锁都要过；`ui/package.json` 补了 `pnpm.onlyBuiltDependencies: ["esbuild"]`，否则 pnpm 10 默认拦 postinstall，CI 上 esbuild 二进制装不上。脚本里的仓库根目录改成从 `$PSScriptRoot` 推，别再硬编码 `D:\Work\desktop-monitor`。


    - 坑：`.ps1` 里 `$ErrorActionPreference = "Stop"` 时，CLI 往 stderr 打的进度行会被 PowerShell 当成错误直接终止，构建脚本要用 `Continue` 并自己检查 `$LASTEXITCODE`。

  - **托盘「显示面板」改成真正的开关（2026-11-11）**：原来这一项调的是 `overlay::reshow()`——把面板窗口强制显示 + 恢复到全屏 + 重新插到 Progman 之上。问题是面板本来就一直显示，`keep_bottom` 每 120ms 还会再兜一次底，所以它平时是个空操作，只有窗口被意外改过尺寸时才看得出效果，名字却像"显示/隐藏"。
    - 改法：新增 `HIDDEN` 状态 + `overlay::toggle_hidden()`；菜单项文案改「显示/隐藏面板」。隐藏时 `ShowWindow(SW_HIDE)`，并且 `keep_bottom` 与最小化钩子都跳过（否则 120ms 后又被拉回来）；再次点击走 `reshow()` 恢复全屏位置。
    - 进入桌面编辑模式会解除隐藏（否则编辑完面板"不存在"）。
    - 实测：点一次 → 面板隐藏且 2 秒后仍隐藏；再点一次 → 回到 (0,0,2560×1440) 可见。
    - 顺带修：`tools/scripts/build_install_run.ps1` 以前**构建失败也会拿旧 exe 去安装**（只看文件是否存在），现在检查 Tauri CLI 退出码；脚本本身改成纯 ASCII（PowerShell 5.1 按 ANSI 读无 BOM 文件，中文注释会吞掉引号导致语法错误）。

  - **设备与指标分离（2026-11-10 改版）**：绑定改成了「设备是设备、指标是指标」，卡片 = 设备 + 指标。
    - 插件 `devices` 返回 `{key,name,source,level,charging,hidden,metrics:[{id,title,kind}]}`；每台设备自带它能出的指标清单，前端据此做两级选择。
    - 「绑定」不再是全局门控：所有设备都会出指标；设备页的开关变成**隐藏/显示**（`setConfig {"hidden":[...]}`），隐藏后该设备指标从列表消失，已有卡片保留但显示 `--`。
    - 设置页新增**设备**独立标签页（从「插件与诊断」里搬出来）：设备名 / 来源 / 电量 / 指标数 / 显示切换 + 重新扫描。
    - 卡片检查器新增「设备」下拉（系统 / 通用 + 各设备），「指标」下拉按设备过滤 → 选设备自动落到该设备第一个指标。
    - 坑：模板里画布容器原来是 `v-if="surface !== 'plugins'"`，加了 devices 页后它把设备页挡住了；改成 `surface === 'panel' || surface === 'hud'`，`v-else-if` 链才生效。

  - **设备发现 / 绑定（2026-11-10）**：不再「扫到什么就出什么指标」，改成先发现、再由用户绑定。
    - 协议：插件新增 devices 方法（返回 key/name/source/level/charging/bound）和真正的 setConfig（{bindings:[键...]}）；indings 缺省 = 未配置 → 全部视为已绑定（兼容旧配置）。
    - 宿主：PluginManager::devices/set_config/get_config，配置落到 <config_dir>/desktop-monitor/plugins/<id>.config.json，**插件启动时自动重放**（setConfig 紧跟 initialize 之后）。IPC：get_plugin_devices / set_plugin_config / get_plugin_config。
    - 插件内部：未绑定的设备不生成 *.level / *.state 指标，attery.devices 列表和 attery.status 也只统计已绑定的设备。
    - 设置页「插件与诊断」顶部新增「设备」区块：设备名 / 来源（hidpp·aula·windows·ble）/ 电量 / 绑定按钮，另有「重新扫描」。
    - 注意：设备键形如 dev-<hash>（由设备名 hash 而来），改名会导致键变化 → 需要重新绑定。

  - **狼蛛（AULA）F87 PRO 键盘电量（2026-11-10）**：桌面上的逆向项目 `狼蛛驱动逆向\f87ctl` 已经把协议摸清了，直接照抄。
    - 传输：HID **UsagePage 0xFF02** 的集合（无线 `3554:FA09` 接收器 / 有线 `258A:010C`），20 字节帧：`[0x13][cmd][总包数][包序号][(方向<<4)|长度][14B 数据][前 19 字节和]`。
    - 读电量用命令 **`0x4A`**，回 2 字节：`byte0` = 电量百分比，`byte1` 的 **bit4** = 充电中。
    - 两个坑：设备约 **1.1 秒**后才吐回包；无线空闲会休眠。所以先发 `0x07` 唤醒（最多 3 次），再对 `0x4A` 做 5 轮 ≈1.1s 读窗口——实测稳定拿到 `5A 01` = 90%。
    - 插件新增 `AulaBattery.cs`（overlapped I/O + 超时，和罗技那份同思路），作为设备发现的第 4 个来源；指标 `狼蛛键盘 电量 / 状态`，读不到时沿用上次读数。
    - 桌面面板新增一张卡片：`view=battery`、`metric=battery.dev-76cf60e7.level`、位置 `(2250, 984) 128×56`、标签「键盘电量」。
    - 调试入口：`--aula`（逐包收发日志）、`--metrics`（列出全部指标 id）、`--hid`（列出 HID 接口的 usagePage）。
  - **dev 渲染器总览页（2026-11-10）**：ui/dev.html + ui/src/dev/DevApp.vue，把每个渲染器在**不同尺寸**和**不同状态**下排出来，专门用来抓布局崩坏（比如条形在超薄高度下消失）。
    - 每个渲染器一节：一排是尺寸（超薄 220×32 / 小 120×48 / 默认 / 大 300×160），一排是状态（ok / warn / bad / 无数据）；另有「紧凑模式（HUD）」开关，切换后同一批卡片按 HUD 的 compact 规则渲染。
    - 覆盖 10 组：number（ratio / bytes / scalar 三种指标）、ring、battery、bar、sparkline、status、list、text。
    - **只在 dev 打开**：windows::dev_enabled() = debug 构建或环境变量 DM_DEV=1；release 正常启动不会创建。
    - 打开方式：先退出应用，再 \='1'; & 'C:\Program Files\desktop-monitor\desktop-monitor.exe'。

  - **不透明度语义修正（2026-11-10）**：之前 100% 仍然发虚，因为主题底色本身是半透明的（
gba(...,0.72)），--card-alpha 只是再乘一次。
    - 改法：① 四个主题的 --card-bg 全部改成**不透明**的 
gb(...)（:root 的兜底值也改了）；② 透明度不再折进底色，改成整卡 opacity: var(--card-alpha, 1)——所以 **100% = 完全不透明**，调低则连**文字一起淡**（这是需求要的，之前刻意只淡背景）。
    - 融合块的边框/投影在 .card-group 上，所以那一层也加了同样的 opacity，否则块淡了边框还实心。
    - 实测：100% 时卡片内部像素 (12,18,42) 与午夜蓝主题底色完全一致（无混合）；调到 20% 后同一区域 (46,46,53) → (64,65,66)（背后的窗口透出来了）；空闲 CPU 仍 0.36% 整机。

## 11. 性能实测（2026-11-09，i5-13600KF 20 线程 / 2560×1440）

- **内存**：13 个进程合计 **770 MB Working Set / 513 MB 私有**。构成：浏览器进程 155MB、GPU 进程 116MB（私有 255MB）、三个窗口的渲染进程 98 / 81 / 71MB、插件 68 + 55MB、主进程 38MB、两个工具进程 40 + 20MB。**每个窗口一个独立渲染进程**（面板 / HUD / 任务栏条各一），这是内存的主要来源。
- **CPU（静置，设置页关闭）**：11.7% 单核 ≈ **0.59% 整机**（另一次采样 6.7% ≈ 0.33%，波动来自面板每秒刷新）。
- **CPU（设置页打开）**：优化前 **42.7% 单核 / 2.14% 整机**，优化后 **12.4% 单核 / 0.62% 整机**。
- **CPU（光标在卡片上移动）**：玻璃高光触发重绘，约 31% 单核（1.5% 整机），只在移动时产生。
- **这轮修的**：设置页预览关掉 CSS 过渡（主因——条形/环形的 0.45s 过渡让 2560×1440 的缩放画布持续合成）、预览指标 5Hz → 1Hz 合批、预览里关掉颗粒与高光层、磨砂颗粒从 SVG eTurbulence 换成 64×64 PNG data URI。
- **剩余可优化**：任务栏条为一个渲染进程（~71MB）只显示 4 个数；系统采集插件 2.4% 单核（LHM 轮询）；GPU 进程私有 255MB 是 WebView2 的缓存。

  - **桌面编辑模式（2026-11-10）**：设置页只管「有哪些指标」，位置/大小直接在桌面上拖。
    - 托盘菜单或设置页画布左上角「桌面编辑」进入：面板临时取消点击穿透 + 置顶，可交互；Esc 或顶部「完成」退出。
    - **命中测试用窗口子类化**（SetWindowSubclass + WM_NCHITTEST 返回 HTTRANSPARENT）：只有卡片区域接收鼠标，空桌面照常能用。
    - 踩坑：一开始用「光标移动时反复切 WS_EX_TRANSPARENT」实现，结果鼠标进出卡片时屏幕会**闪一下**——因为 Tauri 的 set_ignore_cursor_events 会带 SWP_FRAMECHANGED，强制重绘。改成子类化后样式全程不变，闪烁消失。
    - 另一个坑：切置顶/取消 WS_EX_NOACTIVATE 的过程中窗口会被系统藏一次（WS_VISIBLE 变 false，表现为「桌面内容全没了」），所以切换末尾统一 ShowWindow(SW_SHOWNOACTIVATE) 兜一次；命中刷新线程里也会补显示。
    - 编辑期间窗口置顶、不跑「保持置底」循环；退出后恢复置底 + 点击穿透。
  - **融合块圆角修复（2026-11-10）**：融合后偶发「三个角圆、一个角方」。
    - 根因：cardCorners() 用 0.5px 容差比较「卡片边」和「块边」，而成员尺寸常常差 1px（例如 gpu 高 170、gpu-power 高 85，块高 171），于是贴着块底边的那张卡被判成「没贴边」→ 该角给 0，方块角就露在圆角边框外面。坐标里的 70.999999 这类浮点也会加剧这个问题。
    - 修法：① 融合块矩形**取整**（Math.round），块边界和卡片边界对齐；② 帖边判断放宽到 **2px 容差**；③ 保存配置前把卡片几何**取整**（设置页和桌面编辑两处），从源头避免小数。
    - 验证（用当前配置复算）：cpu[tl,bl]、mem[tr,br]、gpu[tl,bl]、gpu-temp[tr]、gpu-power[br] —— 四个角都有人负责，块外轮廓是完整圆角矩形。

  - **等间距吸附（2026-11-10，PPT 手感）**：同一行（列）里已有两块时，把新块拖到旁边会自动让三块间距一致（1 _ 2 _ 3）。
    - 实现：在 ui/src/snap.ts 里找「垂直方向有重叠」的相邻两块 a、b，算出间距 gap，再给出「贴到 b 右侧 = b.right + gap」和「贴到 a 左侧 = a.left − gap − w」两个候选；和普通对齐吸附比谁更近就用谁，命中时画两条边界参考线。
    - 设置页预览和桌面编辑共用 snap.ts（原来设置页是自己一份实现，已合并）。
    - 实测：三块 x=200/500/1500（间距 130），把第三块拖到 805 → 吸附到 **800**，间距变成 130/130；纵排同样成立（y 吸附到 800）。

- **桌面背景预览（2026-11-10，已移除）**：曾经支持把当前壁纸铺在编辑画布后面；因为实际用处不大、还要处理 4K 壁纸把 WebView 拖灰的问题，**按需求整体删掉**（前端按钮/状态/缩图逻辑 + 后端 get_wallpaper 命令一起清理）。

  - **Ctrl + 滚轮缩放编辑视图（2026-11-10）**：设置页画布可以缩放查看。
    - 比例 = 自适应 × 用户缩放；自适应仍然按窗口大小算（HUD 上限 2.6，桌面上限 1），用户缩放再乘上去。
    - **Ctrl + 滚轮**以光标为锚点缩放（缩放后光标下的点不动），缩放倍率用 exp(-deltaY × 0.0015)，范围 0.1–4。
    - 缩放后画布可以滚动：.canvas-wrap 从 overflow: hidden 改成 uto，并用 margin: auto 保持居中（flex 居中 + 溢出会裁掉左侧，这是已知坑）。
    - 重置：右下角比例文字可点，右上角出现「适应」按钮，另外支持 **Ctrl+0**。
    - 实测：23%（自适应）→ 滚一格 30% → 连滚到 46%，点「适应」回到 23%。

  - **卡片的智能参考线（2026-11-10，对标 PPT）**：设置页拖动/缩放卡片时会吸附到相邻元素和画布的对齐线。
    - **吸附点**：其他卡片的左/中/右（上/中/下）三条线，加上画布的 0 / 中线 / 边线；拖动时移动矩形的左中右都参与比对，缩放时只吸被拖的那条边。
    - **阈值**：**7 屏幕像素**（换算成内容坐标要除以 scale，预览是缩放显示的）；没命中吸附就退回原来的 8px 网格吸附，所以「有邻居就贴邻居、没邻居就贴网格」。
    - **参考线**：命中后画一条粉色虚线（#ff5b8a，1px 虚线 + 微光），横向线跟随两端的元素范围，纵向线跟随上下范围；参考线画在 .viewport（未缩放）里，所以线宽永远是 1px 而不是被 scale 吃掉。
    - **松开/失效即消失**；被 canMove（重叠约束）拒绝时也会立刻清掉，避免出现「线在对齐但元素没动」的假象。
    - **按住 Alt** 临时关闭吸附（和 PPT 一致）。
    - 顺带加了 PPT 式的键盘微调：**方向键 1px / Shift+方向键 8px**，连续微调合并成一步撤销；**Delete/Backspace** 删除选中卡片（输入框内不抢键）。
    - 实测：把卡片拖到画布中心 +20px 处松手，结果精确落在 x=1195（中心 1280）；拖到空旷处则落在 8 的倍数（1608）；拖拽过程中截图里能看到粉色参考线。

  - **设置页自绘下拉（2026-11-09）**：原生 `<select>` 的**弹层是 WebView2 自己画的**，会跟随系统浅色主题——白底、直角、和暗色 UI 完全不搭（试过 `color-scheme: dark` 和 `<meta name="color-scheme">` 都无效，只对滚动条有效）。
    - 新增 `settings/Dropdown.vue`：按钮 + Teleport 到 body 的固定定位弹层；深色底 `#1f1f26`、1px 描边、**10px 圆角**、投影、分组标题（指标按插件分组）、选中项打勾、悬停高亮。
    - 交互：点击开合、外部点击关闭、`↑/↓` 移动、`Enter` 选择、`Esc` 关闭、选中项自动滚入视野；下方空间不够时向上翻转。
    - 设置页的 5 个下拉（整体主题 / 指标 / 渲染器 / 排版 / 卡片主题）全部换成它，`<select>` 一个不剩。
    - 实测：弹层底色 `(31,31,38)`、描边 `(58,58,64)`、左上角 7 行内从 x=22 收到 x=16（10px 圆角）；点开出现、点选项后关闭。
    - **滚动回跳修复**：监听 `scroll` 原本是为了页面滚动时让弹层跟着控件走，但捕获阶段也会收到**弹层自己**的滚动；那里原来直接调了 `show()`（内含 `scrollActiveIntoView()`），于是每滚一格就被拽回选中项。改成「只重新定位、不碰滚动与高亮」，并跳过 `target` 在弹层内部的滚动事件。实测滚轮 6 格后列表保持不动（1.5s 后也不回跳）。
  - **设置页整理（2026-11-08）**：去掉开发者视角的文案、补齐交互反馈。
    - 文案：指标下拉只显示中文名（`sys.cpu.usage` 挪到 `title` 提示）；渲染器下拉去掉 `（number）` 这类 id；卡片列表显示渲染器中文名而不是 `view` id；插件行只留版本号（插件 id 挪到悬停提示）；「热重载 / 热插拔」→「即时生效 / 自动加载」。
    - 反馈：新增保存状态徽标（保存中… / 已保存 / 保存失败），失败时原来只 `console.warn`，现在会提示「配置文件被外部修改过，已重新载入」。
    - 交互：插件页隐藏主题下拉（原来它改的是 HUD 的主题，但用户正在看插件页）；宽高输入框补 `min/step` 与 `px` 单位标注；HUD 页新增「回到右上角」一键复位；空状态文案说明下一步动作；删除按钮加「可 Ctrl+Z 撤销」提示。
    - 实测：UIA 验证插件页 combobox 数量为 0（主题下拉确实隐藏）；HUD 页出现「回到右上角」按钮；截图 `docs/settings-window.png`。
  - **指标下拉按来源分组（2026-11-08）**：指标列表用 `<optgroup>` 分组，顺序是「内置指标 → 各插件（用 manifest 里的显示名）」，插件名从 `list_plugins` 取，所以设置页挂载时会先拉一次插件列表（原来只在切到插件页时才拉）。
    - 实测（UIA 展开下拉）：`内置指标`（时间 / CPU 历史 / 网络历史）→ `设备电量`（设备电量 / 电量状态）→ `系统采集`（CPU 使用率 … 网络上行），共 3 组。
    - 说明：`<select>` 的弹层是 WebView2/Chromium 自己画的，会跟随系统浅色主题，`color-scheme: dark` 和 `<meta name="color-scheme">` 都试过仍不生效（对滚动条有效），属于平台限制，暂不处理。
    - 删除卡片仍然没有二次确认（按需求：靠 Ctrl+Z 兜底即可）。
  - **桌面面板视觉重做（2026-11-08）**：目标是「有设计感但不抢眼」，只动排版与数据可视化，不加投影/模糊这类开销大的效果。
    - **统一排版语言**：所有卡片的标签都是 10px、字重 500、`letter-spacing .14em`、`text-transform: uppercase`、`--text-dim`（英文标签读起来像小标题，中文不受影响）；数值改成 600 字重 + `tabular-nums` + `-0.01em` 字距，单位 0.55em、与数值留 0.28em 间隙。
    - **环形**：描边 8 → 6.5、半径 42（留出圆头端点空间），数值弧改成 `--violet → --accent` 的线性渐变（每个实例独立渐变 id），中心数值 22 → 26px，标签在小标题样式下。
    - **条形**：轨道 5 → 4px、圆角 999px，填充改为紫→蓝渐变；标签/数值行与条形之间用 `space-between` 撑满卡片，视觉重心更稳。
    - **折线**：线条 1.2 → 1.5px，底下补一条 `--accent` 22% → 0 的渐隐填充，数据有体积感但仍很淡。
    - **列表**：行与行之间加 `1px solid var(--card-border)` 细分隔线，键 500 字重、值右对齐等宽数字。
    - **状态**：圆点 10 → 7px，外发光改成 14% 透明度的柔光圈（原来 65% 太抢眼）。
    - **卡片**：底色之上叠一层 `linear-gradient(180deg, rgba(255,255,255,.035), transparent 44%)` 顶部高光，玻璃感更明显但不改变主题配色。
    - 实测（像素统计）：环形卡 accent 弧 175 像素、条形卡渐变填充 415 蓝 + 108 紫、折线卡 828 蓝像素，说明渐变/描边都正常渲染。截图 `docs/panel-design.png`。
  - **任务栏指标条（2026-11-08）**：把 CPU / 内存 / 网络上下行直接显示在任务栏上。
    - **为什么这么做**：Win11 的任务栏是 XAML 实现的，**不支持老的 deskband**（注册表那种工具栏），也没有公开的「往任务栏里塞控件」的接口。
    - **真的挂进任务栏**：先按普通方式建一个透明、点击穿透的小窗，再 SetWindowLongPtr(GWL_STYLE) 去掉 WS_POPUP、加上 WS_CHILD | WS_CLIPSIBLINGS，然后 **SetParent(我们的窗口, Shell_TrayWnd)**。挂进去之后它就是这个任务栏的子窗口：**跟着任务栏一起移动、一起自动隐藏、z 序也在任务栏里**，不再需要 600ms 轮询去追位置。
    - **定位**：在任务栏客户区坐标里右对齐到 TrayNotifyWnd（时钟）左侧 10px、垂直居中，尺寸按 DPI 换算。
    - **explorer 重启的坑**：子窗口会**跟着父窗口一起被销毁**（explorer 重启时任务栏窗口没了，我们的也一起没）。所以 	askbar.rs 每 500ms 检查一次：IsWindow 为假就用**新的 label**（	askbar-<时间戳>，同一 label 已被占用，复用会创建失败）重建窗口并重新 SetParent；日志里会打印 [taskbar] 任务栏重建，窗口重挂为 ...。
    - **输入**：配置在 %APPDATA%\desktop-monitor\taskbar.json（首次运行自动生成），字段 enabled / width / items；托盘菜单「任务栏指标」可即时开关。
    - **渲染**：TaskbarApp.vue 只画一行文字，标签 10px 暗色、数值 12px 半粗等宽数字，块之间 1px 分隔线；字节压缩成 1.5M/s。页面里必须**先 invoke 取配置、再 listen 挂事件**——反过来会卡住，命令根本发不出去（踩过）。
    - **又一个坑（数值全是 --）**：新窗口拿不到 metrics 事件——capabilities/default.json 的 windows 只列了 panel / settings / hud，Tauri v2 会把没列出的窗口的**核心插件命令**（含 listen）全部拒绝，而自定义命令不受影响，所以「配置能取到、事件收不到」特别迷惑。修法：窗口列表加 	askbar*（通配符同时覆盖重建时的 	askbar-<时间戳>）。
    - 实测：窗口挂在 Shell_TrayWnd 下、(2052,1396) 248×40、文字右对齐、UIA 读到的内容为 CPU 8% / 内存 58% / ↑ 476B/s / ↓ 470B/s；任务栏空白处和条带位置的右键都能弹出任务栏自己的 XAML 菜单（点击穿透正常）；	askkill /F explorer + 重启后 1 秒内自动重建并重新挂载。截图 docs/taskbar-final@3x.png。

  - **HUD 文字亮度与单位可读性（2026-11-09）**：27 寸 2K 屏上数值偏「纯白刺眼」，单位又小又灰。
    - 数值颜色 `--text`（#e8e8ea，亮度 232）→ `--text-strong`（#d6d6de，214）：暗约 8%，仍然清晰但不再扎眼；`warn/bad` 状态色单独加了更高优先级的规则，不会被这条覆盖。
    - 标签颜色 `--text-strong` → `--text-dim`（#9a9aa6）：标签和数值同字号，靠亮度拉开层次。
    - 单位 `0.5em` → **`0.72em`**（13.5px 的字对应单位约 9.7px，原来只有 6.8px），颜色从 `--text-dim` 提到 `--text-strong`，和数值同亮度。
    - 字号估算里的单位宽度系数同步从 `0.52em` 改成 `0.72em`，否则单位变大后「按宽度定字号」会算窄，`1.5MB/s` 这种长值会顶到格子边缘。实测四格墨迹仍在 `x3..81`（内容宽 81）内。
  - **玻璃质感 + 光标跟随（2026-11-08）**：
    - **玻璃层**：卡片底色之上叠三层——磨砂颗粒（内联 SVG `feTurbulence`，140px 平铺、5.5% 不透明度）、顶部反光（`180deg` 白色 5% → 透明）、内阴影（`inset 0 1px 0 rgba(255,255,255,.10)` 提亮上沿 + `inset 0 -24px 48px rgba(0,0,0,.16)` 压暗下沿）。**没有用 `backdrop-filter`**：透明 WebView2 窗口的 backdrop 里没有桌面，模糊只会得到空气，还更费 GPU。
    - **光标跟随**：面板是点击穿透窗口，页内收不到鼠标事件，所以在 Rust 里新加 `pointer.rs`——`GetCursorPos` 每 60ms 采样一次（≈16Hz），只有真的移动过 >2px 才 `emit("pointer")`；前端按「光标是否落在卡片附近（200px）」只更新那几张卡，避免全屏重绘。
    - **高光实现**：卡片/融合块里放一个 480×480 的圆形 `.glow`，用 `transform: translate(var(--gx), var(--gy))` 平移（`will-change: transform`），坐标由 `--px/--py` 与卡片自身 `--cx/--cy` 在 CSS 里算出——只改 transform，走合成器，不重绘渐变。
    - **代价**（实测 6 秒采样，20 线程）：空闲 0.50 CPU·秒；连续移动鼠标经过卡片时 1.86 CPU·秒（≈单核 31%，整机 1.5%），只在移动时产生。
    - **开关**：`panel.json` 新增 `glow`（默认 true），设置页面板面的工具条上有「光标高光」复选框；关掉后实测光标移动不再有任何像素变化。截图 `docs/panel-glass.png`。
  - **设置窗口去掉系统标题栏（2026-11-08）**：窗口改成无边框沉浸式，标题栏由页面自己画。
    - `windows.rs`：`.decorations(false).shadow(true)`。保留 `shadow` 很关键——Windows 会给无边框窗口加 DWM 外框（圆角 + 投影），窗口因此仍然是**可拖边缘缩放**的（实测拖外框右沿 +60 → 宽 1196→1256）。
    - 页面：顶部拆成两层——40px 高的自绘标题栏（品牌 + 保存徽标 + 最小化/最大化/关闭）和工具条（页签 / 主题 / 提示 / 撤销）；标题栏整条 `data-tauri-drag-region`，子元素 `pointer-events: none` 让整条都能拖，只有窗口按钮例外。
    - 新增命令 `settings_window(action)` 处理最小化 / 最大化切换 / 关闭——无边框窗口没有系统按钮了，而且自定义命令不需要额外权限。
    - 实测：无原生标题栏；拖标题栏可移动；最大化 ↔ 还原正常；拖外框边缘可缩放（客户区 1180×800，外框 1196×809，多出来的 8px 就是 DWM 投影/缩放带）。截图 `docs/settings-immersive.png`。

  - **主题系统升级为「块级」（2026-11-08）**：原来主题只有「面板 / HUD」两个整体级（`panel.json`、`hud.json` 顶层的 `theme`），现在每张卡片可以单独指定主题，整体主题退化为兜底默认值。
    - 数据：`CardConfig.theme`（可选，字符串 = 主题 id），Rust/TS 两侧都有；不写就跟随整体。
    - 渲染：`Card.vue` 把主题的 CSS 变量写在卡片根元素上（`themeById(id).vars`），渲染器只引用变量，所以天然按块生效；卡片根元素补了 `color: var(--text)`——否则文字色会从 `body` 继承全局主题的**计算值**，出现「浅色块配深色主题文字」。
    - 融合块：背景只有一块，所以主题按**整块**算——`Stage` 取组内第一个设了主题的成员作为块主题，块背景和组内所有卡片统一使用它；设置窗口里给组内任一卡片改主题，会同时写到整组。
    - 设置窗口：卡片属性新增「主题」下拉（跟随整体 / 暗色玻璃 / 午夜蓝 / 浅色磨砂），改完即时落盘热重载。
    - 实测：同一块 HUD 切三套主题——`default-dark` 底色 `(19,19,22)` 文字 `(232,232,234)`、`midnight` `(14,19,38)` / `(230,236,255)`、`frost` `(207,209,211)` / `(28,32,40)`，块背景与文字色同步切换。截图 `docs/hud-block-themes.png`。
  - **左右排版最终形态：标签贴左、数值贴右（2026-11-08）**：中间试过「整组居中」（左右留白相等，但网络格换量级时整组会左右移 5px），最后定成 `justify-content: space-between`：

    - 标签 `flex: none` 固定贴左，数值 `flex: none` 固定贴右，两列的边界都对得齐；数值位数变化只会在自己的右列里向左伸展，**两个外沿永远不动**；
    - 实测 5 帧：四格的墨迹左沿 / 右沿全部恒定（CPU `0..79`、其余 `1..79`），比居中版更稳；
    - 代价是中间的空隙随数值长度变化（CPU 8px、内存 18px、网络 21px），这是「标签列 / 数值列」这种表格式排版的固有特征。截图 `docs/hud-lr@6x.png`。
  - **标签与数值同字号（2026-11-08）**：原来是标签 9px、数值 ~15.6px 两级排版，现在两行一个字号，只靠**字重 + 亮度**区分（标签 600 / 数值 650，数值保持原来的加粗）。
    - 字号不再是两条独立公式，而是按「最坏情况的标签 + 最坏情况的数值」一次解出来：横向排版 `size = min(高度封顶, (内容宽 − 间隙) / (标签em + 数值em))`，纵向排版 `size = min(封顶, 内容宽 / max(标签em, 数值em))`；结果以 `--fsize` 传给 CSS，标签和数值共用。
    - 小数位与字号耦合：每多一位小数数值就变宽，所以先按 2 位 / 1 位各解一次字号，能把字号压掉 12% 以内的才采用。
    - 实测（87×32 格）：`CPU` 统一字号 **13.5px**（标签墨迹 11px、数值墨迹 12px，粗细靠 600/650 区分），`↑ / ↓` 统一 14.8px；左右留白 4/8 与 16/19，仍然是居中的。
- **块不重叠**：编辑时拖拽/缩放会与其他块做严格相交检测（融合组按整块并集算，边贴边不算重叠）；移动时 X/Y 轴分别尝试滑动，缩放遇到障碍停在最后一个合法尺寸（竖向缩放也不再连带吸附宽度）；属性面板手改 X/Y/宽/高若重叠则忽略并提示；新增卡片自动扫描空位；融合时要求所选卡片本身互不重叠，且合并后的块不能压到其他卡片。**已重叠的块只能往重叠面积减小的方向移动，任何一块的重叠面积都不能变大；完全分开后必须保持零重叠。融合组内成员之间同样按面积判定。**
- 实测：CPU 卡右移停在 `x=72`（邻居左沿 246），下拖缩放停在 `h=184`（融合块上沿 262），手改宽 400 被拒并弹出提示（`docs/collision-notice.png`）；新增卡片落在 (1032, 96)，与现有卡片无重叠。
- **标签实时编辑**：设置里的标签输入框改为 `@input` 实时更新（聚焦时压一次撤销点，保存仍走 150ms 防抖）；此前只在 `change`（失焦/回车）时提交，输入过程中悬浮窗不会变化。
- **内容居中**：数字、状态、电池渲染器内部内容改为水平 + 垂直居中；数字卡大尺寸保持「标签在上、数值在下」纵向居中，小尺寸单行「标签 + 数值」整组居中；文本渲染器未指定 `align` 时默认居中，`align: left/right` 仍可覆盖；环形/电池本来就居中。条形、列表、折线是通栏布局，保持原样。实测截图 `docs/centering-hud.png`、`docs/v1.1-panel.png`。

## 7. v1 内置指标（定死，不做加法）

- **CPU**：总占用率、每核占用率、频率、温度
- **GPU**：占用率、温度、显存占用、风扇转速、功耗
- **内存**：已用/总量、占用率、提交量
- **磁盘**：各分区已用/总量、读写速度
- **网络**：上下行速率、本次开机累计流量
- **系统**：开机时长、当前时间（可选卡片）

默认间隔：负载类 1s，温度/风扇 2s，磁盘容量 10s，网络 1s，电量 60s。

## 8. v1 验收清单

1. 登录后无需操作即出现面板；任务管理器可见主程序 + 采集插件
2. 杀掉 explorer.exe（自动重启）→ 面板 3 秒内恢复
3. 点击/双击/右键面板区域 → 事件穿透，图标操作正常
4. 设置窗口改卡片位置/颜色/指标 → 壁纸层 1 秒内变化
5. 手动 kill 采集插件 → 卡片进入过期态；重启后自动恢复
6. 解压一个第三方插件到插件目录 → 设置窗口出现它，无需重启
7. 空闲 10 分钟 CPU < 1.5%、内存 < 700MB（v1 收尾实测：403MB / 1.26%，计划任务启动、无设置窗口；原 250MB 预算基于 M1 单面板，加入 HUD 的第二个 WebView2 与 .NET 插件后不成立，见实测）
8. 全屏游戏/视频 → HUD 隐藏；退出恢复
9. 退出程序 → 壁纸恢复原样，无残留透明窗口

## 9. 明确不做

进程 TopN、网络按进程拆分、电池（台式机）、音频、天气等外部数据、任务栏挂件、锁屏面板、插件市场、自动更新、多平台。

## 9.5 M1 实测结论（2026-11-05，Windows 11 build 26200）

**桌面结构**：`Progman` 带 `WS_EX_NOREDIRECTIONBITMAP`（DComp 合成），`SHELLDLL_DefView` 是它的 `WS_EX_LAYERED` 子窗口；发 `0x052C` 后 `Progman` 下会新建一个可见的 `WorkerW` 壁纸宿主。

**实测结果**：以下四种方式创建的面板**全部不可见**（窗口状态均为可见、未 cloaked、父窗口与 z 序正确）：

1. `WS_CHILD` 直接以 `Progman` / `WorkerW` / `SHELLDLL_DefView` 为父创建
2. 上述窗口再补 `WS_EX_LAYERED`（alpha=255，含 `SWP_FRAMECHANGED`）
3. Lively 现行配方：顶层窗 → `WS_CHILD` → `WS_EX_LAYERED` → `SetParent(Progman)` → 插到 DefView 之下 → WorkerW 沉底
4. 同一配方的 **C# WPF** 实现（`AllowsTransparency=true`），在沙箱内外结果一致

顶层窗口（置顶或置底）则**正常显示**。结论：该 build 上第三方进程的窗口无法进入桌面窗口的合成层，与语言、渲染后端（GDI/WPF）无关。

**当前可行方案**：置底（`HWND_BOTTOM`）+ 点击穿透（`WS_EX_TRANSPARENT`）+ 不抢焦点（`WS_EX_NOACTIVATE`）的**顶层透明窗口**（`LWA_COLORKEY` 或 WebView2 透明背景）。已实测可见、壁纸与图标透过透明区正常显示。
代价：位于桌面图标**之上**（卡片重叠处会遮住图标，但点击仍穿透到图标）；Win+D 会把它最小化，需要监听后自恢复；本质上不是"图标之下"。

**未验证的更高阶路线**：按微软说明用 DirectComposition 向分层子窗口呈现（`WS_EX_NOREDIRECTIONBITMAP` + DComp visual 或 WebView2 composition controller），可能在图标之下成立。属研究性质，不作为 v1 前提。

## 10. 风险与退路

| 风险 | 退路 |
|---|---|
| WebView2 透明合成在 WorkerW 下失败（M1 第一件事验证） | 不透明深色卡片背景；或面板层改 Direct2D 绘制、只把设置界面留给 WebView |
| LibreHardwareMonitor 对 AMD 7800 XT 的传感器覆盖不全 | 以「负载类必达、传感器类尽力」定级，缺失指标藏起来而非显示 0 |
| 虚拟显示器（ToDesk/向日葵）污染枚举 | 按驱动名分类，默认折叠 |
| WorkerW 挂载属未公开行为，Win11 更新可能失效 | 挂载逻辑集中在 wallpaper.rs，可整体替换；挂载失败时降级为普通置顶窗并在托盘中提示 |
