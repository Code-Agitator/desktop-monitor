// desktop-monitor 设备电量插件（协议 v1）
// 数据来源：
//   1) Windows 设备属性 System.Devices.BatteryLife（系统「蓝牙和其他设备」里显示的电量）
//   2) BLE GATT 电池服务 0x180F / 特征 0x2A19
// 以 --dump 运行可打印本机可读到的带电量设备。
//
// 设备集合会变化（连上/断开），插件通过 JSON-RPC 通知 "metrics" 重报指标列表，
// 宿主据此增删指标（协议 v1 的向后兼容扩展，无 id 的通知）。

using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using Windows.Devices.Bluetooth;
using Windows.Devices.Bluetooth.GenericAttributeProfile;
using Windows.Devices.Enumeration;
using Windows.Storage.Streams;

namespace DeviceBattery;

internal sealed record MetricDef(
    string Id,
    string Title,
    string Kind,
    string? Unit,
    int IntervalMs,
    bool History,
    double? Min,
    double? Max);

internal sealed record ListRow(string Label, string Value, double? Ratio);

internal sealed record ErrorValue(string Error);

internal sealed class BatteryDevice
{
    public required string Name { get; init; }
    public required string Key { get; init; }
    public int Level { get; set; }
    public bool Charging { get; set; }
    public bool Connected { get; set; } = true;
    public string Source { get; set; } = "windows";
}

internal static class Program
{
    private static readonly JsonSerializerOptions Json = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private static readonly object WriteGate = new();
    private static StreamWriter? _stdout;

    private static void Send(object payload)
    {
        lock (WriteGate)
        {
            _stdout?.WriteLine(JsonSerializer.Serialize(payload, Json));
        }
    }

    private static void NotifyMetrics(List<MetricDef> metrics) => Send(new
    {
        jsonrpc = "2.0",
        method = "metrics",
        @params = new { metrics },
    });

    internal static void Log(string level, string message) => Send(new
    {
        jsonrpc = "2.0",
        method = "log",
        @params = new { level, message },
    });

    private static void Main(string[] args)
    {
        if (args.Contains("--features"))
        {
            foreach (var featLine in LogitechHidpp.Features()) Console.WriteLine(featLine);
            return;
        }

        if (args.Contains("--metrics"))
        {
            using var svc = new Collector(false);
            foreach (var m in svc.Initialize())
                Console.WriteLine($"{m.Id}\t{m.Title}");
            return;
        }

        if (args.Contains("--hid"))
        {
            foreach (var l in AulaBattery.Dump()) Console.WriteLine(l);
            return;
        }

        if (args.Contains("--aula"))
        {
            Console.WriteLine("== 狼蛛键盘电量探测 ==");
            var kb = AulaBattery.Read(attempts: 4, windowMs: 1200, verbose: true);
            Console.WriteLine(kb is null
                ? "   没读到（键盘可能休眠/未连接）"
                : $"   {kb.Name} {kb.Level}%  充电={kb.Charging}");
            return;
        }

        if (args.Contains("--logi"))
        {
            Console.WriteLine("== 罗技 HID++ 详细探测 ==");
            var rs = LogitechHidpp.Read(budgetMs: 8000, verbose: true);
            foreach (var r in rs) Console.WriteLine($"   -> {r.Name} {r.Level}% 充电={r.Charging}");
            if (rs.Count == 0) Console.WriteLine("   （没有结果）");
            return;
        }

        if (args.Contains("--dump"))
        {
            Collector.Dump().GetAwaiter().GetResult();
            return;
        }

        using var reader = new StreamReader(Console.OpenStandardInput(), Encoding.UTF8);
        _stdout = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false))
        {
            AutoFlush = true,
        };

        Collector? collector = null;
        var demo = args.Contains("--demo");

        string? line;
        while ((line = reader.ReadLine()) is not null)
        {
            if (string.IsNullOrWhiteSpace(line)) continue;

            JsonNode? req;
            try
            {
                req = JsonNode.Parse(line);
            }
            catch
            {
                continue;
            }

            var id = req?["id"]?.GetValue<long>() ?? 0;
            var method = req?["method"]?.GetValue<string>();

            try
            {
                switch (method)
                {
                    case "initialize":
                    {
                        collector ??= new Collector(demo);
                        collector.MetricsChanged = NotifyMetrics;
                        var metrics = collector.Initialize();
                        Send(new
                        {
                            jsonrpc = "2.0",
                            id,
                            result = new
                            {
                                protocolVersion = 1,
                                plugin = new { id = Collector.PluginId, version = "0.1.0" },
                                metrics,
                            },
                        });
                        break;
                    }
                    case "poll":
                    {
                        var ids = req?["params"]?["ids"]?.AsArray()
                            .Select(n => n?.GetValue<string>())
                            .Where(s => s is not null)
                            .Select(s => s!)
                            .ToList() ?? new List<string>();
                        var values = collector?.Poll(ids) ?? new Dictionary<string, object>();
                        Send(new { jsonrpc = "2.0", id, result = new { values } });
                        break;
                    }
                    case "setConfig":
                        Send(new { jsonrpc = "2.0", id, result = (object?)null });
                        break;
                    case "shutdown":
                        Send(new { jsonrpc = "2.0", id, result = (object?)null });
                        collector?.Dispose();
                        return;
                    default:
                        Send(new
                        {
                            jsonrpc = "2.0",
                            id,
                            error = new { code = -32601, message = $"未知方法 {method}" },
                        });
                        break;
                }
            }
            catch (Exception e)
            {
                Send(new
                {
                    jsonrpc = "2.0",
                    id,
                    error = new { code = -32000, message = e.Message },
                });
            }
        }
    }
}

