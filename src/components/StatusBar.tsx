/**
 * @file StatusBar.tsx
 * @brief Bottom status bar: machine state on the left, page state on the right
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Presentational only: the page owns the subscription and hands the sample down,
 * so nothing here knows how a pane is implemented.
 */

/** One sample pushed by the backend (`sys-stats`). */
export interface SysStats {
  cpu_percent: number;
  temp_c: number | null;
  mem_used_bytes: number;
  mem_total_bytes: number;
  mem_percent: number;
  uptime_secs: number;
  app_uptime_secs: number;
}

export interface StatusBarProps {
  /** Latest sample; null until the first one arrives */
  stats: SysStats | null;
  /** Effective terminal font zoom of the focused pane, percent */
  fontSizePercent?: number;
  /** Columns on screen */
  columns?: number;
  /** Panes on screen */
  panes?: number;
}

const GIB = 1024 * 1024 * 1024;

/** "6.4 / 15.6 GB" */
function gigabytes(used: number, total: number): string {
  return `${(used / GIB).toFixed(1)} / ${(total / GIB).toFixed(1)} GB`;
}

/** Machine uptime: "3天 4:21" */
function uptimeText(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const clock = `${hours}:${String(minutes).padStart(2, "0")}`;
  return days > 0 ? `${days}天 ${clock}` : clock;
}

/** App run time: "2:18", or "1:02:03" past an hour */
function runTimeText(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

/** Label above a small two-tone progress bar */
function Meter({ percent, barClass }: { percent: number; barClass: string }) {
  return (
    <span className="block h-1 w-[46px] overflow-hidden rounded-[2px] bg-surface0">
      <span className={`block h-full ${barClass}`} style={{ width: `${Math.min(Math.max(percent, 0), 100)}%` }} />
    </span>
  );
}

function StatusBar({ stats, fontSizePercent = 100, columns = 1, panes = 1 }: StatusBarProps) {
  const cpu = stats?.cpu_percent ?? 0;
  const memoryPercent = stats?.mem_percent ?? 0;
  const memoryText = stats && stats.mem_total_bytes > 0 ? gigabytes(stats.mem_used_bytes, stats.mem_total_bytes) : "—";

  return (
    <div className="flex h-[26px] flex-none items-center gap-3 border-t border-surface0 bg-mantle px-2.5 font-mono text-[10.5px] text-overlay1">
      <span className="flex flex-none items-center gap-1.5">
        <span className="tracking-[0.5px] text-overlay0">CPU</span>
        <Meter percent={cpu} barClass="bg-green" />
        <span className="text-subtext0">{Math.round(cpu)}%</span>
      </span>

      <span className="flex flex-none items-center gap-1.5">
        <span className="tracking-[0.5px] text-overlay0">内存</span>
        <Meter percent={memoryPercent} barClass="bg-sky" />
        <span className="text-subtext0">{Math.round(memoryPercent)}%</span>
        <span className="text-overlay0">{memoryText}</span>
      </span>

      <span className="flex flex-none items-center gap-1.5">
        <span className="tracking-[0.5px] text-overlay0">温度</span>
        <span className="text-subtext0">{stats?.temp_c != null ? `${Math.round(stats.temp_c)}°C` : "—"}</span>
      </span>

      <span className="flex-none">
        开机 <span className="text-subtext0">{stats ? uptimeText(stats.uptime_secs) : "—"}</span>
      </span>

      <span className="flex-none">
        运行 <span className="text-subtext0">{stats ? runTimeText(stats.app_uptime_secs) : "—"}</span>
      </span>

      <span className="flex-1" />

      <span className="flex-none">字体 {fontSizePercent}%</span>
      <span className="flex-none">
        栏 {columns} · 面板 {panes}
      </span>
    </div>
  );
}

export default StatusBar;
