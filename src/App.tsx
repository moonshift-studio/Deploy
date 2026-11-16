import { useEffect, useRef, useState } from "react";
import { GlassSwitch } from "glass-ui";
import {
  AppInfo,
  Progress,
  beginInstall,
  beginUninstall,
  beginUpdate,
  chooseDir,
  getAppInfo,
  onProgress,
  win,
} from "./api";
import logo from "./public/eclipsecentered.png";

type Phase =
  | "loading"
  | "welcome"
  | "config"
  | "folder"
  | "progress"
  | "done"
  | "error"
  | "confirm-uninstall"
  | "uninstalled";

const STAGE_LABEL: Record<string, string> = {
  prepare: "Preparing",
  files: "Copying files",
  cert: "Establishing trust",
  identity: "Registering with Windows",
  shortcuts: "Creating shortcuts",
  registry: "Recording installation",
  launch: "Launching",
  done: "Finishing up",
};

const STAGE_ORDER = ["prepare", "files", "cert", "identity", "shortcuts", "registry", "launch"];

const UNINSTALL_STAGE_ORDER = ["prepare", "identity", "files", "registry"];

const UNINSTALL_STAGE_LABEL: Record<string, string> = {
  prepare: "Preparing",
  identity: "Removing Windows registration",
  files: "Deleting files",
  registry: "Cleaning up",
  done: "Finishing up",
};

const SECTION_TITLE: Partial<Record<Phase, string>> = {
  config: "Settings",
  folder: "Install Folder",
  progress: "Installing",
  done: "Lift off",
  error: "Error",
  "confirm-uninstall": "Uninstall",
  uninstalled: "Uninstalled",
};

const MARQUEE_TEXT = (Array.from({ length: 12 }, () => "moonshift studio").join("   ·   ") + "   ·   ");

