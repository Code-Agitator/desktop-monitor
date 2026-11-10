// 罗技 HID++ 2.0 电量：2.4G 接收器上的无线鼠标不会通过标准 HID 电量上报，
// 只能走罗技私有协议。协议参考 Ithilias/logitray（MIT）与 pwr-Solaar/Solaar：
//   帧 = [0x11, device_index, feature_index, (function << 4) | swid, p0, p1, p2, ...]
//   ping      : feature 0x00 / function 0x01
//   getFeature: feature 0x00 / function 0x00, params = [featureIdHi, featureIdLo, 0]
//   电量      : feature 0x1000 fn0 / 0x1004 fn1（百分比），0x1001 fn0（电压 mV）
//
// 这里用 overlapped I/O + 超时，避免 HID 写入/读取把插件线程卡死。

using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace DeviceBattery;

internal static class LogitechHidpp
{
    public sealed record Reading(string Name, int Level, bool Charging);

    private const byte LongId = 0x11;
    private const byte Swid = 0x0A;
    private const uint GenericRead = 0x80000000;
    private const uint GenericWrite = 0x40000000;
    private const uint FileShareReadWrite = 0x3;
    private const uint OpenExisting = 3;
    private const uint FileFlagOverlapped = 0x40000000;
    private const int ErrorIoPending = 997;
    private const uint WaitObject0 = 0;

    /// <summary>枚举罗技接收器并读一次电量；读不到就返回空列表，不抛异常。</summary>
    public static List<Reading> Read(int budgetMs = 3000, bool verbose = false)
    {
        var result = new List<Reading>();
        var deadline = Environment.TickCount64 + budgetMs;
        try
        {
            foreach (var path in EnumHidPaths())
            {
                if (Environment.TickCount64 > deadline) break;
                var lower = path.ToLowerInvariant();
                if (!lower.Contains("vid_046d")) continue;

                var handle = Open(path);
                if (handle is null) { if (verbose) Console.WriteLine($"   {Short(path)} 打不开"); continue; }
                using (handle)
                {
                    if (!Caps(handle, out var outLen, out var inLen) || outLen != 20)
                    {
                        if (verbose) Console.WriteLine($"   {Short(path)} 跳过 in={inLen} out={outLen}");
                        continue;
                    }
                    if (verbose) Console.WriteLine($"   {Short(path)} in={inLen} out={outLen}");
                    foreach (byte dev in new byte[] { 1, 2, 3, 4, 5, 6 })
                    {
                        if (Environment.TickCount64 > deadline) break;
                        var ping = Ask(handle, dev, 0x00, 0x01, new byte[] { 0x00, 0x00, 0x5A }, inLen);
                        if (ping is null) { if (verbose) Console.WriteLine($"      dev={dev} 无回应"); continue; }
                        if (verbose) Console.WriteLine($"      dev={dev} 在线 HID++ {ping[4]}.{ping[5]}");
                        var reading = ReadBattery(handle, dev, inLen, verbose);
                        if (reading is not null) result.Add(reading);
                        break; // 一个接收器通常只带一个设备；找到就够
                    }
                }
            }
        }
        catch
        {
            // 插件不该因为外设异常挂掉
        }
        return result;
    }

    /// <summary>枚举设备特征表（调试用）</summary>
    public static List<string> Features(int budgetMs = 12000)
    {
        var lines = new List<string>();
        try
        {
            foreach (var path in EnumHidPaths())
            {
                var lower = path.ToLowerInvariant();
                if (!lower.Contains("vid_046d")) continue;
                var handle = Open(path);
                if (handle is null) continue;
                using (handle)
                {
                    if (!Caps(handle, out var outLen, out var inLen) || outLen != 20) continue;
                    foreach (byte dev in new byte[] { 1, 2 })
                    {
                        var ping = Ask(handle, dev, 0x00, 0x01, new byte[] { 0, 0, 0x5A }, inLen);
                        if (ping is null) continue;
                        lines.Add($"dev={dev} HID++ {ping[4]}.{ping[5]}");
                        var fs = Ask(handle, dev, 0x00, 0x00, new byte[] { 0x00, 0x01, 0x00 }, inLen);
                        if (fs is null || fs[4] == 0) { lines.Add("  无 IFeatureSet"); continue; }
                        var fsIdx = fs[4];
                        var cnt = Ask(handle, dev, fsIdx, 0x00, new byte[] { 0, 0, 0 }, inLen);
                        var n = cnt?[4] ?? 0;
                        lines.Add($"  特征数 {n}");
                        for (byte i = 0; i < n; i++)
                        {
                            var f = Ask(handle, dev, fsIdx, 0x01, new byte[] { i, 0, 0 }, inLen);
                            if (f is null) continue;
                            lines.Add($"    0x{(f[4] << 8 | f[5]):X4}");
                        }
                        break;
                    }
                }
            }
        }
        catch (Exception e)
        {
            lines.Add("异常 " + e.Message);
        }
        return lines;
    }

