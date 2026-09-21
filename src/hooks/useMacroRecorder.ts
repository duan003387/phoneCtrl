import { useCallback, useEffect, useRef, useState } from "react";
import type { MacroStep, MacroStepAt } from "../types";

export interface MacroRecorder {
  recording: boolean;
  /** 当前录制时长（ms） */
  elapsed: number;
  start: () => void;
  /** 结束录制并返回动作序列 */
  finish: () => MacroStepAt[];
  cancel: () => void;
  /** 供 MirrorCanvas / 控制条推入动作 */
  push: (step: MacroStep) => void;
}

/**
 * 宏录制器：记录相对起始时间的动作序列（含 Wait 间隔由 push 自动推断）。
 */
export function useMacroRecorder(): MacroRecorder {
  const [recording, setRecording] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const t0Ref = useRef(0);
  const lastRef = useRef(0);
  const stepsRef = useRef<MacroStepAt[]>([]);

  const push = useCallback(
    (step: MacroStep) => {
      if (!recording) return;
      const now = performance.now();
      const ts = Math.round(now - t0Ref.current);
      // 合并过近的连续动作（去抖），避免长按拖拽产生海量事件
      const steps = stepsRef.current;
      if (steps.length > 0) {
        const last = steps[steps.length - 1];
        if (ts - last.ts < 30) return;
      }
      steps.push({ ts, step });
    },
    [recording],
  );

  const start = useCallback(() => {
    t0Ref.current = performance.now();
    lastRef.current = t0Ref.current;
    stepsRef.current = [];
    setElapsed(0);
    setRecording(true);
  }, []);

  const finish = useCallback((): MacroStepAt[] => {
    setRecording(false);
    return stepsRef.current;
  }, []);

  const cancel = useCallback(() => {
    setRecording(false);
    stepsRef.current = [];
    setElapsed(0);
  }, []);

  // 计时
  useEffect(() => {
    if (!recording) return;
    const t = setInterval(() => {
      setElapsed(performance.now() - t0Ref.current);
    }, 200);
    return () => clearInterval(t);
  }, [recording]);

  return { recording, elapsed, start, finish, cancel, push };
}