function MarqueeStrips() {
  return (
    <>
      <div className="marquee marquee-top" aria-hidden="true">
        <span>{MARQUEE_TEXT + MARQUEE_TEXT}</span>
      </div>
      <div className="marquee marquee-bottom" aria-hidden="true">
        <span>{MARQUEE_TEXT + MARQUEE_TEXT}</span>
      </div>
    </>
  );
}

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [phase, setPhase] = useState<Phase>("loading");
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [dir, setDir] = useState("");
  const [opts, setOpts] = useState({
    startMenu: true,
    desktop: false,
    autostart: true,
    launchAfter: true,
  });
  const modeRef = useRef<"install" | "update" | "uninstall">("install");

  useEffect(() => {
    let cancelled = false;
    getAppInfo().then((i) => {
      if (cancelled) return;
      setInfo(i);
      modeRef.current = i.mode;
      setDir(i.existing_dir ?? i.default_dir);
      if (i.mode === "update") {
        setPhase("progress");
        beginUpdate(i.restart, i.app_args);
      } else if (i.mode === "uninstall") {
        setPhase("confirm-uninstall");
      } else {
        setPhase("welcome");
      }
    });
    const un = onProgress((p) => {
      setProgress(p);
      if (p.done) {
        if (p.error) {
          setError(p.error);
          setPhase("error");
        } else if (modeRef.current === "update") {
          setTimeout(() => void win.close(), 1400);
        } else if (modeRef.current === "uninstall") {
          setPhase("uninstalled");
        } else {
          setPhase("done");
        }
      }
    });
    return () => {
      cancelled = true;
      void un.then((f) => f());
    };
  }, []);

  const isPill = info?.mode === "update";

  const advance = () => {
    if (phase === "welcome") {
      setPhase("config");
    } else if (phase === "config") {
      setPhase("folder");
    } else if (phase === "folder") {
      if (!dir.trim()) return;
      setPhase("progress");
      void beginInstall({
        dir,
        startMenu: opts.startMenu,
        desktop: opts.desktop,
        autostart: opts.autostart,
        launchAfter: opts.launchAfter,
      });
    } else if (phase === "done" || phase === "uninstalled") {
      void win.close();
    }
  };

  const arrowEnabled =
    phase === "welcome" ||
    phase === "config" ||
    (phase === "folder" && dir.trim().length > 0) ||
    phase === "done" ||
    phase === "uninstalled";

  const title =
    phase === "welcome"
      ? info?.app_name || "Deploy"
      : phase === "progress" && modeRef.current === "uninstall"
        ? "Uninstalling"
        : (SECTION_TITLE[phase] ?? info?.app_name) || "Deploy";
  const showVersion = phase === "welcome";
  const subtitle =
    phase === "done"
      ? "made with love by moonshift.studio"
      : phase === "confirm-uninstall" && info != null
        ? `This removes ${info.app_name}, its shortcuts, autostart entry, and Windows registration. Settings and installed files are deleted.`
        : undefined;

  return (
    <div className={`shell ${isPill ? "pill" : ""}`}>
      {isPill ? (
        <PillBody progress={progress} />
      ) : (
        <>
          {!isPill && phase === "welcome" && <MarqueeStrips />}
          {phase === "config" && info != null ? (
            <div className="config-layout">
              <Header
                key={phase}
                title={title}
                showVersion={showVersion}
                showArrow={false}
                subtitle={subtitle}
              />
              <div className="config-side">
                <div className="options">
                  <Toggle
                    label="Start menu shortcut"
                    checked={opts.startMenu}
                    onChange={(v) => setOpts({ ...opts, startMenu: v })}
                  />
                  <Toggle
                    label="Desktop shortcut"
                    checked={opts.desktop}
                    onChange={(v) => setOpts({ ...opts, desktop: v })}
                  />
                  <Toggle
                    label="Start automatically at login"
                    checked={opts.autostart}
                    onChange={(v) => setOpts({ ...opts, autostart: v })}
                  />
                  <Toggle
                    label="Launch when finished"
                    checked={opts.launchAfter}
                    onChange={(v) => setOpts({ ...opts, launchAfter: v })}
                  />
                </div>
                <ArrowButton onAdvance={advance} enabled={arrowEnabled} />
              </div>
            </div>
          ) : (
            <Header
              key={phase}
              title={title}
              showVersion={showVersion}
              subtitle={subtitle}
              wrapSubtitle={phase === "confirm-uninstall"}
              onAdvance={advance}
              arrowEnabled={arrowEnabled}
            />
          )}
          {info == null && <div className="center muted">Loading…</div>}
          {info != null && phase === "folder" && (
            <FolderScreen dir={dir} setDir={setDir} />
          )}
          {phase === "progress" && (
            <ProgressView progress={progress} uninstall={modeRef.current === "uninstall"} />
          )}
          {phase === "error" && error != null && (
            <ErrorView
              message={error}
              onRetry={() => {
                setError(null);
                if (modeRef.current === "uninstall") setPhase("confirm-uninstall");
                else if (modeRef.current === "update") {
                  setPhase("progress");
                  beginUpdate(info?.restart ?? false, info?.app_args ?? []);
                } else setPhase("folder");
              }}
            />
          )}
          {info != null && phase === "confirm-uninstall" && (
            <ConfirmUninstall
              onCancel={() => void win.close()}
              onRemove={() => {
                setPhase("progress");
                void beginUninstall();
              }}
            />
          )}
        </>
      )}
    </div>
  );
}

function ArrowButton({ onAdvance, enabled }: { onAdvance: () => void; enabled: boolean }) {
  return (
    <button
      className="head-arrow"
      onClick={onAdvance}
      disabled={!enabled}
      aria-label="Continue"
    >
      <svg viewBox="0 0 24 24" width="44" height="44" fill="none">
        <path
          d="M4 12h15M13 5l7 7-7 7"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </button>
  );
}