internal sealed class Collector : IDisposable
{
    public const string PluginId = "device-battery";
    private const int IntervalMs = 60_000;
    private static readonly TimeSpan RefreshEvery = TimeSpan.FromSeconds(30);

    private readonly object _gate = new();
    private readonly Dictionary<string, BatteryDevice> _devices = new();
    private List<MetricDef> _metrics = new();
    private volatile bool _stopping;
    private Thread? _refresher;
    private readonly bool _demo;

    public Collector(bool demo = false)
    {
        _demo = demo;
    }

    /// 由主循环注入：设备集合变化时重报指标
    public Action<List<MetricDef>>? MetricsChanged { get; set; }

    public List<MetricDef> Initialize()
    {
        lock (_gate)
        {
            if (_demo)
            {
                _devices["demo-device"] = new BatteryDevice
                {
                    Name = "演示设备（模拟）",
                    Key = "demo-device",
                    Level = 82,
                    Charging = false,
                    Source = "demo",
                };
            }
            else
            {
                ReplaceDevices(DiscoverAsync(probeAllBle: false).GetAwaiter().GetResult());
            }
            _metrics = BuildMetrics();
        }

        if (!_demo)
        {
            _refresher = new Thread(RefreshLoop) { IsBackground = true, Name = "battery-refresh" };
            _refresher.Start();
        }
        return _metrics;
    }

    private void RefreshLoop()
    {
        while (!_stopping)
        {
            for (var slept = 0; slept < RefreshEvery.TotalMilliseconds && !_stopping; slept += 500)
                Thread.Sleep(500);
            if (_stopping) break;

            try
            {
                var found = DiscoverAsync(probeAllBle: false).GetAwaiter().GetResult();
                List<MetricDef>? changed = null;
                lock (_gate)
                {
                    if (ReplaceDevices(found))
                    {
                        _metrics = BuildMetrics();
                        changed = _metrics;
                    }
                }
                if (changed is not null)
                {
                    Program.Log("info", $"设备集合变化，{changed.Count} 个指标");
                    MetricsChanged?.Invoke(changed);
                }
            }
            catch (Exception e)
            {
                Program.Log("warn", $"刷新设备失败：{e.Message}");
            }
        }
    }

    /// <summary>返回 true 表示设备集合（键或名称）发生变化。</summary>
    private bool ReplaceDevices(Dictionary<string, BatteryDevice> fresh)
    {
        var oldKeys = _devices.Keys.OrderBy(k => k).ToList();
        var newKeys = fresh.Keys.OrderBy(k => k).ToList();
        var changed = !oldKeys.SequenceEqual(newKeys);

        foreach (var (key, device) in fresh)
        {
            if (_devices.TryGetValue(key, out var old) && old.Name != device.Name)
                changed = true;
        }

        _devices.Clear();
        foreach (var (key, device) in fresh) _devices[key] = device;
        return changed;
    }

