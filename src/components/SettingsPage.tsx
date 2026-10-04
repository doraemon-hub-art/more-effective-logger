/**
 * @file SettingsPage.tsx
 * @brief The system settings page: categories on the left, every setting in one column
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-03
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * A page of its own, not a pane: it holds no terminal to resize and no shell to kill, so
 * opening it is only another entry in the page strip.
 *
 * Presentational: the values live in the app and are handed down here, so this file only
 * knows how to draw them and what a click means. The category menu jumps within the
 * column and follows the scroll, which for four short groups is enough — no scrolling
 * indirection beyond that.
 */
import { useEffect, useRef, useState, type ReactNode } from "react";
import { FLAVORS, type ThemeName } from "../theme";

/** What the app brings up on a start. */
export type StartupMode = "restore" | "fresh";

/**
 * Everything the settings page can change, in one bag: wiring a new value up is a field
 * plus a row, and nothing else has to learn about it.
 */
export interface AppSettings {
  /** Family name of the monospace font the UI and the terminal use. */
  fontFamily: string;
  /** Font size in px; the terminal follows it. */
  fontSize: number;
  /**
   * Whole-interface zoom: the webview's own page zoom (browser Ctrl+wheel semantics),
   * applied by App through a backend call; the terminal screens scale with the UI.
   */
  zoom: number;
  /** Catppuccin flavor. */
  theme: ThemeName;
  /** Terminal cursor blink. */
  cursorBlink: boolean;
  /** Terminal scrollback lines. */
  scrollback: number;
  /** Baud rate a freshly opened serial port starts with. */
  baud: number;
  /** Serial frame splitting: the idle gap that ends a frame, in ms. */
  frameGapMs: number;
  /** A start opens the layout left behind last time, or a single new page. */
  startup: StartupMode;
}

/** Font size the status bar calls 100%. */
export const BASE_FONT_SIZE = 12.5;

export const DEFAULT_SETTINGS: AppSettings = {
  fontFamily: "JetBrains Mono",
  fontSize: BASE_FONT_SIZE,
  zoom: 1,
  theme: "mocha",
  cursorBlink: true,
  scrollback: 5000,
  baud: 115200,
  frameGapMs: 2,
  startup: "restore",
};

/** Families offered when the scan came back empty; the names are real family names. */
const FONT_FALLBACK = ["JetBrains Mono", "Fira Code", "DejaVu Sans Mono", "Noto Sans Mono"];
/** Font sizes offered. */
const FONT_STEPS = [11.5, 12.5, 14];
/** Interface zoom levels offered, as multipliers of the designed sizes. */
const ZOOM_STEPS = [0.8, 0.9, 1, 1.1, 1.25, 1.5];
/** Scrollback lines offered. */
const SCROLLBACK_STEPS = [1000, 5000, 20000];
/** Baud rates offered; these are the ones the backend takes. */
const BAUD_STEPS = [9600, 115200, 921600];
/** Serial frame gaps offered. */
const FRAME_GAP_STEPS = [2, 5, 10];
/** Startup behaviors offered. */
const STARTUP_STEPS: Array<{ id: StartupMode; label: string }> = [
  { id: "restore", label: "上次布局" },
  { id: "fresh", label: "新页面" },
];

/** The categories, in the order they appear in the column. */
const SECTIONS = [
  { id: "gui", title: "界面" },
  { id: "term", title: "终端" },
  { id: "serial", title: "串口" },
  { id: "keys", title: "快捷键" },
];

/** How close to the top of the column a category counts as the current one, in px. */
const FOLLOW_MARGIN = 16;

export interface SettingsPageProps {
  settings: AppSettings;
  /** Hand the changed fields back; the app owns the values. */
  onChange: (patch: Partial<AppSettings>) => void;
  /** Monospace families installed on this machine, from the backend scan. */
  fonts: string[];
}

/** How long the jump highlight holds before it fades back to normal, in ms. */
const FLASH_MS = 700;

/** One category: a highlighted title row with its settings under it. */
function Group({
  id,
  title,
  flash,
  children,
}: {
  id: string;
  title: string;
  /** Just jumped to: the title band lights up, then fades out by itself. */
  flash: boolean;
  children: ReactNode;
}) {
  return (
    <div id={id} className="scroll-mt-2">
      <div
        className={`mb-0.5 rounded-sm px-2 py-1 text-center font-mono text-[13px] transition-colors duration-300 ${
          flash ? "bg-blue/40 text-base" : "bg-surface0/45 text-fg"
        }`}
      >
        {title}
      </div>
      {children}
    </div>
  );
}

