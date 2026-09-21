import { invoke } from "@tauri-apps/api/core";
import { Channel } from "@tauri-apps/api/core";
import type { Macro, MacroStepAt } from "../types";

export const save = (name: string, steps: MacroStepAt[], screenSize: [number, number] | null) =>
  invoke<string>("macro_save", { name, steps, screenSize });

export const list = () => invoke<Macro[]>("macro_list");

export const remove = (id: string) => invoke<void>("macro_delete", { id });

export function play(serial: string, macroId: string, onProgress: Channel<number>) {
  return invoke<void>("macro_play", { serial, macroId, onProgress });
}
