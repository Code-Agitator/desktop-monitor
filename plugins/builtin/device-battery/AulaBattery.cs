// 狼蛛（AULA）F87 PRO 键盘电量：厂商私有 20 字节报文，走 HID UsagePage 0xFF02。
// 协议来自桌面上的逆向项目（f87ctl / docs/protocol.md）：
//   帧 = [0x13][cmd][总包数][包序号][(方向<<4)|本包长度][14B 数据][前 19 字节和]
//   读电源 = cmd 0x4A，返回 2 字节：byte0 电量%，byte1 的 bit4 = 充电中
// 设备：无线 3554:FA09（接收器）/ 有线 258A:010C；设备约 1.1s 后才把包吐出来，所以要多轮读。

using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace DeviceBattery;

internal static class AulaBattery
{
    public sealed record Reading(string Name, int Level, bool Charging);

    private const byte ReportId = 0x13;
    private const int FrameLen = 20;
    private const int Chunk = 14;
    private const ushort UsagePage = 0xFF02;

    private const uint GenericRead = 0x80000000;
    private const uint GenericWrite = 0x40000000;
    private const uint FileShareReadWrite = 0x3;
    private const uint OpenExisting = 3;
    private const uint FileFlagOverlapped = 0x40000000;
    private const uint ErrorIoPending = 997;
    private const uint WaitObject0 = 0;

    /// <summary>读一次键盘电量；没插接收器/键盘休眠时返回 null，不抛异常。</summary>
    public static Reading? Read(int attempts = 4, int windowMs = 1100, bool verbose = false)
    {
        try
        {
            foreach (var path in EnumHidPaths())
            {
                var lower = path.ToLowerInvariant();
                var isWireless = lower.Contains("vid_3554&pid_fa09");
                var isWired = lower.Contains("vid_258a&pid_010c");
                if (!isWireless && !isWired) continue;

                var handle = Open(path);
                if (handle is null) continue;
                using (handle)
                {
                    if (!Caps(handle, out var usagePage, out var inLen, out var outLen)) continue;
                    if (usagePage != UsagePage) continue;

                    var probeLen = Math.Max((int)inLen, FrameLen);
                    if (verbose) Console.WriteLine($"   打开 {Short(path)} usagePage=0x{usagePage:X4} in={inLen}");
                    // 先 0x07 唤醒（无线空闲会休眠，且第一包常被“吃掉”）
                    for (var wake = 0; wake < 3; wake++)
                    {
                        if (ReadCmd(handle, 0x07, 1, 500, probeLen, false) is not null) break;
                    }
                    var data = ReadCmd(handle, 0x4A, attempts, windowMs, probeLen, verbose);
                    if (data is null || data.Count < 2) continue;

                    var level = Math.Min(100, (int)data[0]);
                    var charging = (data[1] & 0x10) != 0;
                    if (level is <= 0 or > 100) continue;
                    return new Reading("狼蛛键盘", level, charging);
                }
            }
        }
        catch
        {
            // 外设异常不该影响插件
        }
        return null;
    }

    /// <summary>调试：对指定命令做一次收发</summary>
    public static List<byte>? Probe(byte cmd, int attempts = 2, int windowMs = 1500)
    {
        foreach (var path in EnumHidPaths())
        {
            var lower = path.ToLowerInvariant();
            if (!lower.Contains("vid_3554&pid_fa09")) continue;
            var handle = Open(path);
            if (handle is null) continue;
            using (handle)
            {
                if (!Caps(handle, out var up, out var inLen, out _)) continue;
                if (up != UsagePage) continue;
                Console.WriteLine($"   {Short(path)} usagePage=0x{up:X4} in={inLen}");
                return ReadCmd(handle, cmd, attempts, windowMs, Math.Max((int)inLen, FrameLen), true);
            }
        }
        return null;
    }

    /// <summary>调试：列出所有 HID 接口的 UsagePage/报告长度</summary>
    public static List<string> Dump()
    {
        var lines = new List<string>();
        foreach (var path in EnumHidPaths())
        {
            var handle = Open(path);
            if (handle is null) { lines.Add($"{Short(path)}  打不开"); continue; }
            using (handle)
            {
                if (Caps(handle, out var up, out var inLen, out var outLen))
                    lines.Add($"{Short(path)}  usagePage=0x{up:X4} in={inLen} out={outLen}");
                else
                    lines.Add($"{Short(path)}  caps 失败");
            }
        }
        return lines;
    }

    private static string Short(string path)
    {
        var i = path.IndexOf("hid#", StringComparison.OrdinalIgnoreCase);
        return i < 0 ? path : path[i..Math.Min(path.Length, i + 58)];
    }

