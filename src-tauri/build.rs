// v0.9.4（二次重切）：macOS Swift 运行库 rpath 修复（P0）
//
// 为什么不用 .cargo/config.toml 的 [target.*] rustflags？
// 复查发现（CI run 35554355978 产物 Mach-O 解析）：v0.9.4 重切版二进制
// 仍无任何 LC_RPATH 条目，说明 src-tauri/.cargo/config.toml 的 rustflags
// 没有进入 CI 链接。根因：tauri CLI 在【仓库根】目录 spawn cargo，
// cargo 的配置发现机制只从【进程 cwd】向上查找 .cargo/config.toml，
// 永远看不到 src-tauri/.cargo/config.toml（它只在手动 cd src-tauri 构建时生效）。
//
// build.rs 的 cargo:rustc-link-arg 由构建脚本直接回传给 cargo，
// 与 cwd 无关，对本 crate（最终 bin 链接）100% 生效。
//
// 仅 macOS 需要此 rpath：/usr/lib/swift 是 macOS 12+ 系统自带目录，
// 零体积、无需随包分发。Windows/Linux 构建不受影响（cfg 守卫）。
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // Swift 桥 crate（screencapturekit → swift-rs）链接了
        // @rpath/libswift_Concurrency.dylib，必须补 LC_RPATH 才能被 dyld 解析。
        // rustc-link-arg-bins 只注入 bin 目标（不污染 cdylib/staticlib）。
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,/usr/lib/swift");
    }
    tauri_build::build()
}
