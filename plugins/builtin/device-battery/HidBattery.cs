// 通用 HID 电量读取：对每个 HID 设备的 Feature 报表逐个询问 HID 解析器，
// 看报表里是否含 Battery Strength（Usage Page 0x06 / Usage 0x20），有则取数值。
// 这是无线键鼠在 HID 层常见的标准电量上报方式（不依赖厂商私有协议）。

using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace DeviceBattery;

internal static class HidBattery
{
    public static List<(string Name, int Level)> Probe(bool verbose = false)
    {
        var result = new List<(string, int)>();
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var guid = new Guid("4D1E55B2-F16F-11CF-88CB-001111000030"); // GUID_DEVINTERFACE_HID
        var set = SetupDiGetClassDevsW(ref guid, null, IntPtr.Zero, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        if (set == new IntPtr(-1)) return result;
        _verbose = verbose;
        if (verbose) Console.WriteLine("   HID 设备枚举开始");

        try
        {
            for (uint i = 0; ; i++)
            {
                var ifd = new SP_DEVICE_INTERFACE_DATA { cbSize = Marshal.SizeOf<SP_DEVICE_INTERFACE_DATA>() };
                if (!SetupDiEnumDeviceInterfaces(set, IntPtr.Zero, ref guid, i, ref ifd))
                    break;

                var path = GetDevicePath(set, ref ifd);
                if (path is null) continue;

                try
                {
                    if (!TryRead(path, out var name, out var level, out var diag))
                    {
                        if (verbose) Console.WriteLine($"   {name,-36} 未读数 [{diag}]");
                        continue;
                    }
                    if (verbose) Console.WriteLine($"   {name,-36} level={level}% [{diag}]");
                    if (level is > 0 and <= 100 && seen.Add(name))
                        result.Add((name, level));
                }
                catch (Exception e)
                {
                    if (verbose) Console.WriteLine($"   {Path.GetFileName(path),-36} 异常 {e.GetType().Name}: {e.Message}");
                }
            }
        }
        finally
        {
            SetupDiDestroyDeviceInfoList(set);
        }
        return result;
    }

    private static bool _verbose;

    private static bool TryRead(string path, out string name, out int level, out string diag)
    {
        name = "";
        level = 0;
        diag = "";
        var handle = Open(path);
        if (handle is null) { diag = "打开失败 " + Marshal.GetLastWin32Error(); return false; }

        using (handle)
        {
            var sb = new StringBuilder(256);
            if (HidD_GetProductString(handle, sb, sb.Capacity))
                name = sb.ToString().Trim();
            if (string.IsNullOrWhiteSpace(name))
            {
                var msb = new StringBuilder(256);
                if (HidD_GetManufacturerString(handle, msb, msb.Capacity))
                    name = msb.ToString().Trim();
            }
            if (string.IsNullOrWhiteSpace(name)) name = Path.GetFileName(path);

            if (!HidD_GetPreparsedData(handle, out var preparsed)) { diag = "Preparsed 失败"; return false; }
            try
            {
                var capsBuf = Marshal.AllocHGlobal(64);
                int capsStatus;
                ushort featureLen;
                ushort featureValueCaps;
                try
                {
                    for (var z = 0; z < 64; z++) Marshal.WriteByte(capsBuf, z, 0);
                    capsStatus = HidP_GetCaps(preparsed, capsBuf);
                    featureLen = (ushort)Marshal.ReadInt16(capsBuf, 8);
                    featureValueCaps = (ushort)Marshal.ReadInt16(capsBuf, 60);
                }
                finally
                {
                    Marshal.FreeHGlobal(capsBuf);
                }
                if (capsStatus != HIDP_STATUS_SUCCESS)
                {
                    diag = $"GetCaps 失败 status=0x{capsStatus:X8}";
                    return false;
                }

                if (_verbose && featureValueCaps > 0) DumpValueCaps(preparsed, featureValueCaps);

                var len = Math.Max(3, (int)featureLen);
                var buffer = new byte[len];
                var readable = 0;
                for (var rid = 0; rid <= 15; rid++)
                {
                    buffer[0] = (byte)rid;
                    if (!HidD_GetFeature(handle, buffer, buffer.Length)) continue;
                    readable++;
                    if (TryExtract(buffer, preparsed, out level))
                    {
                        diag = $"reportId={rid}";
                        return true;
                    }
                }
                diag = $"无电量 Feature（可读 Feature 报表 {readable} 个，Feature ValueCap {featureValueCaps} 个）";
                return false;
            }
            finally
            {
                HidD_FreePreparsedData(preparsed);
            }
        }
    }

    private static void DumpValueCaps(IntPtr preparsed, int expect)
    {
        var buf = Marshal.AllocHGlobal(4096);
        try
        {
            var len = (ushort)Math.Min(64, Math.Max(expect, 1));
            var status = HidP_GetValueCaps(HIDP_FEATURE, buf, ref len, preparsed);
            Console.WriteLine($"      ValueCaps status=0x{status:X8} 返回 {len} 个");
            if (status != HIDP_STATUS_SUCCESS) return;
            var p = buf;
            for (var i = 0; i < len; i++)
            {
                var page = (ushort)Marshal.ReadInt16(p, 0);
                var rid = Marshal.ReadByte(p, 2);
                var isRange = Marshal.ReadByte(p, 12) != 0;
                var bits = (ushort)Marshal.ReadInt16(p, 18);
                var reportCount = (ushort)Marshal.ReadInt16(p, 20);
                var usage = (ushort)Marshal.ReadInt16(p, 56);
                var usageMax = (ushort)Marshal.ReadInt16(p, 58);
                Console.WriteLine($"      cap[{i}] page=0x{page:X2} usage=0x{usage:X2}..0x{usageMax:X2} rid={rid} range={isRange} bits={bits} n={reportCount}");
                var n = isRange ? Math.Max(1, usageMax - usage + 1) : 1;
                p += (72 + 2 * n + 3) & ~3;
            }
        }
        finally
        {
            Marshal.FreeHGlobal(buf);
        }
    }

    /// <summary>报表缓冲带 report ID 时读取；再退一步按「无 ID」布局读取。</summary>
    private static bool TryExtract(byte[] buf, IntPtr preparsed, out int level)
    {
        if (HidP_GetUsageValue(HIDP_FEATURE, 0x06, 0, 0x20, out var v, preparsed, buf, (uint)buf.Length) == HIDP_STATUS_SUCCESS
            && v <= 100)
        {
            level = (int)v;
            return true;
        }
        if (buf.Length > 1)
        {
            var body = new byte[buf.Length - 1];
            Array.Copy(buf, 1, body, 0, body.Length);
            if (HidP_GetUsageValue(HIDP_FEATURE, 0x06, 0, 0x20, out v, preparsed, body, (uint)body.Length) == HIDP_STATUS_SUCCESS
                && v <= 100)
            {
                level = (int)v;
                return true;
            }
        }
        level = 0;
        return false;
    }

    private static SafeFileHandle? Open(string path)
    {
        // 0 访问权限通常足够读取 Feature；读写权限对个别设备会失败
        foreach (var access in new[] { 0u, GENERIC_READ, GENERIC_READ | GENERIC_WRITE })
        {
            var h = CreateFileW(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE, IntPtr.Zero,
                OPEN_EXISTING, 0, IntPtr.Zero);
            if (!h.IsInvalid) return h;
            h.Dispose();
        }
        return null;
    }

    private static string? GetDevicePath(IntPtr set, ref SP_DEVICE_INTERFACE_DATA ifd)
    {
        SetupDiGetDeviceInterfaceDetailW(set, ref ifd, IntPtr.Zero, 0, out var size, IntPtr.Zero);
        if (size == 0) return null;
        var buf = Marshal.AllocHGlobal((int)size);
        try
        {
            // x64 上 cbSize 必须为 8（4 字节 cbSize + 4 字节对齐），x86 为 6
            Marshal.WriteInt32(buf, IntPtr.Size == 8 ? 8 : 6);
            if (!SetupDiGetDeviceInterfaceDetailW(set, ref ifd, buf, size, out _, IntPtr.Zero))
                return null;
            return Marshal.PtrToStringUni(buf + 4);
        }
        finally
        {
            Marshal.FreeHGlobal(buf);
        }
    }

    // ---------- P/Invoke ----------

    private const uint DIGCF_PRESENT = 0x2;
    private const uint DIGCF_DEVICEINTERFACE = 0x10;
    private const uint GENERIC_READ = 0x80000000;
    private const uint GENERIC_WRITE = 0x40000000;
    private const uint FILE_SHARE_READ = 0x1;
    private const uint FILE_SHARE_WRITE = 0x2;
    private const uint OPEN_EXISTING = 3;
    private const int HIDP_FEATURE = 2;
    private const int HIDP_STATUS_SUCCESS = 0x00110000;

    [StructLayout(LayoutKind.Sequential)]
    private struct SP_DEVICE_INTERFACE_DATA
    {
        public int cbSize;
        public Guid InterfaceClassGuid;
        public int Flags;
        public IntPtr Reserved;
    }

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr SetupDiGetClassDevsW(ref Guid classGuid, string? enumerator, IntPtr hwndParent, uint flags);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiEnumDeviceInterfaces(IntPtr set, IntPtr devInfo, ref Guid classGuid, uint memberIndex, ref SP_DEVICE_INTERFACE_DATA data);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetupDiGetDeviceInterfaceDetailW(IntPtr set, ref SP_DEVICE_INTERFACE_DATA data, IntPtr detail, uint detailSize, out uint required, IntPtr devInfo);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiDestroyDeviceInfoList(IntPtr set);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFileW(string fileName, uint access, uint share, IntPtr sa, uint disposition, uint flags, IntPtr template);

    [DllImport("hid.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool HidD_GetProductString(SafeFileHandle device, StringBuilder buffer, int bufferLength);

    [DllImport("hid.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool HidD_GetManufacturerString(SafeFileHandle device, StringBuilder buffer, int bufferLength);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_GetPreparsedData(SafeFileHandle device, out IntPtr preparsed);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_FreePreparsedData(IntPtr preparsed);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_GetFeature(SafeFileHandle device, byte[] reportBuffer, int reportBufferLength);

    [DllImport("hid.dll")]
    private static extern int HidP_GetCaps(IntPtr preparsed, IntPtr caps);

    [DllImport("hid.dll")]
    private static extern int HidP_GetValueCaps(int reportType, IntPtr valueCaps, ref ushort valueCapsLength, IntPtr preparsed);

    [DllImport("hid.dll")]
    private static extern int HidP_GetUsageValue(
        int reportType,
        ushort usagePage,
        ushort linkCollection,
        ushort usage,
        out uint usageValue,
        IntPtr preparsed,
        byte[] report,
        uint reportLength);
}