    private static Reading? ReadBattery(SafeFileHandle handle, byte dev, ushort inLen, bool verbose)
    {
        // 0x1000 Battery Unified Level：fn0 = batteryLevelStatus
        if (TryFeature(handle, dev, 0x1000, inLen, out var idx))
        {
            var r = Ask(handle, dev, idx, 0x00, new byte[] { 0, 0, 0 }, inLen);
            if (verbose) Console.WriteLine($"      0x1000 idx=0x{idx:X2} -> {(r is null ? "无回包" : Hex(r))}");
            if (r is not null && r[4] is > 0 and <= 100)
                return new Reading($"罗技鼠标", r[4], IsCharging(r[5]));
        }
        else if (verbose) Console.WriteLine("      0x1000 不支持");

        // 0x1004 Unified Battery：fn1 = getStatus
        if (TryFeature(handle, dev, 0x1004, inLen, out idx))
        {
            var r = Ask(handle, dev, idx, 0x01, new byte[] { 0, 0, 0 }, inLen);
            if (verbose) Console.WriteLine($"      0x1004 idx=0x{idx:X2} -> {(r is null ? "无回包" : Hex(r))}");
            if (r is not null && r[4] is > 0 and <= 100)
                return new Reading($"罗技鼠标", r[4], IsCharging(r[5]));
        }
        else if (verbose) Console.WriteLine("      0x1004 不支持");

        // 0x1001 Battery Voltage：fn0 返回 mV，用曲线换算百分比
        if (TryFeature(handle, dev, 0x1001, inLen, out idx))
        {
            var r = Ask(handle, dev, idx, 0x00, new byte[] { 0, 0, 0 }, inLen);
            if (verbose) Console.WriteLine($"      0x1001 idx=0x{idx:X2} -> {(r is null ? "无回包" : Hex(r))}");
            if (r is not null)
            {
                var mv = (r[4] << 8) | r[5];
                if (mv is > 2000 and < 5000)
                    return new Reading($"罗技鼠标", MvToPercent(mv), IsChargingFlags(r[6]));
            }
        }
        else if (verbose) Console.WriteLine("      0x1001 不支持");
        return null;
    }

    private static string Hex(byte[] b) => string.Join(" ", b.Take(10).Select(x => x.ToString("X2")));

    private static string Short(string path)
    {
        var i = path.IndexOf("hid#", StringComparison.OrdinalIgnoreCase);
        return i < 0 ? path : path[i..Math.Min(path.Length, i + 46)];
    }

    private static bool TryFeature(SafeFileHandle handle, byte dev, ushort featureId, ushort inLen, out byte idx)
    {
        idx = 0;
        var gf = Ask(handle, dev, 0x00, 0x00, new byte[] { (byte)(featureId >> 8), (byte)featureId, 0 }, inLen);
        if (gf is null || gf[4] == 0) return false;
        idx = gf[4];
        return true;
    }

    private static bool IsCharging(byte status) => status is 1 or 2 or 4;

    private static bool IsChargingFlags(byte flags) => (flags & 0x80) != 0 && (flags & 0x07) != 2;

    /// <summary>
    /// 电压→百分比。曲线取自 Solaar（hidpp20.estimate_battery_level_percentage），
    /// 用线性插值；G HUB 在 G903 上比这条通用曲线高约 4 个百分点
    /// （实测 3912 mV：G HUB 72%，通用曲线 68%），所以先按 +23 mV 校准再查表。
    /// </summary>
    private const int VoltageOffsetMv = 23;

