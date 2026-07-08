import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "node:path";

// Tauri 期望一个固定的开发端口，详见 https://tauri.app
export default defineConfig({
  plugins: [vue()],
  // Tauri 接管了开发服务器生命周期，禁止清空终端
  clearScreen: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    hmr: {
      protocol: "ws",
      host: "localhost",
      port: 1421,
    },
    watch: {
      // 排除 src-tauri 中的 Rust 文件，避免 Vite 热重载干扰
      ignored: ["**/src-tauri/**"],
    },
  },
});
