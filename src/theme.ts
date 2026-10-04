/*
 * @file theme.ts
 * @brief Catppuccin flavors: one palette feeds the interface and the terminal alike
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-04
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * The interface reads a flavor through the --color-* tokens index.css registers (the
 * palette is written onto the root element), the terminal through the xterm theme built
 * from the same palette. One source, so the two can never drift apart.
 *
 * Mocha is the default and carries the exact values index.css ships, which is what keeps
 * the app looking the way it did before the flavors existed.
 *
 * Values are the Catppuccin palette from upstream; the other flavors are not used here
 * yet, so a typo in them would only show up once someone picks one.
 */
import type { ITheme } from "@xterm/xterm";

/** The flavors the app ships. */
export type ThemeName = "mocha" | "macchiato" | "frappe" | "latte";

/** Everything the interface draws with; the names are the --color-* token names. */
export interface Palette {
  crust: string;
  mantle: string;
  base: string;
  surface0: string;
  surface1: string;
  surface2: string;
  overlay0: string;
  overlay1: string;
  subtext0: string;
  subtext1: string;
  fg: string;
  blue: string;
  sky: string;
  green: string;
  yellow: string;
  red: string;
  mauve: string;
}

/** Flavors in picker order; the labels are what the settings page shows. */
export const FLAVORS: Array<{ id: ThemeName; label: string; palette: Palette }> = [
  {
    id: "mocha",
    label: "Mocha",
    palette: {
      crust: "#11111b",
      mantle: "#181825",
      base: "#1e1e2e",
      surface0: "#313244",
      surface1: "#45475a",
      surface2: "#585b70",
      overlay0: "#6c7086",
      overlay1: "#7f849c",
      subtext0: "#a6adc8",
      subtext1: "#bac2de",
      fg: "#cdd6f4",
      blue: "#89b4fa",
      sky: "#89dceb",
      green: "#a6e3a1",
      yellow: "#f9e2af",
      red: "#f38ba8",
      mauve: "#cba6f7",
    },
  },
  {
    id: "macchiato",
    label: "Macchiato",
    palette: {
      crust: "#181926",
      mantle: "#1e2030",
      base: "#24273a",
      surface0: "#363a4f",
      surface1: "#494d64",
      surface2: "#5b6078",
      overlay0: "#6e738d",
      overlay1: "#8087a2",
      subtext0: "#a5adcb",
      subtext1: "#b8c0e0",
      fg: "#cad3f5",
      blue: "#8aadf4",
      sky: "#91d7e3",
      green: "#a6da95",
      yellow: "#eed49f",
      red: "#ed8796",
      mauve: "#c6a0f6",
    },
  },
  {
    id: "frappe",
    label: "Frappé",
    palette: {
      crust: "#232634",
      mantle: "#292c3c",
      base: "#303446",
      surface0: "#414559",
      surface1: "#51576d",
      surface2: "#626880",
      overlay0: "#737994",
      overlay1: "#838ba7",
      subtext0: "#a5adce",
      subtext1: "#b5bfe2",
      fg: "#c6d0f5",
      blue: "#8caaee",
      sky: "#99d1db",
      green: "#a6d189",
      yellow: "#e5c890",
      red: "#e78284",
      mauve: "#ca9ee6",
    },
  },
  {
    id: "latte",
    label: "Latte",
    palette: {
      crust: "#dce0e8",
      mantle: "#e6e9ef",
      base: "#eff1f5",
      surface0: "#ccd0da",
      surface1: "#bcc0cc",
      surface2: "#acb0be",
      overlay0: "#9ca0b0",
      overlay1: "#8c8fa1",
      subtext0: "#6c6f85",
      subtext1: "#5c5f77",
      fg: "#4c4f69",
      blue: "#1e66f5",
      sky: "#04a5e5",
      green: "#40a02b",
      yellow: "#df8e1d",
      red: "#d20f39",
      mauve: "#8839ef",
    },
  },
];

/** The palette behind a flavor; an unknown name falls back to the default. */
export function paletteOf(name: ThemeName): Palette {
  return (FLAVORS.find(flavor => flavor.id === name) ?? FLAVORS[0]).palette;
}

/**
 * xterm theme for a flavor. The mapping is the one the app always used: the screen sits
 * on the flavor's base, and the ANSI slots borrow the palette's own colors.
 */
export function terminalTheme(name: ThemeName): ITheme {
  const p = paletteOf(name);
  return {
    background: p.base,
    foreground: p.fg,
    cursor: p.fg,
    cursorAccent: p.base,
    selectionBackground: p.surface1,
    black: p.surface1,
    red: p.red,
    green: p.green,
    yellow: p.yellow,
    blue: p.blue,
    magenta: p.mauve,
    cyan: p.sky,
    white: p.subtext1,
    brightBlack: p.surface2,
    brightRed: p.red,
    brightGreen: p.green,
    brightYellow: p.yellow,
    brightBlue: p.blue,
    brightMagenta: p.mauve,
    brightCyan: p.sky,
    brightWhite: p.subtext0,
  };
}
