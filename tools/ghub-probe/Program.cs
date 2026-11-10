// 罗技 G HUB 本地集成 API 探测（临时工具）
// G HUB 在 127.0.0.1:9010 开了一个 WebSocket，用 GET 请求就能拿到设备列表/电量。
// 用法：dotnet run -c Release

using System.Net.WebSockets;
using System.Text;
using System.Text.Json;

internal static class Program
{
    private static async Task<int> Main()
    {
        ClientWebSocket? ws = null;
        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(6));
        var variants = new (string Url, string? Origin, string[] Sub)
        {
            ("ws://127.0.0.1:9010/", null, Array.Empty<string>()),
            ("ws://127.0.0.1:9010/", "http://localhost", Array.Empty<string>()),
            ("ws://127.0.0.1:9010/", "https://localhost", Array.Empty<string>()),
            ("ws://127.0.0.1:9010/", null, new[] { "json" }),
            ("ws://localhost:9010/", "http://localhost", Array.Empty<string>()),
            ("ws://127.0.0.1:9010/lghub", "http://localhost", Array.Empty<string>()),
        };
        foreach (var (url, origin, sub) in variants)
        {
            var c = new ClientWebSocket();
            if (origin is not null) c.Options.SetRequestHeader("Origin", origin);
            foreach (var sp in sub) c.Options.AddSubProtocol(sp);
            try
            {
                await c.ConnectAsync(new Uri(url), cts.Token);
                Console.WriteLine($"连上了：{url} origin={origin ?? "-"} sub=[{string.Join(",", sub)}]");
                ws = c;
                break;
            }
            catch (Exception e)
            {
                Console.WriteLine($"  失败 {url} origin={origin ?? "-"} sub=[{string.Join(",", sub)}]：{e.Message}");
                c.Dispose();
            }
        }
        if (ws is null) return 1;

        foreach (var path in new[] { "/devices/list", "/battery/state", "/devices/battery" })
        {
            var reply = await Ask(ws, path, cts.Token);
            Console.WriteLine($"--- GET {path}");
            Console.WriteLine(string.IsNullOrEmpty(reply) ? "   (无回应)" : "   " + reply);
        }

        // 从设备列表里挑有电量的设备，逐个查
        var list = await Ask(ws, "/devices/list", cts.Token);
        foreach (var id in PickIds(list))
        {
            foreach (var p in new[] { $"/battery/{id}/state", $"/battery/{id}", $"/device/{id}/battery" })
            {
                var r = await Ask(ws, p, cts.Token);
                if (!string.IsNullOrEmpty(r))
                {
                    Console.WriteLine($"--- GET {p}");
                    Console.WriteLine("   " + r);
                }
            }
        }
        return 0;
    }

    private static async Task<string> Ask(ClientWebSocket ws, string path, CancellationToken token)
    {
        var payload = JsonSerializer.Serialize(new { msgId = Guid.NewGuid().ToString("N"), verb = "GET", path });
        await ws.SendAsync(Encoding.UTF8.GetBytes(payload), WebSocketMessageType.Text, true, token);
        var buf = new byte[64 * 1024];
        var sb = new StringBuilder();
        try
        {
            while (true)
            {
                var r = await ws.ReceiveAsync(buf, token);
                sb.Append(Encoding.UTF8.GetString(buf, 0, r.Count));
                if (r.EndOfMessage) break;
            }
        }
        catch (OperationCanceledException)
        {
            return "(超时)";
        }
        return sb.ToString();
    }

    private static IEnumerable<string> PickIds(string json)
    {
        var ids = new List<string>();
        try
        {
            using var doc = JsonDocument.Parse(json);
            if (doc.RootElement.TryGetProperty("payload", out var p) &&
                p.TryGetProperty("devices", out var arr))
            {
                foreach (var d in arr.EnumerateArray())
                {
                    foreach (var key in new[] { "id", "deviceId", "serial" })
                    {
                        if (d.TryGetProperty(key, out var v) && v.ValueKind == JsonValueKind.String)
                        {
                            ids.Add(v.GetString()!);
                            break;
                        }
                    }
                }
            }
        }
        catch
        {
            // 解析失败就算了
        }
        return ids.Distinct();
    }
}