    private List<MetricDef> BuildMetrics()
    {
        var list = new List<MetricDef>
        {
            new("battery.devices", "设备电量", "list", null, IntervalMs, false, null, null),
            new("battery.status", "电量状态", "state", null, IntervalMs, false, null, null),
        };
        foreach (var d in _devices.Values.OrderBy(d => d.Name, StringComparer.CurrentCulture))
        {
            list.Add(new MetricDef($"battery.{d.Key}.level", $"{d.Name} 电量", "ratio", "%", IntervalMs, true, 0, 1));
            list.Add(new MetricDef($"battery.{d.Key}.state", $"{d.Name} 状态", "state", null, IntervalMs, false, null, null));
        }
        return list;
    }

    public Dictionary<string, object> Poll(List<string> ids)
    {
        var result = new Dictionary<string, object>();
        List<BatteryDevice> snapshot;
        lock (_gate) snapshot = _devices.Values.ToList();
        var byKey = snapshot.ToDictionary(d => d.Key);

        foreach (var id in ids)
        {
            try
            {
                if (id == "battery.devices")
                {
                    result[id] = snapshot
                        .OrderBy(d => d.Name, StringComparer.CurrentCulture)
                        .Select(d => new ListRow(d.Name, $"{d.Level}%", d.Level / 100.0))
                        .ToArray();
                }
                else if (id == "battery.status")
                {
                    result[id] = snapshot.Count == 0
                        ? "未发现带电量设备"
                        : $"{snapshot.Count} 个设备在线";
                }
                else
                {
                    var parts = id.Split('.');
                    if (parts.Length >= 3 && byKey.TryGetValue(parts[1], out var device))
                    {
                        if (parts[^1] == "level") result[id] = device.Level / 100.0;
                        else if (parts[^1] == "state") result[id] = device.Charging ? "充电中" : "使用中";
                        else result[id] = new ErrorValue("未知字段");
                    }
                    else
                    {
                        result[id] = new ErrorValue("设备已移除");
                    }
                }
            }
            catch (Exception e)
            {
                result[id] = new ErrorValue(e.Message);
            }
        }
        return result;
    }

    // ---------- 发现 ----------

    private async Task<Dictionary<string, BatteryDevice>> DiscoverAsync(bool probeAllBle)
    {
        var found = new Dictionary<string, BatteryDevice>(StringComparer.OrdinalIgnoreCase);
        var errors = new List<string>();

        // 1) Windows 已汇总的电量
        try
        {
            var props = new[]
            {
                "System.Devices.BatteryLife",
                "System.Devices.BatteryPlusCharging",
                "System.Devices.BatteryPlusChargingText",
                "System.Devices.Aep.DeviceAddress",
            };
            var selectors = new[]
            {
                BluetoothDevice.GetDeviceSelector(),
                BluetoothLEDevice.GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus.Connected),
            };
            foreach (var selector in selectors)
            {
                DeviceInformationCollection infos;
                try
                {
                    infos = await DeviceInformation.FindAllAsync(selector, props);
                }
                catch
                {
                    continue;
                }
                foreach (var info in infos)
                {
                    if (!info.Properties.TryGetValue("System.Devices.BatteryLife", out var raw) || raw is null)
                        continue;
                    int level;
                    try { level = Convert.ToInt32(raw); }
                    catch { continue; }
                    if (level is <= 0 or > 100) continue;

                    var charging = info.Properties.TryGetValue("System.Devices.BatteryPlusCharging", out var c)
                        ? Convert.ToInt32(c) == 1
                        : false;
                    if (info.Properties.TryGetValue("System.Devices.BatteryPlusChargingText", out var ct)
                        && ct is string text)
                    {
                        charging |= text.Contains("charg", StringComparison.OrdinalIgnoreCase)
                                    || text.Contains("充电", StringComparison.Ordinal);
                    }

                    var name = string.IsNullOrWhiteSpace(info.Name) ? "未知设备" : info.Name;
                    AddCandidate(found, name, info.Id, level, charging, "windows");
                }
            }
        }
        catch (Exception e)
        {
            errors.Add($"系统属性：{e.Message}");
        }