    private static readonly (int Mv, int Pct)[] VoltageCurve =
    {
        (4186, 100), (4067, 90), (3989, 80), (3922, 70), (3859, 60),
        (3811, 50), (3778, 40), (3751, 30), (3717, 20), (3671, 10),
        (3646, 5), (3579, 2), (3500, 0),
    };

    private static int _lastMv;

    private static int MvToPercent(int mv)
    {
        // 电压会随负载抖动，做一次指数平滑（首次直接采用）
        var smoothed = _lastMv == 0 ? mv : (int)Math.Round(_lastMv * 0.55 + mv * 0.45);
        _lastMv = smoothed;
        var v = smoothed + VoltageOffsetMv;

        if (v >= VoltageCurve[0].Mv) return 100;
        var last = VoltageCurve[^1];
        if (v <= last.Mv) return 0;
        for (var i = 0; i < VoltageCurve.Length - 1; i++)
        {
            var (hiMv, hiPct) = VoltageCurve[i];
            var (loMv, loPct) = VoltageCurve[i + 1];
            if (v <= hiMv && v >= loMv)
                return (int)Math.Round(loPct + (hiPct - loPct) * (double)(v - loMv) / (hiMv - loMv));
        }
        return 0;
    }

    // ---------- HID++ 收发 ----------

    private static byte[]? Ask(SafeFileHandle handle, byte dev, byte feature, byte function, byte[] prms, ushort inLen)
    {
        var outLen = 20;
        var req = new byte[outLen];
        req[0] = LongId;
        req[1] = dev;
        req[2] = feature;
        req[3] = (byte)((function << 4) | Swid);
        Array.Copy(prms, 0, req, 4, Math.Min(prms.Length, 3));

        if (!OverlappedWrite(handle, req, 250)) return null;

        var readLen = Math.Max((int)inLen, 20);
        var deadline = Environment.TickCount64 + 260;
        while (Environment.TickCount64 < deadline)
        {
            var msg = OverlappedRead(handle, readLen, 150);
            if (msg is null) return null;
            if (msg.Length < 7) continue;
            if (msg[1] != dev) continue;
            if (msg[2] == 0xFF || msg[2] == 0x8F) return null;   // 错误帧
            if (msg[2] != feature) continue;
            if ((msg[3] & 0x0F) != Swid) continue;
            return msg;
        }
        return null;
    }

    private static bool OverlappedWrite(SafeFileHandle handle, byte[] buffer, int timeoutMs)
    {
        var ev = CreateEventW(IntPtr.Zero, true, false, null);
        if (ev == IntPtr.Zero) return false;
        try
        {
            var ov = new NativeOverlapped { EventHandle = ev };
            if (WriteFile(handle, buffer, (uint)buffer.Length, out _, ref ov))
                return true;
            if ((uint)Marshal.GetLastWin32Error() != ErrorIoPending) return false;
            if (WaitForSingleObject(ev, timeoutMs) != WaitObject0)
            {
                CancelIoEx(handle, ref ov);
                return false;
            }
            return GetOverlappedResult(handle, ref ov, out _, false);
        }
        finally
        {
            CloseHandle(ev);
        }
    }

    private static byte[]? OverlappedRead(SafeFileHandle handle, int len, int timeoutMs)
    {
        var buffer = new byte[len];
        var ev = CreateEventW(IntPtr.Zero, true, false, null);
        if (ev == IntPtr.Zero) return null;
        try
        {
            var ov = new NativeOverlapped { EventHandle = ev };
            if (!ReadFile(handle, buffer, (uint)len, out var got, ref ov))
            {
                if ((uint)Marshal.GetLastWin32Error() != ErrorIoPending) return null;
                if (WaitForSingleObject(ev, timeoutMs) != WaitObject0)
                {
                    CancelIoEx(handle, ref ov);
                    return null;
                }
                if (!GetOverlappedResult(handle, ref ov, out got, false)) return null;
            }
            if (got == 0) return null;
            var n = (int)got;
            var msg = n >= 8 && buffer[0] != LongId && buffer[0] != 0x10
                ? buffer.Skip(1).Take(n - 1).ToArray()
                : buffer.Take(n).ToArray();
            return msg;
        }
        finally
        {
            CloseHandle(ev);
        }
    }