    /// <summary>发一条读命令并收齐分包；返回拼接后的数据。</summary>
    private static List<byte>? ReadCmd(SafeFileHandle handle, byte cmd, int attempts, int windowMs, int inLen, bool verbose)
    {
        var parts = new SortedDictionary<int, byte[]>();
        int? total = null;

        for (var attempt = 0; attempt < attempts; attempt++)
        {
            Drain(handle, inLen);
            if (attempt > 0) Thread.Sleep(40);

            var req = Frame(cmd, 1, 0, 0, Array.Empty<byte>());
            if (verbose) Console.WriteLine($"   第 {attempt + 1} 轮写入 {BitConverter.ToString(req)}");
            if (!Write(handle, req)) { if (verbose) Console.WriteLine("   写入失败"); return null; }

            var deadline = Environment.TickCount64 + windowMs;
            while (Environment.TickCount64 < deadline)
            {
                var raw = Read(handle, inLen, 50);
                if (raw is null) continue;
                if (verbose) Console.WriteLine($"   收到 {BitConverter.ToString(raw)}");
                if (raw.Length < FrameLen) continue;
                if (!ChecksumOk(raw)) continue;
                if (raw[1] != cmd) continue;

                total = raw[2];
                var len = raw[4] & 0x0F;
                var take = Math.Min(len, Chunk);
                parts[raw[3]] = raw.Skip(5).Take(take).ToArray();
                if (total is > 0 && parts.Count >= total.Value) break;
            }

            if (total is > 0 && parts.Count >= total.Value) break;
        }

        if (total is not > 0 || parts.Count < total.Value) return null;
        var outBytes = new List<byte>();
        for (var i = 0; i < total.Value; i++)
        {
            if (parts.TryGetValue(i, out var seg)) outBytes.AddRange(seg);
        }
        return outBytes;
    }

    private static byte[] Frame(byte cmd, byte total, byte index, byte dir, byte[] chunk)
    {
        var f = new byte[FrameLen];
        f[0] = ReportId;
        f[1] = cmd;
        f[2] = total;
        f[3] = index;
        var n = Math.Min(chunk.Length, Chunk);
        f[4] = (byte)(((dir & 0x0F) << 4) | (n & 0x0F));
        Array.Copy(chunk, 0, f, 5, n);
        byte sum = 0;
        for (var i = 0; i < FrameLen - 1; i++) sum = (byte)(sum + f[i]);
        f[FrameLen - 1] = sum;
        return f;
    }

    private static bool ChecksumOk(byte[] raw)
    {
        byte sum = 0;
        for (var i = 0; i < FrameLen - 1; i++) sum = (byte)(sum + raw[i]);
        return sum == raw[FrameLen - 1];
    }

    private static void Drain(SafeFileHandle handle, int inLen)
    {
        for (var i = 0; i < 8; i++)
        {
            if (Read(handle, inLen, 5) is null) return;
        }
    }

    // ---------- overlapped 收发（带超时，不会把插件线程卡死） ----------

    private static bool Write(SafeFileHandle handle, byte[] buffer)
    {
        var ev = CreateEventW(IntPtr.Zero, true, false, null);
        if (ev == IntPtr.Zero) return false;
        try
        {
            var ov = new NativeOverlapped { EventHandle = ev };
            if (WriteFile(handle, buffer, (uint)buffer.Length, out _, ref ov)) return true;
            if ((uint)Marshal.GetLastWin32Error() != ErrorIoPending) return false;
            if (WaitForSingleObject(ev, 300) != WaitObject0)
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

    private static byte[]? Read(SafeFileHandle handle, int len, int timeoutMs)
    {
        var buffer = new byte[Math.Max(len, FrameLen)];
        var ev = CreateEventW(IntPtr.Zero, true, false, null);
        if (ev == IntPtr.Zero) return null;
        try
        {
            var ov = new NativeOverlapped { EventHandle = ev };
            if (!ReadFile(handle, buffer, (uint)buffer.Length, out var got, ref ov))
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
            return buffer.Take((int)got).ToArray();
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

    private static bool Caps(SafeFileHandle handle, out ushort usagePage, out ushort inLen, out ushort outLen)
    {
        usagePage = inLen = outLen = 0;
        if (!HidD_GetPreparsedData(handle, out var pre)) return false;
        try
        {
            var buf = Marshal.AllocHGlobal(64);
            try
            {
                for (var i = 0; i < 64; i++) Marshal.WriteByte(buf, i, 0);
                if (HidP_GetCaps(pre, buf) != 0x00110000) return false;
                usagePage = (ushort)Marshal.ReadInt16(buf, 2);
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
