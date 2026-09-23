import { useCallback, useEffect, useState } from "react";
import {
  run as automateRun,
  env as automateEnv,
  BY_OPTIONS,
  type Step,
  type StepResult,
} from "../api/automate";

type AnyStep = { type: string } & Record<string, unknown>;

interface Props {
  serial: string | null;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

const STEP_TYPES: { value: string; label: string }[] = [
  { value: "openApp", label: "启动应用" },
  { value: "tap", label: "点击（控件/坐标）" },
  { value: "input", label: "输入文本" },
  { value: "swipe", label: "滑动" },
  { value: "waitFor", label: "等待出现控件" },
  { value: "assertText", label: "断言含文本" },
  { value: "wait", label: "固定等待" },
  { value: "key", label: "按键" },
  { value: "back", label: "返回" },
  { value: "home", label: "主屏" },
  { value: "screenshot", label: "截图" },
];

function blank(type: string): AnyStep {
  switch (type) {
    case "openApp":
      return { type, package: "" };
    case "tap":
      return { type, by: "text", value: "" };
    case "input":
      return { type, by: "text", value: "", text: "" };
    case "swipe":
      return { type, direction: "up" };
    case "waitFor":
      return { type, by: "text", value: "", timeoutMs: 8000 };
    case "assertText":
      return { type, contains: "", timeoutMs: 2000 };
    case "wait":
      return { type, ms: 1000 };
    case "key":
      return { type, keycode: 4 };
    default:
      return { type };
  }
}

const LS_KEY = "phonectrl.autocase.v1";

export function AutoPanel({ serial, notify }: Props) {
  const [steps, setSteps] = useState<AnyStep[]>(() => {
    try {
      const raw = localStorage.getItem(LS_KEY);
      return raw ? (JSON.parse(raw) as AnyStep[]) : [];
    } catch {
      return [];
    }
  });
  const [running, setRunning] = useState(false);
  const [results, setResults] = useState<StepResult[] | null>(null);
  const [overall, setOverall] = useState<{ ok: boolean; ms: number } | null>(null);
  const [envInfo, setEnvInfo] = useState<{ mode: string; appiumReady: boolean } | null>(null);
  const [viewShot, setViewShot] = useState<string | null>(null);

  useEffect(() => {
    localStorage.setItem(LS_KEY, JSON.stringify(steps));
  }, [steps]);

  useEffect(() => {
    void automateEnv().then(setEnvInfo).catch(() => setEnvInfo(null));
  }, []);

  const patch = (i: number, kv: Partial<AnyStep>) =>
    setSteps((prev) => prev.map((s, idx) => (idx === i ? { ...s, ...kv } : s)));
  const move = (i: number, dir: -1 | 1) =>
    setSteps((prev) => {
      const j = i + dir;
      if (j < 0 || j >= prev.length) return prev;
      const a = [...prev];
      [a[i], a[j]] = [a[j], a[i]];
      return a;
    });
  const remove = (i: number) => setSteps((prev) => prev.filter((_, idx) => idx !== i));
  const add = (type: string) => setSteps((prev) => [...prev, blank(type)]);

  const doRun = useCallback(async () => {
    if (!serial) {
      notify("请先在顶栏选择设备", "err");
      return;
    }
    if (steps.length === 0) {
      notify("请先添加至少一个步骤", "err");
      return;
    }
    setRunning(true);
    setResults(null);
    setOverall(null);
    try {
      const rep = await automateRun(serial, steps as unknown as Step[]);
      setResults(rep.steps);
      setOverall({ ok: rep.ok, ms: rep.elapsedMs });
      notify(rep.ok ? "用例全部通过 ✅" : "用例失败，见结果", rep.ok ? "ok" : "err");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setRunning(false);
    }
  }, [serial, steps, notify]);

  const labelOf = (t: string) => STEP_TYPES.find((x) => x.value === t)?.label ?? t;

  return (
    <div className="panel">
      <div className="toolbar">
        <select
          className="field-inline"
          defaultValue=""
          onChange={(e) => {
            if (e.target.value) {
              add(e.target.value);
              e.currentTarget.value = "";
            }
          }}
        >
          <option value="">＋ 添加步骤…</option>
          {STEP_TYPES.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </select>
        <button className="btn" onClick={() => setSteps([])} disabled={steps.length === 0}>
          清空
        </button>
        <span style={{ flex: 1 }} />
        {envInfo && (
          <span className="ctl-hint">
            Appium：{envInfo.mode}
            {envInfo.appiumReady ? "（服务已就绪）" : "（首次运行自动准备）"}
          </span>
        )}
        <button className="btn primary" disabled={running || !serial} onClick={() => void doRun()}>
          {running ? "运行中…（首次会自动安装/启动 Appium，较慢）" : "▶ 运行用例"}
        </button>
      </div>

      {overall && (
        <div
          className="field-inline"
          style={{
            padding: "8px 12px",
            borderRadius: 8,
            background: overall.ok ? "rgba(63,185,80,.15)" : "rgba(229,83,75,.15)",
            color: overall.ok ? "var(--ok)" : "var(--danger)",
          }}
        >
          {overall.ok ? "✅ 用例通过" : "❌ 用例失败"} · {overall.ms} ms
        </div>
      )}

      {/* 步骤编辑 */}
      <div className="table-wrap">
        {steps.length === 0 ? (
          <div className="empty">还没有步骤。上方「＋ 添加步骤」开始编排，运行时将通过 Appium(UiAutomator2) 逐步执行。</div>
        ) : (
          <table className="file-table">
            <tbody>
              {steps.map((s, i) => (
                <tr key={i}>
                  <td style={{ width: 28 }}>{i + 1}</td>
                  <td style={{ width: 150 }}>
                    <select
                      value={s.type}
                      onChange={(e) => setSteps((p) => p.map((x, idx) => (idx === i ? { ...blank(e.target.value), type: e.target.value } : x)))}
                    >
                      <option value={s.type}>{labelOf(s.type)}</option>
                      {STEP_TYPES.filter((t) => t.value !== s.type).map((t) => (
                        <option key={t.value} value={t.value}>
                          {t.label}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td>{renderFields(s, i, patch)}</td>
                  <td style={{ width: 90 }} className="row-actions">
                    <button className="btn mini" onClick={() => move(i, -1)} title="上移">↑</button>
                    <button className="btn mini" onClick={() => move(i, 1)} title="下移">↓</button>
                    <button className="btn mini danger" onClick={() => remove(i)} title="删除">✕</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* 运行结果 */}
      {results && (
        <div className="table-wrap" style={{ maxHeight: "32vh" }}>
          <table className="file-table">
            <thead>
              <tr>
                <th style={{ width: 40 }}>#</th>
                <th>步骤</th>
                <th style={{ width: 60 }}>结果</th>
                <th style={{ width: 70 }}>耗时</th>
                <th>信息</th>
              </tr>
            </thead>
            <tbody>
              {results.map((r) => (
                <tr key={r.index}>
                  <td>{r.index + 1}</td>
                  <td>{r.label}</td>
                  <td style={{ color: r.ok ? "var(--ok)" : "var(--danger)" }}>{r.ok ? "通过" : "失败"}</td>
                  <td className="mono">{r.elapsedMs}ms</td>
                  <td>
                    {r.message}
                    {r.screenshot && (
                      <button className="btn mini" style={{ marginLeft: 8 }} onClick={() => setViewShot(r.screenshot!)}>
                        看失败截图
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {viewShot && (
        <div className="modal-mask" onClick={() => setViewShot(null)}>
          <div className="modal" style={{ width: "auto", maxWidth: "80vw" }} onClick={(e) => e.stopPropagation()}>
            <h3>失败时刻截图</h3>
            <img src={`data:image/png;base64,${viewShot}`} alt="" style={{ maxWidth: "100%", borderRadius: 8 }} />
            <div className="modal-actions">
              <button className="btn" onClick={() => setViewShot(null)}>关闭</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function renderFields(s: AnyStep, i: number, patch: (i: number, kv: Partial<AnyStep>) => void) {
  const bySelect = (
    <select value={String(s.by ?? "text")} onChange={(e) => patch(i, { by: e.target.value })}>
      {BY_OPTIONS.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
  const text = (key: string, ph: string, w = 200) => (
    <input
      style={{ width: w }}
      placeholder={ph}
      value={String(s[key] ?? "")}
      onChange={(e) => patch(i, { [key]: e.target.value } as Partial<AnyStep>)}
    />
  );
  const num = (key: string, ph: string) => (
    <input
      type="number"
      style={{ width: 90 }}
      placeholder={ph}
      value={String(s[key] ?? "")}
      onChange={(e) => patch(i, { [key]: Number(e.target.value) } as Partial<AnyStep>)}
    />
  );
  switch (s.type) {
    case "openApp":
      return text("package", "包名，如 com.android.settings", 260);
    case "tap":
      return (
        <span style={{ display: "inline-flex", gap: 6, flexWrap: "wrap" }}>
          {bySelect}
          {text("value", "定位值")}
          <span style={{ color: "var(--text-dim)" }}>或坐标</span>
          {num("x", "x")}
          {num("y", "y")}
        </span>
      );
    case "input":
      return (
        <span style={{ display: "inline-flex", gap: 6 }}>
          {bySelect}
          {text("value", "定位值", 160)}
          {text("text", "要输入的文本", 160)}
        </span>
      );
    case "swipe":
      return (
        <select value={String(s.direction ?? "up")} onChange={(e) => patch(i, { direction: e.target.value })}>
          {["up", "down", "left", "right"].map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
      );
    case "waitFor":
      return (
        <span style={{ display: "inline-flex", gap: 6 }}>
          {bySelect}
          {text("value", "定位值")}
          {num("timeoutMs", "超时ms")}
        </span>
      );
    case "assertText":
      return (
        <span style={{ display: "inline-flex", gap: 6 }}>
          {text("contains", "期望出现的文本", 220)}
          {num("timeoutMs", "超时ms")}
        </span>
      );
    case "wait":
      return num("ms", "毫秒");
    case "key":
      return num("keycode", "keycode");
    default:
      return <span style={{ color: "var(--text-dim)" }}>—</span>;
  }
}
