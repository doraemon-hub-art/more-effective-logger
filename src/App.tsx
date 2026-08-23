/**
 * @file App.tsx
 * @brief App root component: IPC test page
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [ipcResult, setIpcResult] = useState<string>("");

  async function testIpc() {
    try {
      const pong = await invoke<string>("app_ping");
      setIpcResult(pong);
    } catch (e) {
      setIpcResult(`IPC Error: ${e}`);
    }
  }

  return (
    <div className="flex h-screen flex-col items-center justify-center gap-6 bg-gray-950 text-gray-100">
      <h1 className="text-3xl font-bold tracking-tight">
        Developer Workspace Skeleton
      </h1>
      <button
        onClick={testIpc}
        className="rounded-lg bg-blue-600 px-6 py-3 font-medium text-white transition-colors hover:bg-blue-500 active:bg-blue-700"
      >
        Test IPC Connection
      </button>
      {ipcResult && (
        <p className="font-mono text-green-400">{ipcResult}</p>
      )}
    </div>
  );
}

export default App;