        // 2) BLE GATT 电池服务
        try
        {
            var selector = probeAllBle
                ? BluetoothLEDevice.GetDeviceSelector()
                : BluetoothLEDevice.GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus.Connected);
            var infos = await DeviceInformation.FindAllAsync(selector);
            foreach (var info in infos)
            {
                BluetoothLEDevice? device = null;
                try
                {
                    device = await BluetoothLEDevice.FromIdAsync(info.Id);
                    if (device is null || device.ConnectionStatus != BluetoothConnectionStatus.Connected)
                        continue;
                    // 已有系统级电量时不再读 GATT，省时间
                    if (found.Keys.Any(k => k.Contains(SlugName(device.Name ?? info.Name))))
                        continue;

                    var services = await device.GetGattServicesForUuidAsync(
                        GattServiceUuids.Battery, BluetoothCacheMode.Uncached);
                    if (services.Status != GattCommunicationStatus.Success) continue;

                    foreach (var service in services.Services)
                    {
                        var chars = await service.GetCharacteristicsForUuidAsync(
                            GattCharacteristicUuids.BatteryLevel, BluetoothCacheMode.Uncached);
                        if (chars.Status != GattCommunicationStatus.Success) continue;
                        foreach (var ch in chars.Characteristics)
                        {
                            var value = await ch.ReadValueAsync(BluetoothCacheMode.Uncached);
                            if (value.Status != GattCommunicationStatus.Success || value.Value is null) continue;
                            var level = ReadByte(value.Value);
                            if (level is <= 0 or > 100) continue;
                            var name = string.IsNullOrWhiteSpace(device.Name) ? info.Name : device.Name;
                            AddCandidate(found, name ?? "BLE 设备", $"ble|{device.BluetoothAddress:X}", level, false, "ble");
                        }
                    }
                }
                catch (Exception e)
                {
                    errors.Add($"BLE {info.Name}：{e.Message}");
                }
                finally
                {
                    device?.Dispose();
                }
            }
        }
        catch (Exception e)
        {
            errors.Add($"BLE 枚举：{e.Message}");
        }

        // 3) 罗技 HID++（2.4G 接收器上的无线鼠标，仅 Windows 私有协议能读到）
        try
        {
            foreach (var r in LogitechHidpp.Read())
            {
                AddCandidate(found, r.Name, "hidpp|" + r.Name, r.Level, r.Charging, "hidpp");
            }
        }
        catch (Exception e)
        {
            errors.Add($"罗技 HID++：{e.Message}");
        }

        // 4) 狼蛛键盘（AULA F87 PRO，厂商私有 20 字节报文）
        try
        {
            var kb = AulaBattery.Read(attempts: 5, windowMs: 1100);
            if (kb is not null)
                AddCandidate(found, kb.Name, "aula|" + kb.Name, kb.Level, kb.Charging, "aula");
        }
        catch (Exception e)
        {
            errors.Add($"狼蛛键盘：{e.Message}");
        }

        // 罗技鼠标休眠时读不到：沿用上一次的读数，免得指标忽有忽无
        foreach (var src in new[] { "hidpp", "aula" })
        {
            if (found.Values.Any(d => d.Source == src)) continue;
            foreach (var old in _devices.Values.Where(d => d.Source == src))
                found[old.Key] = old;
        }

        if (errors.Count > 0)
            Program.Log("info", "部分来源不可用：" + string.Join("；", errors));

        return found;
    }

    private static void AddCandidate(
        Dictionary<string, BatteryDevice> found,
        string name,
        string sourceId,
        int level,
        bool charging,
        string source)
    {
        // 同一设备可能同时出现在多个来源：按名称去重，先到先得（系统属性优先）
        var slug = SlugName(name);
        if (string.IsNullOrEmpty(slug)) slug = "dev";
        var keyBase = $"{slug}-{Hash8(name.ToLowerInvariant())}";
        var key = keyBase;
        var n = 2;
        while (found.ContainsKey(key))
        {
            if (found[key].Name.Equals(name, StringComparison.OrdinalIgnoreCase))
            {
                // 同名同设备：保留已有（更高优先级）来源
                return;
            }
            key = $"{keyBase}-{n++}";
        }
        found[key] = new BatteryDevice
        {
            Name = name.Length > 32 ? name[..32] : name,
            Key = key,
            Level = level,
            Charging = charging,
            Source = source,
        };
    }

    private static byte ReadByte(IBuffer buffer)
    {
        using var reader = DataReader.FromBuffer(buffer);
        return reader.ReadByte();
    }

    private static string SlugName(string name)
    {
        var sb = new StringBuilder();
        foreach (var ch in name.ToLowerInvariant())
        {
            if (ch is >= 'a' and <= 'z' or >= '0' and <= '9') sb.Append(ch);
            else if (sb.Length > 0 && sb[^1] != '-') sb.Append('-');
        }
        var s = sb.ToString().Trim('-');
        return s.Length > 24 ? s[..24].TrimEnd('-') : s;
    }

    private static string Hash8(string text)
    {
        var bytes = SHA256.HashData(Encoding.UTF8.GetBytes(text));
        return Convert.ToHexString(bytes)[..8].ToLowerInvariant();
    }

    public void Dispose()
    {
        _stopping = true;
    }

    // ---------- 诊断 ----------

    public static async Task Dump()
    {
        Console.WriteLine("== 系统设备属性（System.Devices.BatteryLife）==");
        var props = new[]
        {
            "System.Devices.BatteryLife",
            "System.Devices.BatteryPlusCharging",
            "System.Devices.BatteryPlusChargingText",
            "System.Devices.Aep.DeviceAddress",
        };
        foreach (var selectorName in new[] { "BluetoothDevice", "BluetoothLE(Connected)" })
        {
            var selector = selectorName == "BluetoothDevice"
                ? BluetoothDevice.GetDeviceSelector()
                : BluetoothLEDevice.GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus.Connected);
            Console.WriteLine($"-- {selectorName}");
            var infos = await DeviceInformation.FindAllAsync(selector, props);
            foreach (var info in infos)
            {
                var battery = info.Properties.TryGetValue("System.Devices.BatteryLife", out var b)
                    ? (b?.ToString() ?? "(null)")
                    : "(无)";
                var charging = info.Properties.TryGetValue("System.Devices.BatteryPlusChargingText", out var c)
                    ? (c?.ToString() ?? "(null)")
                    : "(无)";
                Console.WriteLine($"   {info.Name,-28} battery={battery,-6} charging={charging,-8} kind={info.Kind} id={info.Id}");
            }
        }

        Console.WriteLine();
        Console.WriteLine("== BLE GATT 电池服务 0x180F ==");
        var ble = await DeviceInformation.FindAllAsync(
            BluetoothLEDevice.GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus.Connected));
        foreach (var info in ble)
        {
            BluetoothLEDevice? dev = null;
            try
            {
                dev = await BluetoothLEDevice.FromIdAsync(info.Id);
                if (dev is null)
                {
                    Console.WriteLine($"   {info.Name,-28} -> FromIdAsync = null");
                    continue;
                }
                var services = await dev.GetGattServicesForUuidAsync(
                    GattServiceUuids.Battery, BluetoothCacheMode.Uncached);
                Console.WriteLine($"   {info.Name,-28} conn={dev.ConnectionStatus} gatt={services.Status}");
                if (services.Status != GattCommunicationStatus.Success) continue;
                foreach (var s in services.Services)
                {
                    var chars = await s.GetCharacteristicsForUuidAsync(
                        GattCharacteristicUuids.BatteryLevel, BluetoothCacheMode.Uncached);
                    Console.WriteLine($"      char status={chars.Status} count={chars.Characteristics.Count}");
                    foreach (var ch in chars.Characteristics)
                    {
                        var v = await ch.ReadValueAsync(BluetoothCacheMode.Uncached);
                        Console.WriteLine($"      read={v.Status} level={(v.Value is null ? "-" : ReadByte(v.Value).ToString())}");
                    }
                }
            }
            catch (Exception e)
            {
                Console.WriteLine($"   {info.Name,-28} -> 异常 {e.GetType().Name}: {e.Message}");
            }
            finally
            {
                dev?.Dispose();
            }
        }

        Console.WriteLine();
        Console.WriteLine("== 罗技 HID++ 2.0（2.4G 接收器）==");
        try
        {
            var logi = LogitechHidpp.Read(budgetMs: 4000);
            if (logi.Count == 0) Console.WriteLine("   （没有读到；接收器上的鼠标可能处于休眠）");
            foreach (var r in logi) Console.WriteLine($"   {r.Name,-40} {r.Level}%  充电={r.Charging}");
        }
        catch (Exception e)
        {
            Console.WriteLine($"   异常 {e.GetType().Name}: {e.Message}");
        }

        Console.WriteLine();
        Console.WriteLine("== HID Battery Strength (Usage Page 0x06 / Usage 0x20) ==");
        try
        {
            var hid = HidBattery.Probe(verbose: true);
            if (hid.Count == 0) Console.WriteLine("   （未发现实现标准电量上报的 HID 设备）");
            foreach (var (name, level) in hid) Console.WriteLine($"   {name,-40} {level}%");
        }
        catch (Exception e)
        {
            Console.WriteLine($"   HID 枚举异常：{e.GetType().Name}: {e.Message}");
        }
    }
}
