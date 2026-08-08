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
    // 强制 IPv4：本机 hosts 已注释 localhost 映射（走 DNS），WebView2 解析
    // localhost 可能先命中 ::1(IPv6)，而 dev server 仅监听 127.0.0.1(IPv4)，
    // 导致窗口白屏/黑屏且前端永不加载。显式绑定 127.0.0.1 消除歧义。
    host: "127.0.0.1",
    hmr: {
      protocol: "ws",
      host: "127.0.0.1",
      port: 1421,
    },
    watch: {
      // 排除 src-tauri 中的 Rust 文件，避免 Vite 热重载干扰
      ignored: ["**/src-tauri/**"],
    },
  },
});
