import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export interface AppInfo {
  mode: "install" | "update" | "uninstall";
  app_name: string;
  version: string;
  description: string;
  publisher: string;
  icon: string | null;
  default_dir: string;
  existing_dir: string | null;
  has_identity_package: boolean;
  payload_files: number;
  restart: boolean;
  app_args: string[];
}

export interface Progress {
  stage: string;
  detail: string;
  percent: number;
  done: boolean;
  error: string | null;
}

export const getAppInfo = () => invoke<AppInfo>("app_info");

export const chooseDir = (current: string) =>
  invoke<string | null>("choose_dir", { current });

export const beginInstall = (opts: {
  dir: string;
  startMenu: boolean;
  desktop: boolean;
  autostart: boolean;
  launchAfter: boolean;
}) => invoke("begin_install", opts);

export const beginUpdate = (restart: boolean, appArgs: string[]) =>
  invoke("begin_update", { restart, appArgs });

export const beginUninstall = () => invoke("begin_uninstall");

export const onProgress = (cb: (p: Progress) => void) =>
  listen<Progress>("deploy://progress", (e) => cb(e.payload));

export const win = getCurrentWindow();
