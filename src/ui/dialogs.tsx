import { useEffect, useRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";

// 应用内 Promise 化对话框：替代 window.prompt/confirm。
// WKWebView 对原生对话框支持不可靠（prompt 常返回 null、confirm 返回 false），
// 会导致调用方 `if (!x) return` 静默中止。统一走这里可确保行为一致。

type Mode = "confirm" | "prompt";

interface DialogOpts {
  mode: Mode;
  title: string;
  message?: string;
  defaultValue?: string;
  placeholder?: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
}

let container: HTMLDivElement | null = null;
let root: Root | null = null;
let settle: ((v: never) => void) | null = null;

function Dialog({ opts }: { opts: DialogOpts }) {
  const [value, setValue] = useState(opts.defaultValue ?? "");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (opts.mode === "prompt") inputRef.current?.focus();
  }, [opts.mode]);

  const finish = (v: string | boolean | null) => {
    settle?.(v as never);
    settle = null;
    root?.render(null);
  };

  const cancel = () => finish(opts.mode === "prompt" ? null : false);
  const ok = () => finish(opts.mode === "prompt" ? value.trim() : true);

  return (
    <div className="modal-mask" onClick={cancel}>
      <div className="modal" style={{ width: 420 }} onClick={(e) => e.stopPropagation()}>
        <h3>{opts.title}</h3>
        {opts.message && (
          <p style={{ color: "var(--text-dim)", fontSize: 13, margin: "0 0 12px" }}>
            {opts.message}
          </p>
        )}
        {opts.mode === "prompt" && (
          <label className="field">
            {opts.placeholder ?? "值"}
            <input
              ref={inputRef}
              value={value}
              placeholder={opts.placeholder}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") ok();
                if (e.key === "Escape") cancel();
              }}
            />
          </label>
        )}
        <div className="modal-actions">
          <button className="btn" onClick={cancel}>
            {opts.cancelText ?? "取消"}
          </button>
          <button
            className={`btn ${opts.danger ? "danger" : "primary"}`}
            onClick={ok}
            disabled={opts.mode === "prompt" && value.trim() === ""}
          >
            {opts.confirmText ?? "确定"}
          </button>
        </div>
      </div>
    </div>
  );
}

function show(opts: DialogOpts): Promise<string | boolean | null> {
  return new Promise((resolve) => {
    if (!container) {
      container = document.createElement("div");
      container.id = "ui-dialog-root";
      document.body.appendChild(container);
    }
    if (!root) root = createRoot(container);
    settle = resolve as (v: never) => void;
    root.render(<Dialog opts={opts} />);
  });
}

/** 确认框：确定返回 true，取消返回 false。 */
export function confirmAction(message: string, title = "确认操作"): Promise<boolean> {
  return show({ mode: "confirm", title, message }) as Promise<boolean>;
}

/** 危险操作确认（删除/卸载等）：确定按钮红色。 */
export function confirmDanger(message: string, title = "确认删除"): Promise<boolean> {
  return show({ mode: "confirm", title, message, confirmText: "确定", danger: true }) as Promise<boolean>;
}

/** 输入框：确定返回去除首空白的字符串，取消返回 null。 */
export function promptText(
  title: string,
  defaultValue = "",
  placeholder?: string
): Promise<string | null> {
  return show({ mode: "prompt", title, defaultValue, placeholder }) as Promise<string | null>;
}
