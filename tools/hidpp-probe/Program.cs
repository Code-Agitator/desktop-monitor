// 罗技 HID++ 2.0 电量探测（临时工具，用 HidSharp 走正经的 HID 流）
// 协议参考 Ithilias/logitray（MIT）：帧 = [0x11, dev, feature, (func<<4)|swid, p0,p1,p2]
// 用法：dotnet run -c Release

using HidSharp;

internal static class Program
{
    private const byte LongId = 0x11;
    private const byte Swid = 0x0A;

    private static void Main()
    {
        var devices = DeviceList.Local.GetHidDevices(0x046D).ToList();
        Console.WriteLine($"找到 {devices.Count} 个罗技 HID 设备");
        foreach (var d in devices)
        {
            if (d.MaxOutputReportLength < 20) continue;   // 长报表接口才跑 HID++
            Console.WriteLine($"\n== {d.GetProductName()}  in={d.MaxInputReportLength} out={d.MaxOutputReportLength}");
            Console.WriteLine($"   {d.DevicePath}");
            try
            {
                using var stream = d.Open();
                stream.WriteTimeout = 1500;
                stream.ReadTimeout = 600;
                Probe(stream, d);
            }
            catch (Exception e)
            {
                Console.WriteLine($"   打开/通讯失败：{e.GetType().Name}: {e.Message}");
            }
        }
    }

    private static void Probe(HidStream stream, HidDevice d)
    {
        foreach (byte dev in new byte[] { 0xFF, 1, 2, 3, 4, 5, 6 })
        {
            var ping = Ask(stream, d, dev, 0x00, 0x01, new byte[] { 0, 0, 0x5A });
            if (ping is null) { Console.WriteLine($"   dev=0x{dev:X2} 无回应"); continue; }
            Console.WriteLine($"   dev={dev} 在线，HID++ {ping[4]}.{ping[5]}");

            var fs = Ask(stream, d, dev, 0x00, 0x00, new byte[] { 0x00, 0x01, 0x00 });
            if (fs is null || fs[4] == 0) { Console.WriteLine("     没有 IFeatureSet"); continue; }
            var fsIdx = fs[4];
            var cnt = Ask(stream, d, dev, fsIdx, 0x00, new byte[] { 0, 0, 0 });
            var n = cnt?[4] ?? 0;
            Console.WriteLine($"     特征数 {n}");
            var ids = new List<ushort>();
            for (byte i = 0; i < n; i++)
            {
                var f = Ask(stream, d, dev, fsIdx, 0x01, new byte[] { i, 0, 0 });
                if (f is null) continue;
                var id = (ushort)((f[4] << 8) | f[5]);
                ids.Add(id);
                Console.Write($"0x{id:X4} ");
            }
            Console.WriteLine();

            foreach (var fid in new ushort[] { 0x1000, 0x1004, 0x1001 })
            {
                if (!ids.Contains(fid)) continue;
                var gf = Ask(stream, d, dev, 0x00, 0x00, new byte[] { (byte)(fid >> 8), (byte)fid, 0 });
                if (gf is null || gf[4] == 0) continue;
                var idx = gf[4];
                foreach (var fn in new byte[] { 0x00, 0x01 })
                {
                    var r = Ask(stream, d, dev, idx, fn, new byte[] { 0, 0, 0 });
                    if (r is null) continue;
                    Console.WriteLine($"      0x{fid:X4} fn{fn} -> {Hex(r)}");
                    if (fid == 0x1000 && fn == 0x00 && r[4] <= 100) Console.WriteLine($"      >>> 电量 {r[4]}% 状态 {r[5]}");
                    if (fid == 0x1004 && fn == 0x01 && r[4] <= 100) Console.WriteLine($"      >>> 电量 {r[4]}% 状态 {r[5]}");
                    if (fid == 0x1001 && fn == 0x00) Console.WriteLine($"      >>> 电压 {(r[4] << 8) | r[5]} mV 标志 0x{r[6]:X2}");
                }
            }
        }
    }

    private static byte[]? Ask(HidStream stream, HidDevice d, byte dev, byte feature, byte function, byte[]? prms = null)
    {
        var len = Math.Max(20, d.MaxOutputReportLength);
        var req = new byte[len];
        req[0] = LongId;
        req[1] = dev;
        req[2] = feature;
        req[3] = (byte)((function << 4) | Swid);
        if (prms is not null) Array.Copy(prms, 0, req, 4, Math.Min(prms.Length, 3));

        for (var attempt = 0; attempt < 2; attempt++)
        {
            try { stream.Write(req, 0, len); }
            catch (TimeoutException) { Console.WriteLine("     [写超时]"); return null; }
            catch (Exception e) { Console.WriteLine($"     [写失败 {e.GetType().Name}]"); return null; }

            var deadline = Environment.TickCount64 + 600;
            while (Environment.TickCount64 < deadline)
            {
                byte[] buf;
                int n;
                try
                {
                    buf = new byte[d.MaxInputReportLength];
                    n = stream.Read(buf, 0, buf.Length);
                }
                catch (TimeoutException) { break; }
                catch (Exception) { return null; }
                if (n < 7) continue;
                var msg = n >= 8 && buf[0] != LongId && buf[0] != 0x10 ? buf.Skip(1).ToArray() : buf;
                if (msg[1] != dev) continue;
                if (msg[2] == 0xFF || msg[2] == 0x8F) return null;
                if (msg[2] != feature) continue;
                if ((msg[3] & 0x0F) != Swid) continue;
                return msg;
            }
        }
        return null;
    }

    private static string Hex(byte[] b) => string.Join(" ", b.Take(10).Select(x => x.ToString("X2")));
}
