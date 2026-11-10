// desktop-monitor 插件 SDK（v1 协议）
// 插件是一个独立进程：宿主通过 stdin 下发 NDJSON/JSON-RPC 2.0 请求，
// 插件通过 stdout 回复。本文件只依赖 Node 内置模块。

export type MetricKind =
  | "scalar"
  | "ratio"
  | "bytes"
  | "duration"
  | "state"
  | "series"
  | "list";

export interface MetricSpec {
  id: string;
  title: string;
  kind: MetricKind;
  unit?: string;
  intervalMs?: number;
  history?: boolean;
  min?: number;
  max?: number;
}

export interface ListRow {
  label: string;
  value: string;
  ratio?: number;
}

export type MetricValue = number | string | number[] | ListRow[];
export type PollValues = Record<string, MetricValue | { error: string }>;

export interface PluginInfo {
  id: string;
  version: string;
}

export interface Plugin {
  metrics: MetricSpec[];
  /** 宿主按各指标 interval 到期后调用，ids 是本次需要取值的指标 */
  poll(ids: string[]): PollValues | Promise<PollValues>;
  setConfig?(config: unknown): void;
  dispose?(): void;
}

interface RpcRequest {
  jsonrpc: string;
  id?: number;
  method?: string;
  params?: any;
}

const PROTOCOL_VERSION = 1;

export function serve(plugin: Plugin, info: PluginInfo): void {
  const send = (msg: unknown) => {
    process.stdout.write(JSON.stringify(msg) + "\n");
  };
  const log = (level: string, message: string) =>
    send({ jsonrpc: "2.0", method: "log", params: { level, message } });

  let buffer = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", async (chunk: string) => {
    buffer += chunk;
    let idx: number;
    while ((idx = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, idx).trim();
      buffer = buffer.slice(idx + 1);
      if (!line) continue;

      let req: RpcRequest;
      try {
        req = JSON.parse(line);
      } catch {
        continue;
      }

      try {
        switch (req.method) {
          case "initialize":
            send({
              jsonrpc: "2.0",
              id: req.id,
              result: {
                protocolVersion: PROTOCOL_VERSION,
                plugin: info,
                metrics: plugin.metrics,
              },
            });
            break;
          case "poll": {
            const ids: string[] = req.params?.ids ?? [];
            const values = await plugin.poll(ids);
            send({ jsonrpc: "2.0", id: req.id, result: { values } });
            break;
          }
          case "setConfig":
            plugin.setConfig?.(req.params?.config);
            send({ jsonrpc: "2.0", id: req.id, result: null });
            break;
          case "shutdown":
            send({ jsonrpc: "2.0", id: req.id, result: null });
            plugin.dispose?.();
            process.exit(0);
            break;
          default:
            send({
              jsonrpc: "2.0",
              id: req.id,
              error: { code: -32601, message: `未知方法 ${req.method}` },
            });
        }
      } catch (e) {
        send({
          jsonrpc: "2.0",
          id: req.id,
          error: { code: -32000, message: String(e) },
        });
      }
    }
  });

  log("info", `插件 ${info.id}@${info.version} 已启动（协议 v${PROTOCOL_VERSION}）`);
}