    // ---------- 设备枚举 ----------

    private static SafeFileHandle? Open(string path)
    {
        foreach (var access in new[] { GenericRead | GenericWrite, GenericRead })
        {
            var h = CreateFileW(path, access, FileShareReadWrite, IntPtr.Zero, OpenExisting, FileFlagOverlapped, IntPtr.Zero);
            if (!h.IsInvalid) return h;
            h.Dispose();
        }
        return null;
    }

    private static bool Caps(SafeFileHandle handle, out ushort outLen, out ushort inLen)
    {
        outLen = inLen = 0;
        if (!HidD_GetPreparsedData(handle, out var pre)) return false;
        try
        {
            var buf = Marshal.AllocHGlobal(64);
            try
            {
                for (var i = 0; i < 64; i++) Marshal.WriteByte(buf, i, 0);
                if (HidP_GetCaps(pre, buf) != 0x00110000) return false;
                inLen = (ushort)Marshal.ReadInt16(buf, 4);
                outLen = (ushort)Marshal.ReadInt16(buf, 6);
                return true;
            }
            finally { Marshal.FreeHGlobal(buf); }
        }
        finally { HidD_FreePreparsedData(pre); }
    }

    private static IEnumerable<string> EnumHidPaths()
    {
        var g = new Guid("4D1E55B2-F16F-11CF-88CB-001111000030");
        var set = SetupDiGetClassDevsW(ref g, null, IntPtr.Zero, 0x12);
        if (set == new IntPtr(-1)) yield break;
        try
        {
            for (uint i = 0; ; i++)
            {
                var ifd = new SpDeviceInterfaceData { Size = Marshal.SizeOf<SpDeviceInterfaceData>() };
                if (!SetupDiEnumDeviceInterfaces(set, IntPtr.Zero, ref g, i, ref ifd)) break;
                SetupDiGetDeviceInterfaceDetailW(set, ref ifd, IntPtr.Zero, 0, out var size, IntPtr.Zero);
                if (size == 0) continue;
                var buf = Marshal.AllocHGlobal((int)size);
                try
                {
                    Marshal.WriteInt32(buf, IntPtr.Size == 8 ? 8 : 6);
                    if (!SetupDiGetDeviceInterfaceDetailW(set, ref ifd, buf, size, out _, IntPtr.Zero)) continue;
                    var p = Marshal.PtrToStringUni(buf + 4);
                    if (!string.IsNullOrEmpty(p)) yield return p;
                }
                finally { Marshal.FreeHGlobal(buf); }
            }
        }
        finally { SetupDiDestroyDeviceInfoList(set); }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SpDeviceInterfaceData
    {
        public int Size;
        public Guid InterfaceClassGuid;
        public int Flags;
        public IntPtr Reserved;
    }

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr SetupDiGetClassDevsW(ref Guid g, string? e, IntPtr p, uint f);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiEnumDeviceInterfaces(IntPtr s, IntPtr d, ref Guid g, uint i, ref SpDeviceInterfaceData data);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetupDiGetDeviceInterfaceDetailW(IntPtr s, ref SpDeviceInterfaceData d, IntPtr det, uint size, out uint req, IntPtr info);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiDestroyDeviceInfoList(IntPtr s);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFileW(string n, uint a, uint s, IntPtr sa, uint d, uint f, IntPtr t);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool ReadFile(SafeFileHandle h, byte[] buf, uint len, out uint read, ref NativeOverlapped ov);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool WriteFile(SafeFileHandle h, byte[] buf, uint len, out uint written, ref NativeOverlapped ov);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetOverlappedResult(SafeFileHandle h, ref NativeOverlapped ov, out uint transferred, bool wait);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CancelIoEx(SafeFileHandle h, ref NativeOverlapped ov);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr CreateEventW(IntPtr attr, bool manualReset, bool initialState, string? name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr handle, int ms);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_GetPreparsedData(SafeFileHandle h, out IntPtr pre);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_FreePreparsedData(IntPtr pre);

    [DllImport("hid.dll")]
    private static extern int HidP_GetCaps(IntPtr pre, IntPtr caps);
}
