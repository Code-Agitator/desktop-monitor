// desktop-monitor 系统采集插件（协议 v1）
// 通过 LibreHardwareMonitor 读取 CPU/GPU 传感器；内存、磁盘、网络用系统 API。
// 以 --dump 运行可打印本机所有硬件传感器，便于核对名称。

using System.Runtime.InteropServices;
using System.Net.NetworkInformation;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using LibreHardwareMonitor.Hardware;

namespace SysCollector;

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

internal static class Program
{
    private static readonly JsonSerializerOptions Json = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private static Collector? _collector;

    private static void Main(string[] args)
    {
        if (args.Contains("--dump"))
        {
            Collector.Dump();
            return;
        }

        using var reader = new StreamReader(Console.OpenStandardInput(), Encoding.UTF8);
        var stdout = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false))
        {
            AutoFlush = true,
        };

        void Send(object payload) => stdout.WriteLine(JsonSerializer.Serialize(payload, Json));

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
                        _collector ??= new Collector();
                        var metrics = _collector.Initialize();
                        Send(new
                        {
                            jsonrpc = "2.0",
                            id,
                            result = new
                            {
                                protocolVersion = 1,
                                plugin = new { id = "sys-collector", version = "0.1.0" },
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
                        var values = _collector?.Poll(ids) ?? new Dictionary<string, object>();
                        Send(new { jsonrpc = "2.0", id, result = new { values } });
                        break;
                    }
                    case "setConfig":
                        Send(new { jsonrpc = "2.0", id, result = (object?)null });
                        break;
                    case "shutdown":
                        Send(new { jsonrpc = "2.0", id, result = (object?)null });
                        _collector?.Dispose();
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
    private readonly Computer _computer;
    private readonly List<MetricDef> _metrics = new();
    private readonly Dictionary<string, Func<object>> _getters = new();
    private readonly Dictionary<string, IHardware> _hardwareOf = new();
    private readonly bool _isAdmin;
    private bool _sensorsAvailable;

    private long _netAt;
    private long _netRx;
    private long _netTx;
    private (double Down, double Up) _netCache;

    public Collector()
    {
        _isAdmin = new WindowsPrincipal(WindowsIdentity.GetCurrent())
            .IsInRole(WindowsBuiltInRole.Administrator);

        _computer = new Computer
        {
            IsCpuEnabled = true,
            IsGpuEnabled = true,
            IsMemoryEnabled = true,
            IsStorageEnabled = true,
            IsNetworkEnabled = true,
            IsMotherboardEnabled = false,
            IsControllerEnabled = false,
        };
        _computer.Open();
        Console.Error.WriteLine(
            $"[collector] ctx admin={_isAdmin} session={System.Diagnostics.Process.GetCurrentProcess().SessionId} " +
            $"cwd={Environment.CurrentDirectory} temp={Path.GetTempPath()}");
        var cpuHw = _computer.Hardware.FirstOrDefault(h => h.HardwareType == HardwareType.Cpu);
        if (cpuHw is not null)
        {
            cpuHw.Update();
            var temps = cpuHw.Sensors
                .Where(s => s.SensorType == SensorType.Temperature)
                .Take(4)
                .Select(s => $"{s.Name}={s.Value?.ToString() ?? "null"}");
            Console.Error.WriteLine($"[collector] cpu temps: {string.Join(", ", temps)}");
        }
    }

    public List<MetricDef> Initialize()
    {
        // 首次打开后部分传感器（温度/频率）需要一到两轮才会出值
        for (var i = 0; i < 3; i++)
        {
            foreach (var hw in _computer.Hardware) hw.Update();
            Thread.Sleep(120);
        }

        BuildCpu();
        BuildGpu();
        BuildMemory();
        BuildStorage();
        BuildNetwork();
        BuildSystem();
        return _metrics;
    }

    private void Add(MetricDef def, Func<object> getter, IHardware? hardware = null)
    {
        _metrics.Add(def);
        _getters[def.Id] = getter;
        if (hardware is not null) _hardwareOf[def.Id] = hardware;
    }

    private static ISensor? Find(IHardware hw, SensorType type, params string[] hints)
    {
        var sensors = hw.Sensors
            .Where(s => s.SensorType == type)
            .ToList();
        foreach (var hint in hints)
        {
            var match = sensors.FirstOrDefault(s =>
                s.Name.Contains(hint, StringComparison.OrdinalIgnoreCase));
            if (match is not null) return match;
        }
        return null;
    }

    private void BuildCpu()
    {
        var cpu = _computer.Hardware.FirstOrDefault(h => h.HardwareType == HardwareType.Cpu);
        if (cpu is null) return;
        cpu.Update();

        var total = Find(cpu, SensorType.Load, "CPU Total", "Total");
        if (total is not null)
        {
            Add(
                new MetricDef("sys.cpu.usage", "CPU 使用率", "ratio", "%", 1000, true, 0, 1),
                () => (total.Value ?? 0) / 100.0,
                cpu);
        }

        var temp = Find(cpu, SensorType.Temperature, "Tctl", "Tdie", "Package", "Core Average")
                   ?? cpu.Sensors.FirstOrDefault(s =>
                       s.SensorType == SensorType.Temperature && s.Value.HasValue);
        Console.Error.WriteLine($"[collector] cpu temp sensor = {temp?.Name ?? "(none)"} value={temp?.Value}");
        _sensorsAvailable = temp?.Value.HasValue == true;
        if (temp is not null && temp.Value.HasValue)
        {
            Add(
                new MetricDef("sys.cpu.temp", "CPU 温度", "scalar", "°C", 2000, false, 0, 110),
                () => (double)(temp.Value ?? 0),
                cpu);
        }

        var clocks = cpu.Sensors
            .Where(s => s.SensorType == SensorType.Clock)
            .Where(s => !s.Name.Contains("Bus", StringComparison.OrdinalIgnoreCase))
            .ToList();
        Console.Error.WriteLine($"[collector] clock sensors = {clocks.Count} first={clocks.FirstOrDefault()?.Name}={clocks.FirstOrDefault()?.Value}");
        if (clocks.Count > 0 && clocks.Any(c => c.Value.HasValue))
        {
            Add(
                new MetricDef("sys.cpu.freq", "CPU 频率", "scalar", "MHz", 2000, false, null, null),
                () => clocks.Average(c => (double)(c.Value ?? 0)),
                cpu);
        }

        static bool IsCoreLoad(ISensor s) =>
            s.SensorType == SensorType.Load
            && s.Name.StartsWith("CPU Core #", StringComparison.OrdinalIgnoreCase)
            && !s.Name.Contains("Thread", StringComparison.OrdinalIgnoreCase)
            && !s.Name.Contains("Max", StringComparison.OrdinalIgnoreCase);

        var coreLoads = cpu.Sensors
            .Where(IsCoreLoad)
            .OrderBy(s => int.TryParse(s.Name.Split('#')[1], out var n) ? n : 0)
            .ToList();
        if (coreLoads.Count > 0)
        {
            Add(
                new MetricDef("sys.cpu.cores", "每核占用", "list", null, 2000, false, null, null),
                () => coreLoads
                    .Select(s => new ListRow(
                        s.Name.Replace("CPU ", string.Empty),
                        $"{Math.Round(s.Value ?? 0)}%",
                        (s.Value ?? 0) / 100.0))
                    .ToArray(),
                cpu);
        }
    }

    private void BuildGpu()
    {
        var gpu = _computer.Hardware.FirstOrDefault(h =>
            h.HardwareType is HardwareType.GpuAmd or HardwareType.GpuNvidia or HardwareType.GpuIntel);
        if (gpu is null) return;
        gpu.Update();

        var load = Find(gpu, SensorType.Load, "GPU Core", "D3D 3D", "GPU");
        if (load is not null)
        {
            Add(
                new MetricDef("sys.gpu.usage", "GPU 使用率", "ratio", "%", 1000, true, 0, 1),
                () => (load.Value ?? 0) / 100.0,
                gpu);
        }

        var temp = Find(gpu, SensorType.Temperature, "GPU Core", "GPU Hot Spot", "GPU");
        if (temp is not null)
        {
            Add(
                new MetricDef("sys.gpu.temp", "GPU 温度", "scalar", "°C", 2000, false, 0, 110),
                () => (double)(temp.Value ?? 0),
                gpu);
        }

        var power = Find(gpu, SensorType.Power, "GPU Package", "GPU Power", "GPU Core");
        if (power is not null)
        {
            Add(
                new MetricDef("sys.gpu.power", "GPU 功耗", "scalar", "W", 2000, false, 0, 400),
                () => (double)(power.Value ?? 0),
                gpu);
        }

        var fan = Find(gpu, SensorType.Fan, "GPU Fan", "Fan");
        if (fan is not null)
        {
            Add(
                new MetricDef("sys.gpu.fan", "GPU 风扇", "scalar", "RPM", 3000, false, 0, 4000),
                () => (double)(fan.Value ?? 0),
                gpu);
        }

        var mem = Find(gpu, SensorType.SmallData, "GPU Memory Used", "D3D Dedicated Memory Used", "Memory Used")
                  ?? Find(gpu, SensorType.Data, "GPU Memory Used");
        if (mem is not null)
        {
            Add(
                new MetricDef("sys.gpu.mem", "显存占用", "bytes", "B", 2000, false, null, null),
                () => (double)(mem.Value ?? 0) * 1024 * 1024,
                gpu);
        }
    }

    private static (ulong Total, ulong Avail) MemInfo()
    {
        var st = new MEMORYSTATUSEX { dwLength = (uint)Marshal.SizeOf<MEMORYSTATUSEX>() };
        return Native.GlobalMemoryStatusEx(ref st) ? (st.ullTotalPhys, st.ullAvailPhys) : (0, 0);
    }

    private void BuildMemory()
    {
        var probe = MemInfo();
        if (probe.Total == 0) return;

        Add(
            new MetricDef("sys.mem.usage", "内存占用", "ratio", "%", 1000, true, 0, 1),
            () =>
            {
                var (total, avail) = MemInfo();
                return total == 0 ? 0 : (double)(total - avail) / total;
            });

        Add(
            new MetricDef("sys.mem.used", "已用内存", "bytes", "B", 1000, false, null, null),
            () =>
            {
                var (total, avail) = MemInfo();
                return (double)(total - avail);
            });

        var ok = Native.GetPerformanceInfo(out var probePerf, (uint)Marshal.SizeOf<PERFORMANCE_INFORMATION>());
        if (ok && probePerf.PageSize != IntPtr.Zero)
        {
            Add(
                new MetricDef("sys.mem.commit", "提交内存", "bytes", "B", 2000, false, null, null),
                () =>
                {
                    if (!Native.GetPerformanceInfo(out var perf, (uint)Marshal.SizeOf<PERFORMANCE_INFORMATION>()))
                        return 0d;
                    return (double)perf.CommitTotal.ToInt64() * perf.PageSize.ToInt64();
                });
        }
    }

    private void BuildStorage()
    {
        Add(
            new MetricDef("sys.disk.parts", "磁盘分区", "list", null, 5000, false, null, null),
            () =>
            {
                var rows = new List<ListRow>();
                foreach (var d in DriveInfo.GetDrives())
                {
                    if (d.DriveType != DriveType.Fixed || !d.IsReady) continue;
                    var total = d.TotalSize;
                    if (total <= 0) continue;
                    var free = d.TotalFreeSpace;
                    rows.Add(new ListRow(
                        d.Name.TrimEnd('\\'),
                        $"{Bytes(free)} 可用 / {Bytes(total)}",
                        (double)(total - free) / total));
                }
                return rows.ToArray();
            });

        var storages = _computer.Hardware.Where(h => h.HardwareType == HardwareType.Storage).ToList();
        if (storages.Count == 0) return;

        var readSensors = storages
            .SelectMany(s => s.Sensors)
            .Where(s => s.SensorType == SensorType.Data && s.Name.Equals("Data Read", StringComparison.OrdinalIgnoreCase))
            .ToList();
        var writeSensors = storages
            .SelectMany(s => s.Sensors)
            .Where(s => s.SensorType == SensorType.Data && s.Name.Equals("Data Written", StringComparison.OrdinalIgnoreCase))
            .ToList();

        if (readSensors.Count > 0 || writeSensors.Count > 0)
        {
            Add(
                new MetricDef("sys.disk.read", "磁盘读取", "bytes", "B/s", 1000, true, null, null),
                () => DiskRate().Read);
            Add(
                new MetricDef("sys.disk.write", "磁盘写入", "bytes", "B/s", 1000, true, null, null),
                () => DiskRate().Write);
        }
    }

    private (double Read, double Write) _diskCache;
    private long _diskAt;
    private double _diskReadLast;
    private double _diskWriteLast;

    private (double Read, double Write) DiskRate()
    {
        var now = Environment.TickCount64;
        if (now - _diskAt < 100) return _diskCache;

        foreach (var hw in _computer.Hardware.Where(h => h.HardwareType == HardwareType.Storage))
        {
            hw.Update();
        }

        double read = 0;
        double write = 0;
        foreach (var hw in _computer.Hardware.Where(h => h.HardwareType == HardwareType.Storage))
        {
            foreach (var s in hw.Sensors.Where(s => s.SensorType == SensorType.Data && s.Value.HasValue))
            {
                if (s.Name.Equals("Data Read", StringComparison.OrdinalIgnoreCase)) read += s.Value ?? 0;
                else if (s.Name.Equals("Data Written", StringComparison.OrdinalIgnoreCase)) write += s.Value ?? 0;
            }
        }

        const double GiB = 1024d * 1024d * 1024d;
        if (_diskAt > 0 && now > _diskAt)
        {
            var dt = (now - _diskAt) / 1000.0;
            _diskCache = (
                Math.Max(0, (read - _diskReadLast) * GiB / dt),
                Math.Max(0, (write - _diskWriteLast) * GiB / dt));
        }
        _diskAt = now;
        _diskReadLast = read;
        _diskWriteLast = write;
        return _diskCache;
    }

    private void BuildNetwork()
    {
        if (!HaveInterfaces()) return;

        Add(
            new MetricDef("sys.net.down", "网络下行", "bytes", "B/s", 1000, true, null, null),
            () => NetRate().Down);

        Add(
            new MetricDef("sys.net.up", "网络上行", "bytes", "B/s", 1000, true, null, null),
            () => NetRate().Up);
    }

    private void BuildSystem()
    {
        Add(
            new MetricDef("sys.uptime", "运行时长", "duration", "s", 1000, false, null, null),
            () => Environment.TickCount64 / 1000.0);

        Add(
            new MetricDef("sys.status", "系统状态", "state", null, 2000, false, null, null),
            () =>
            {
                var hw = _computer.Hardware.Count(h => h.HardwareType != HardwareType.Motherboard);
                var sensorHint = _sensorsAvailable ? "传感器正常" : "传感器受限";
                return $"在线 · {hw} 个硬件 · {(_isAdmin ? "管理员" : "受限")} · {sensorHint}";
            });
    }

    private static bool HaveInterfaces() =>
        NetworkInterface.GetAllNetworkInterfaces().Any(IsActive);

    private static bool IsActive(NetworkInterface n) =>
        n.OperationalStatus == OperationalStatus.Up
        && n.NetworkInterfaceType != NetworkInterfaceType.Loopback
        && n.NetworkInterfaceType != NetworkInterfaceType.Tunnel;

    private (double Down, double Up) NetRate()
    {
        var now = Environment.TickCount64;
        if (now - _netAt < 100) return _netCache;

        long rx = 0;
        long tx = 0;
        foreach (var n in NetworkInterface.GetAllNetworkInterfaces().Where(IsActive))
        {
            var s = n.GetIPStatistics();
            rx += s.BytesReceived;
            tx += s.BytesSent;
        }

        if (_netAt > 0 && now > _netAt)
        {
            var dt = (now - _netAt) / 1000.0;
            _netCache = (Math.Max(0, (rx - _netRx) / dt), Math.Max(0, (tx - _netTx) / dt));
        }
        _netAt = now;
        _netRx = rx;
        _netTx = tx;
        return _netCache;
    }

    public Dictionary<string, object> Poll(List<string> ids)
    {
        var result = new Dictionary<string, object>();
        if (_collectorHardwareUpdate(ids)) { /* values refreshed */ }

        foreach (var id in ids)
        {
            if (!_getters.TryGetValue(id, out var getter))
            {
                result[id] = new ErrorValue("未知指标");
                continue;
            }
            try
            {
                result[id] = getter();
            }
            catch (Exception e)
            {
                result[id] = new ErrorValue(e.Message);
            }
        }
        return result;
    }

    private bool _collectorHardwareUpdate(List<string> ids)
    {
        var seen = new HashSet<IHardware>();
        foreach (var id in ids)
        {
            if (_hardwareOf.TryGetValue(id, out var hw) && seen.Add(hw))
            {
                hw.Update();
            }
        }
        return seen.Count > 0;
    }

    private static string Bytes(long n)
    {
        string[] units = { "B", "KB", "MB", "GB", "TB" };
        double v = Math.Abs(n);
        var i = 0;
        while (v >= 1024 && i < units.Length - 1)
        {
            v /= 1024;
            i++;
        }
        return $"{v:0.#} {units[i]}";
    }

    public void Dispose()
    {
        try
        {
            _computer.Close();
        }
        catch
        {
            // 退出路径，忽略
        }
    }

    public static void Dump()
    {
        var computer = new Computer
        {
            IsCpuEnabled = true,
            IsGpuEnabled = true,
            IsMemoryEnabled = true,
            IsStorageEnabled = true,
            IsNetworkEnabled = true,
        };
        computer.Open();
        foreach (var hw in computer.Hardware)
        {
            hw.Update();
            Console.WriteLine($"[{hw.HardwareType}] {hw.Name}");
            foreach (var sub in hw.SubHardware)
            {
                sub.Update();
                foreach (var s in sub.Sensors.Where(s => s.Value.HasValue))
                    Console.WriteLine($"    ({sub.Name}) {s.SensorType}: {s.Name} = {s.Value:0.##}");
            }
            foreach (var s in hw.Sensors.Where(s => s.Value.HasValue))
            {
                Console.WriteLine($"    {s.SensorType}: {s.Name} = {s.Value:0.##}");
            }
        }
        computer.Close();
    }
}

[StructLayout(LayoutKind.Sequential)]
internal struct MEMORYSTATUSEX
{
    public uint dwLength;
    public uint dwMemoryLoad;
    public ulong ullTotalPhys;
    public ulong ullAvailPhys;
    public ulong ullTotalPageFile;
    public ulong ullAvailPageFile;
    public ulong ullTotalVirtual;
    public ulong ullAvailVirtual;
    public ulong ullAvailExtendedVirtual;
}

[StructLayout(LayoutKind.Sequential)]
internal struct PERFORMANCE_INFORMATION
{
    public uint cb;
    public IntPtr CommitTotal;
    public IntPtr CommitLimit;
    public IntPtr CommitPeak;
    public IntPtr PhysicalTotal;
    public IntPtr PhysicalAvailable;
    public IntPtr SystemCache;
    public IntPtr KernelTotal;
    public IntPtr KernelPaged;
    public IntPtr KernelNonpaged;
    public IntPtr PageSize;
    public uint HandleCount;
    public uint ProcessCount;
    public uint ThreadCount;
}

internal static class Native
{
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GlobalMemoryStatusEx(ref MEMORYSTATUSEX buffer);

    [DllImport("psapi.dll", SetLastError = true)]
    public static extern bool GetPerformanceInfo(out PERFORMANCE_INFORMATION info, uint size);
}
