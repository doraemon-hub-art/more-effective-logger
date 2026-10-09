/**
 * @file vite.config.ts
 * @brief Vite build config: React + Tailwind v4
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    // xterm.js ships pre-minified; letting esbuild minify it again corrupts its
    // parser (requestMode references a renamed-away binding), which crashes on
    // every DECRQM query (nvim 0.12+ sends those on startup). Terser handles
    // double-minified input correctly.
    minify: "terser",
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