/** One setting: name and note on the left, choices on the right end of the row. */
function Row({ label, note, children }: { label: string; note?: string; children: ReactNode }) {
  return (
    <div className="flex h-9 items-center gap-2.5 border-b border-surface0/55 last:border-b-0">
      <span className="flex-none text-[12px] text-fg">{label}</span>
      {note ? <span className="flex-none font-mono text-[10.5px] text-overlay0">{note}</span> : null}
      <span className="flex-1" />
      <span className="flex flex-none items-center gap-1.5">{children}</span>
    </div>
  );
}

/** One choice of a row; the picked one reads green, like the serial baud row. */
function Chip({ on, onClick, children }: { on: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-sm border px-1.5 py-px font-mono text-[11px] ${
        on ? "border-green bg-green/10 text-green" : "border-surface0 text-subtext0 hover:border-subtext0 hover:text-fg"
      }`}
    >
      {children}
    </button>
  );
}

/** Width of a picker, trigger and list alike. */
const PICK_WIDTH = 164;

/**
 * A choice picked from a list: the row shows the current value, clicking it drops the
 * list open over the rows below. Used where the choices are too many or too long to sit
 * in a row as chips.
 */
function PickList({
  options,
  value,
  onPick,
}: {
  options: Array<{ id: string; label: string }>;
  value: string;
  onPick: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const boxRef = useRef<HTMLSpanElement | null>(null);

  // Open list: clicking anywhere else, or Escape, folds it back up.
  useEffect(() => {
    if (!open) return;
    const onMouseDown = (event: globalThis.MouseEvent) => {
      if (!boxRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onMouseDown, true);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mousedown", onMouseDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const current = options.find(option => option.id === value) ?? options[0];

  return (
    <span ref={boxRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen(opened => !opened)}
        className="flex items-center justify-between gap-2 rounded-sm border border-surface0 px-2 py-[3px] font-mono text-[11px] text-subtext0 hover:border-subtext0 hover:text-fg"
        style={{ width: PICK_WIDTH }}
      >
        <span className="truncate">{current.label}</span>
        <span className="flex-none text-overlay0">▾</span>
      </button>

      {open ? (
        <div
          className="absolute right-0 top-full z-20 mt-1 rounded-md border border-surface0 bg-mantle py-1 shadow-[0_6px_20px_rgba(0,0,0,0.45)]"
          style={{ width: PICK_WIDTH }}
        >
          {options.map(option => (
            <button
              key={option.id}
              type="button"
              onClick={() => {
                onPick(option.id);
                setOpen(false);
              }}
              className={`flex w-full items-center px-2 py-[4px] text-left font-mono text-[11px] hover:bg-surface0 ${
                option.id === value ? "text-green" : "text-subtext0"
              }`}
            >
              {option.label}
            </button>
          ))}
        </div>
      ) : null}
    </span>
  );
}

/** A key in a read-only shortcut row. */
function Key({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded-sm border border-surface0 bg-surface0 px-1.5 py-px font-mono text-[10.5px] text-fg">
      {children}
    </kbd>
  );
}

function SettingsPage({ settings, onChange, fonts }: SettingsPageProps) {
  const columnRef = useRef<HTMLDivElement | null>(null);
  const [current, setCurrent] = useState(SECTIONS[0].id);
  const [flash, setFlash] = useState<string | null>(null);
  const flashTimer = useRef(0);

  // Leaving the page while a flash is pending must not fire it into a dead component.
  useEffect(() => () => window.clearTimeout(flashTimer.current), []);

  /** Menu click: bring that category to the top of the column and flash its title. */
  const jump = (id: string) => {
    setCurrent(id);
    columnRef.current?.querySelector(`#${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
    setFlash(id);
    window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlash(null), FLASH_MS);
  };

  /** The menu follows the scroll, so the two never disagree about where you are. */
  const follow = () => {
    const column = columnRef.current;
    if (!column) return;
    const top = column.getBoundingClientRect().top;
    let next = SECTIONS[0].id;
    for (const section of SECTIONS) {
      const group = column.querySelector(`#${section.id}`);
      if (group && group.getBoundingClientRect().top - top <= FOLLOW_MARGIN) next = section.id;
    }
    setCurrent(picked => (picked === next ? picked : next));
  };

  return (
    // Capped and centred: on a wide window the block stays a readable width instead of
    // stretching the fields and their values to opposite edges. Full height is what the
    // column scrolls against — without it the page grows to its content and a short
    // window pushes the rows off the bottom instead of letting them scroll.
    <div className="mx-auto flex h-full min-h-0 min-w-0 w-full max-w-[900px] bg-base">
      <nav className="flex w-[168px] flex-none flex-col border-r border-surface0 bg-mantle py-1.5">
        {SECTIONS.map(section => (
          <button
            key={section.id}
            type="button"
            onClick={() => jump(section.id)}
            className={`flex h-[30px] items-center border-l-2 px-3 text-left text-[11.5px] ${
              current === section.id
                ? "border-blue bg-base text-fg"
                : "border-transparent text-overlay1 hover:text-subtext1"
            }`}
          >
            {section.title}
          </button>
        ))}
      </nav>

      {/* The column follows the window: rows run the full width, so a maximized window
          is used instead of leaving the settings stranded in a corner. */}
      <div ref={columnRef} onScroll={follow} className="min-h-0 flex-1 overflow-y-auto px-5 pb-6 pt-3">
        <div className="space-y-5">
          <Group id="gui" title="界面" flash={flash === "gui"}>
            <Row label="字体">
              <PickList
                options={(fonts.length ? fonts : FONT_FALLBACK).map(name => ({ id: name, label: name }))}
                value={settings.fontFamily}
                onPick={id => onChange({ fontFamily: id })}
              />
            </Row>
            <Row label="字体大小" note="终端一并生效">
              {FONT_STEPS.map(size => (
                <Chip key={size} on={settings.fontSize === size} onClick={() => onChange({ fontSize: size })}>
                  {size} px
                </Chip>
              ))}
            </Row>
            <Row label="整体缩放" note="界面与终端一起">
              {ZOOM_STEPS.map(step => (
                <Chip key={step} on={settings.zoom === step} onClick={() => onChange({ zoom: step })}>
                  {Math.round(step * 100)}%
                </Chip>
              ))}
            </Row>
            <Row label="主题" note="Catppuccin">
              {FLAVORS.map(flavor => (
                <Chip key={flavor.id} on={settings.theme === flavor.id} onClick={() => onChange({ theme: flavor.id })}>
                  <span
                    className="mr-1.5 inline-block h-2 w-2 rounded-full border border-black/35 align-[-1px]"
                    style={{ background: flavor.palette.base }}
                  />
                  {flavor.label}
                  {flavor.id === DEFAULT_SETTINGS.theme ? " (default)" : null}
                </Chip>
              ))}
            </Row>
            <Row label="启动时">
              {STARTUP_STEPS.map(step => (
                <Chip key={step.id} on={settings.startup === step.id} onClick={() => onChange({ startup: step.id })}>
                  {step.label}
                </Chip>
              ))}
            </Row>
          </Group>

          <Group id="term" title="终端" flash={flash === "term"}>
            <Row label="光标闪烁">
              <Chip on={settings.cursorBlink} onClick={() => onChange({ cursorBlink: true })}>
                开
              </Chip>
              <Chip on={!settings.cursorBlink} onClick={() => onChange({ cursorBlink: false })}>
                关
              </Chip>
            </Row>
            <Row label="回滚行数">
              {SCROLLBACK_STEPS.map(lines => (
                <Chip key={lines} on={settings.scrollback === lines} onClick={() => onChange({ scrollback: lines })}>
                  {lines}
                </Chip>
              ))}
            </Row>
          </Group>

          <Group id="serial" title="串口" flash={flash === "serial"}>
            <Row label="默认波特率">
              {BAUD_STEPS.map(rate => (
                <Chip key={rate} on={settings.baud === rate} onClick={() => onChange({ baud: rate })}>
                  {rate}
                </Chip>
              ))}
            </Row>
            <Row label="切帧空闲" note="线路静默超过这么久算一帧结束">
              {FRAME_GAP_STEPS.map(gap => (
                <Chip key={gap} on={settings.frameGapMs === gap} onClick={() => onChange({ frameGapMs: gap })}>
                  {gap} ms
                </Chip>
              ))}
            </Row>
          </Group>

          <Group id="keys" title="快捷键" flash={flash === "keys"}>
            <Row label="新建页面">
              <Key>Ctrl</Key>
              <Key>Shift</Key>
              <Key>T</Key>
            </Row>
            <Row label="关闭页面">
              <Key>Ctrl</Key>
              <Key>Shift</Key>
              <Key>W</Key>
            </Row>
            <Row label="面板焦点">
              <Key>Alt</Key>
              <Key>←→↑↓</Key>
            </Row>
            <Row label="切换页面">
              <Key>Alt</Key>
              <Key>PageUp</Key>
              <Key>PageDown</Key>
            </Row>
            <Row label="打开设置">
              <Key>面板右键</Key>
            </Row>
          </Group>
        </div>
      </div>
    </div>
  );
}

export default SettingsPage;
