import { serve } from "../../../sdk/ts/src/index";

let counter = 0;
let polls = 0;

serve(
  {
    metrics: [
      { id: "mock.random", title: "随机负载", kind: "ratio", intervalMs: 1000 },
      { id: "mock.counter", title: "计数器", kind: "scalar", intervalMs: 500 },
      { id: "mock.flaky", title: "不稳定指标", kind: "scalar", intervalMs: 2000 },
      { id: "mock.status", title: "Mock 插件", kind: "state", intervalMs: 3000 },
    ],
    poll(ids) {
      polls += 1;
      const out: Record<string, unknown> = {};
      for (const id of ids) {
        switch (id) {
          case "mock.random":
            out[id] = Math.random();
            break;
          case "mock.counter":
            out[id] = counter;
            break;
          case "mock.flaky":
            out[id] =
              polls % 3 === 0
                ? { error: "模拟采集失败" }
                : 40 + Math.round(Math.random() * 8);
            break;
          case "mock.status":
            out[id] = "模拟插件在线";
            break;
        }
      }
      counter += 1;
      return out as any;
    },
  },
  { id: "mock", version: "0.1.0" },
);
