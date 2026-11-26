//! 键盘输入统计：平台只读钩子 → HID 键位事件 → 按日聚合计数。
//! 不记录字符、顺序、逐事件时间、应用或窗口内容。

use crate::db::AppDb;
use chrono::Local;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

type Pending = Arc<Mutex<HashMap<(String, i32), i64>>>;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::{spawn as spawn_hook, InputHookHandle};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::{spawn as spawn_hook, InputHookHandle};
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::{spawn as spawn_hook, InputHookHandle};

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
struct InputHookHandle;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl InputHookHandle {
    fn stop(&mut self) {}
}
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn spawn_hook(_callback: Arc<dyn Fn(i32) + Send + Sync>) -> Result<InputHookHandle, String> {
    Err("当前平台尚未实现键盘输入采集".to_string())
}

struct Worker {
    hook: InputHookHandle,
    aggregate: JoinHandle<()>,
}

impl Worker {
    fn stop(mut self) {
        self.hook.stop();
        if self.aggregate.join().is_err() {
            tracing::warn!("键盘聚合线程异常退出");
        }
    }
}

pub struct KeyboardInputManager {
    db: Arc<AppDb>,
    device_id: String,
    enabled: AtomicBool,
    worker: Mutex<Option<Worker>>,
    last_error: Mutex<Option<String>>,
    pending: Pending,
    operation: Mutex<()>,
}

impl KeyboardInputManager {
    pub fn new(db: Arc<AppDb>, device_id: String) -> Self {
        Self {
            db,
            device_id,
            enabled: AtomicBool::new(false),
            worker: Mutex::new(None),
            last_error: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            operation: Mutex::new(()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn operation(&self) -> std::sync::MutexGuard<'_, ()> {
        self.operation.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn flush(&self) -> Result<(), String> {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        flush_pending(&self.db, &self.device_id, &mut pending)
    }

    pub fn stop_and_flush(&self) -> Result<(), String> {
        self.shutdown();
        self.flush()
    }

    pub fn is_running(&self) -> bool {
        let guard = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .as_ref()
            .is_some_and(|worker| !worker.aggregate.is_finished())
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn record_error(&self, error: Option<String>) {
        *self.last_error.lock().unwrap_or_else(|e| e.into_inner()) = error;
    }

    pub fn supported() -> bool {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            true
        }
        #[cfg(target_os = "linux")]
        {
            std::env::var_os("DISPLAY").is_some()
                && std::env::var("XDG_SESSION_TYPE")
                    .map(|session| !session.eq_ignore_ascii_case("wayland"))
                    .unwrap_or(true)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut guard = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        let running = guard
            .as_ref()
            .is_some_and(|worker| !worker.aggregate.is_finished());
        if enabled == self.enabled() && running == enabled {
            return Ok(());
        }
        if !enabled {
            self.enabled.store(false, Ordering::Release);
            self.record_error(None);
            if let Some(worker) = guard.take() {
                worker.stop();
            }
            return Ok(());
        }
        if let Some(worker) = guard.take() {
            worker.stop();
        }

        let (sender, receiver) = mpsc::channel::<i32>();
        let callback = Arc::new(move |key_id| {
            let _ = sender.send(key_id);
        });
        let mut hook = match spawn_hook(callback) {
            Ok(hook) => hook,
            Err(error) => {
                self.record_error(Some(error.clone()));
                return Err(error);
            }
        };
        let db = Arc::clone(&self.db);
        let device_id = self.device_id.clone();
        let pending = Arc::clone(&self.pending);
        let aggregate = match std::thread::Builder::new()
            .name("keyboard-aggregate".to_string())
            .spawn(move || aggregate_loop(receiver, db, device_id, pending))
        {
            Ok(handle) => handle,
            Err(error) => {
                hook.stop();
                let message = format!("启动键盘聚合线程失败: {error}");
                self.record_error(Some(message.clone()));
                return Err(message);
            }
        };
        *guard = Some(Worker { hook, aggregate });
        self.record_error(None);
        self.enabled.store(true, Ordering::Release);
        Ok(())
    }

    pub fn shutdown(&self) {
        self.enabled.store(false, Ordering::Release);
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            worker.stop();
        }
    }
}

impl Drop for KeyboardInputManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn aggregate_loop(
    receiver: mpsc::Receiver<i32>,
    db: Arc<AppDb>,
    device_id: String,
    pending: Pending,
) {
    let mut last_flush = Instant::now();
    loop {
        let timeout = Duration::from_secs(5).saturating_sub(last_flush.elapsed());
        match receiver.recv_timeout(timeout) {
            Ok(key_id) => {
                let day = Local::now().format("%Y-%m-%d").to_string();
                let mut pending = pending.lock().unwrap_or_else(|e| e.into_inner());
                *pending.entry((day, key_id)).or_insert(0) += 1;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if let Err(error) = flush_pending(
                    &db,
                    &device_id,
                    &mut pending.lock().unwrap_or_else(|e| e.into_inner()),
                ) {
                    tracing::warn!(%error, "键盘聚合线程退出时仍有计数待落库，已保留给下次启动重试");
                }
                break;
            }
        }
        if last_flush.elapsed() >= Duration::from_secs(5) {
            if let Err(error) = flush_pending(
                &db,
                &device_id,
                &mut pending.lock().unwrap_or_else(|e| e.into_inner()),
            ) {
                tracing::warn!(%error, "键盘计数批量落库失败，保留到下一批次重试");
            }
            last_flush = Instant::now();
        }
    }
}

fn flush_pending(
    db: &AppDb,
    device_id: &str,
    pending: &mut HashMap<(String, i32), i64>,
) -> Result<(), String> {
    if pending.is_empty() {
        return Ok(());
    }
    let entries: Vec<(String, i32, i64)> = pending
        .iter()
        .map(|((date, key_id), count)| (date.clone(), *key_id, *count))
        .collect();
    db.increment_key_usage(device_id, &entries)
        .map(|_| pending.clear())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn continuous_input_still_flushes_every_five_seconds() {
        let directory =
            std::env::temp_dir().join(format!("stp-keyboard-aggregate-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("创建键盘聚合测试目录");
        let db = Arc::new(AppDb::open(&directory).expect("打开键盘聚合测试数据库"));
        let device = "aggregate-test".to_string();
        let (sender, receiver) = mpsc::channel();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let worker = std::thread::spawn({
            let db = Arc::clone(&db);
            let device = device.clone();
            let pending = Arc::clone(&pending);
            move || aggregate_loop(receiver, db, device, pending)
        });
        for _ in 0..30 {
            sender.send(0x04).expect("发送连续按键");
            std::thread::sleep(Duration::from_millis(200));
        }
        let today = Local::now().format("%Y-%m-%d").to_string();
        let device_filter = Some(device);
        let (daily, _, total) = db
            .keyboard_period(&today, &today, &device_filter)
            .expect("读取连续输入的周期落批结果");
        assert!(total > 0, "持续输入超过 5 秒也必须已经落库");
        assert_eq!(daily.iter().map(|day| day.count).sum::<i64>(), total);
        drop(sender);
        worker.join().expect("停止聚合线程");
        let _ = std::fs::remove_dir_all(directory);
    }
}
