/** Bridge to the Python core (`core/exmt_core`) and the game's bundle folder. */
import { invoke } from "@tauri-apps/api/core";
import { gameBundleDir } from "./settings";

export const DEFAULT_PYTHON =
  "C:\\Users\\Administrator\\.dsh\\dsh-runtimes\\dsh-primary-runtime\\dependencies\\python\\python.exe";
export const DEFAULT_CORE = "C:\\Dev\\EXMT\\core";

/** Bundle folder of the game install chosen in Settings (empty until then). */
export function gameBundles(): string {
  return gameBundleDir.value;
}

export interface CoreOutput {
  code: number;
  stdout: string;
  stderr: string;
}

export async function runCore(
  args: string[],
  python: string = DEFAULT_PYTHON,
  coreDir: string = DEFAULT_CORE,
): Promise<CoreOutput> {
  return invoke<CoreOutput>("run_core", { python, coreDir, args });
}

export function bundlePath(hash: string): string {
  return `${gameBundles()}\\${hash}.bundle`;
}

export interface CharacterPreset {
  id: string;
  name: string;
  note: string;
  mesh: string;
  bundle: string;
  textures: string[];
  textureBundle: string;
}

export const PRESETS: CharacterPreset[] = [
  {
    id: "vepley-ssr",
    name: "维普蕾 · SSR 作战服",
    note: "大厅默认外观；含外套 / 内衬 / 毛绒 / 头发四套网格",
    mesh: "c_VepleySSR01_slg_cloth_lod0",
    bundle: "872f1c4f93f07d6ba65b103ae5d1a337",
    textures: ["c_VepleySSR01_slg_cloth01_d", "c_VepleySSR01_slg_cloth02_d"],
    textureBundle: "287dc73c9302244853e6d72689dd51bb",
  },
];