function Header({
  info,
  title,
  showVersion,
  showArrow = true,
  onAdvance,
  arrowEnabled,
  subtitle,
  wrapSubtitle = false,
}: {
  info?: AppInfo | null;
  title: string;
  showVersion: boolean;
  showArrow?: boolean;
  onAdvance?: () => void;
  arrowEnabled?: boolean;
  subtitle?: string;
  wrapSubtitle?: boolean;
}) {
  return (
    <header className="head" data-tauri-drag-region>
      <img className="head-logo" src={logo} alt="" draggable={false} />
      <div className="head-titles">
        <div className="head-title-row">
          <h1 className="head-title">{title}</h1>
          {showVersion && info != null && (
            <span className="head-version">v{info.version}</span>
          )}
        </div>
        {subtitle != null && (
          <span className={`head-subtitle${wrapSubtitle ? " wrap" : ""}`}>
            {subtitle}
          </span>
        )}
      </div>
      {showArrow && onAdvance != null && arrowEnabled != null && (
        <ArrowButton onAdvance={onAdvance} enabled={arrowEnabled} />
      )}
    </header>
  );
}

function FolderScreen({ dir, setDir }: { dir: string; setDir: (v: string) => void }) {
  return (
    <div className="screen">
      <div className="dir-row">
        <input
          className="dir-input"
          value={dir}
          onChange={(e) => setDir(e.target.value)}
          spellCheck={false}
        />
        <button
          className="btn"
          onClick={() =>
            void chooseDir(dir).then((d) => {
              if (d) setDir(d);
            })
          }
        >
          Browse…
        </button>
      </div>
    </div>
  );
}

function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="toggle-row">
      <span className="toggle-label" onClick={() => onChange(!checked)}>
        {label}
      </span>
      <GlassSwitch checked={checked} onChange={onChange} />
    </div>
  );
}

function PillBody({ progress }: { progress: Progress | null }) {
  const stage = progress?.stage ?? "prepare";
  return (
    <div className="pill-body">
      <div className="spinner" />
      <div className="pill-text">
        <div className="pill-stage">{STAGE_LABEL[stage] ?? stage}</div>
        <div className="pill-detail">{progress?.detail ?? "Working"}</div>
      </div>
      <div className="pill-pct">{progress?.percent ?? 0}%</div>
      <div className="bar small">
        <div className="bar-fill" style={{ width: `${progress?.percent ?? 0}%` }} />
      </div>
    </div>
  );
}

function ProgressView({
  progress,
  uninstall,
}: {
  progress: Progress | null;
  uninstall: boolean;
}) {
  const stage = progress?.stage ?? "prepare";
  const order = uninstall ? UNINSTALL_STAGE_ORDER : STAGE_ORDER;
  const labels = uninstall ? UNINSTALL_STAGE_LABEL : STAGE_LABEL;
  return (
    <div className="screen">
      <div className="bar-row">
        <div className="bar">
          <div className="bar-fill" style={{ width: `${progress?.percent ?? 0}%` }} />
        </div>
        <span className="pct">{progress?.percent ?? 0}%</span>
      </div>
      <div className="stage-list">
        {order.map((s) => (
          <StageChip key={s} id={s} current={stage} order={order} labels={labels} />
        ))}
      </div>
    </div>
  );
}

function StageChip({
  id,
  current,
  order,
  labels,
}: {
  id: string;
  current: string;
  order: string[];
  labels: Record<string, string>;
}) {
  const currentIdx = order.indexOf(current);
  const idx = order.indexOf(id);
  const state = idx < currentIdx ? "done" : idx === currentIdx ? "active" : "pending";
  return (
    <span className={`stage-chip ${state}`}>
      {state === "done" ? "✓ " : ""}
      {labels[id] ?? id}
    </span>
  );
}

function ErrorView({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="screen">
      <p className="error-message">{message}</p>
      <div className="actions centered">
        <button className="btn" onClick={() => void win.close()}>
          Close
        </button>
        <button className="btn primary" onClick={onRetry}>
          Try again
        </button>
      </div>
    </div>
  );
}

function ConfirmUninstall({
  onCancel,
  onRemove,
}: {
  onCancel: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="screen">
      <div className="actions centered">
        <button className="btn" onClick={onCancel}>
          Cancel
        </button>
        <button className="btn danger" onClick={onRemove}>
          Remove
        </button>
      </div>
    </div>
  );
}
