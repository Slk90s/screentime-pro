// 前端入口：创建 Vue 应用并挂载到 #app
import { createApp } from "vue";
import App from "./App.vue";
import { installGlobalErrorHandlers, log } from "@/lib/logger";
import { i18n } from "./i18n";
import "./style.css";

// v0.4.2 日志：捕获全局未处理错误，写入与 Rust 同一日志文件
installGlobalErrorHandlers();
log.info("前端启动", { version: "0.4.2" });

// 禁用应用内右键上下文菜单（WebView 默认菜单会露出「重新加载 / 检查元素」等，
// 在 Windows(WebView2)/macOS/Linux 下统一屏蔽，避免误操作与界面穿帮）
document.addEventListener("contextmenu", (e) => e.preventDefault());

createApp(App).use(i18n).mount("#app");
