//! 网络详情：原生链路缓存 + 用户可选的 HTTPS 质量探测。
//!
//! 原生接口信息最多每 10 秒刷新一次。HTTPS 探测只在 `snapshot()` 被调用且已到
//! 采样时间时派生单飞后台任务，IPC 本身不等待网络；悬浮条隐藏后没有调用也就
//! 没有新请求。失败只表示检测服务不可达，不等价于整台设备断网。

pub mod platform;

use platform::{Backend, LinkState, NativeBackend, NativeStatus};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROBE_URLS: &[&str] = &[
    "https://www.gstatic.com/generate_204",
    "https://connect.rom.miui.com/generate_204",
];
const PROBE_INTERVAL: Duration = Duration::from_secs(5);
const PROBE_FAILURE_INTERVAL: Duration = Duration::from_secs(15);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_STALE_AFTER: Duration = Duration::from_secs(15);
const PROBE_WINDOW: Duration = Duration::from_secs(300);
const NATIVE_CACHE_FOR: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InternetState {
    Disabled,
    Checking,
    Online,
    Unreachable,
    Stale,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkStatusOut {
    #[serde(flatten)]
    pub native: NativeStatus,
    pub internet: InternetState,
    pub latency_p50_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub failure_pct: Option<f64>,
    pub attempts: u32,
    pub successes: u32,
    pub sampled_at_ms: u64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct NetworkProbeConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy)]
struct Attempt {
    at: Instant,
    rtt_ms: Option<f64>,
    target: usize,
}

#[derive(Debug, Clone, Copy)]
struct ProbeResult {
    at: Instant,
    wall_ms: u64,
    success: bool,
}

#[derive(Default)]
struct ProbeInner {
    history: VecDeque<Attempt>,
    latest: Option<ProbeResult>,
    next_due: Option<Instant>,
}

#[derive(Default)]
struct ProbeState {
    enabled: AtomicBool,
    in_flight: AtomicBool,
    generation: AtomicU64,
    inner: Mutex<ProbeInner>,
}

#[derive(Debug, Clone, Copy)]
struct ProbeSnapshot {
    internet: InternetState,
    latency_p50_ms: Option<f64>,
    latency_p95_ms: Option<f64>,
    jitter_ms: Option<f64>,
    failure_pct: Option<f64>,
    attempts: u32,
    successes: u32,
    sampled_at_ms: u64,
}

fn resolve_internet_state(
    probed: InternetState,
    link: LinkState,
    probing_enabled: bool,
    client_available: bool,
) -> InternetState {
    if probing_enabled && (link == LinkState::Down || !client_available) {
        InternetState::Stale
    } else {
        probed
    }
}

impl ProbeState {
    fn finish(&self, generation: u64, success: bool, rtt_ms: f64, target: usize) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if generation == self.generation.load(Ordering::Acquire) {
            inner.latest = Some(ProbeResult {
                at: now,
                wall_ms: wall_now_ms(),
                success,
            });
            inner.history.push_back(Attempt {
                at: now,
                rtt_ms: success.then_some(rtt_ms),
                target,
            });
            let cutoff = now.checked_sub(PROBE_WINDOW);
            if let Some(cutoff) = cutoff {
                while inner
                    .history
                    .front()
                    .is_some_and(|attempt| attempt.at < cutoff)
                {
                    inner.history.pop_front();
                }
            }
            inner.next_due = Some(
                now + if success {
                    PROBE_INTERVAL
                } else {
                    PROBE_FAILURE_INTERVAL
                },
            );
        }
        self.in_flight.store(false, Ordering::Release);
    }

    fn snapshot(&self) -> ProbeSnapshot {
        let now = Instant::now();
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let attempts: Vec<Attempt> = inner
            .history
            .iter()
            .copied()
            .filter(|attempt| now.duration_since(attempt.at) <= PROBE_WINDOW)
            .collect();
        let successes: Vec<f64> = attempts
            .iter()
            .filter_map(|attempt| attempt.rtt_ms)
            .collect();
        let failure_pct = if attempts.is_empty() {
            None
        } else {
            Some((attempts.len() - successes.len()) as f64 * 100.0 / attempts.len() as f64)
        };
        let (latency_p50_ms, latency_p95_ms) = if successes.len() >= 5 {
            (percentile(&successes, 0.50), percentile(&successes, 0.95))
        } else {
            (None, None)
        };
        let sampled_at_ms = inner.latest.map_or(0, |result| result.wall_ms);
        let internet = match inner.latest {
            None if self.in_flight.load(Ordering::Acquire) => InternetState::Checking,
            None => InternetState::Stale,
            Some(result) if now.duration_since(result.at) > PROBE_STALE_AFTER => {
                if self.in_flight.load(Ordering::Acquire) {
                    InternetState::Checking
                } else {
                    InternetState::Stale
                }
            }
            Some(result) if result.success => InternetState::Online,
            Some(_) => InternetState::Unreachable,
        };
        ProbeSnapshot {
            internet,
            latency_p50_ms,
            latency_p95_ms,
            jitter_ms: jitter_from_attempts(&attempts),
            failure_pct,
            attempts: attempts.len() as u32,
            successes: successes.len() as u32,
            sampled_at_ms,
        }
    }
}

fn jitter_from_attempts(attempts: &[Attempt]) -> Option<f64> {
    let mut jitter: Option<f64> = None;
    let mut previous: Option<f64> = None;
    let mut previous_target = None;
    for attempt in attempts {
        if previous_target != Some(attempt.target) {
            previous = None;
            jitter = None;
            previous_target = Some(attempt.target);
        }
        if let Some(rtt_ms) = attempt.rtt_ms {
            if let Some(previous_rtt) = previous {
                let difference = (rtt_ms - previous_rtt).abs();
                jitter = Some(match jitter {
                    Some(current) => current + (difference - current) / 16.0,
                    None => difference,
                });
            }
            previous = Some(rtt_ms);
        } else {
            previous = None;
            jitter = None;
        }
    }
    jitter
}

fn percentile(values: &[f64], quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let index = ((quantile * sorted.len() as f64).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted.get(index).copied()
}

fn wall_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

/// 网络详情采样器；所有外部请求都在后台单飞执行，调用方只读缓存。
pub struct NetworkStatusSampler {
    native_cache: Mutex<Option<(Instant, NativeStatus)>>,
    probe: Arc<ProbeState>,
    client: Option<reqwest::Client>,
}

impl NetworkStatusSampler {
    /// 创建采样器；`enabled` 只控制主动质量探测，原生链路信息始终可用。
    pub fn new(enabled: bool) -> Self {
        let probe = Arc::new(ProbeState::default());
        probe.enabled.store(enabled, Ordering::Release);
        let client = reqwest::Client::builder()
            .user_agent("ScreenTimePro")
            .build()
            .ok();
        Self {
            native_cache: Mutex::new(None),
            probe,
            client,
        }
    }

    /// 返回质量探测配置，不包含端点或任何隐私数据。
    pub fn probe_config(&self) -> NetworkProbeConfig {
        NetworkProbeConfig {
            enabled: self.probe.enabled.load(Ordering::Acquire),
        }
    }

    /// 更新质量探测开关；相同值不重排采样，实际切换时使在途旧结果失效。
    pub fn set_probe_enabled(&self, enabled: bool) {
        let mut inner = self.probe.inner.lock().unwrap_or_else(|e| e.into_inner());
        if self.probe.enabled.load(Ordering::Acquire) == enabled {
            return;
        }
        self.probe.enabled.store(enabled, Ordering::Release);
        self.probe.generation.fetch_add(1, Ordering::AcqRel);
        inner.history.clear();
        inner.latest = None;
        inner.next_due = enabled.then(Instant::now);
    }

    fn native(&self) -> NativeStatus {
        let mut cache = self.native_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((sampled_at, status)) = *cache {
            if sampled_at.elapsed() < NATIVE_CACHE_FOR {
                return status;
            }
        }
        let status = Backend::snapshot();
        *cache = Some((Instant::now(), status));
        status
    }

    fn maybe_spawn_probe(&self, link: LinkState) {
        if link == LinkState::Down {
            return;
        }
        let (generation, client) = {
            let inner = self.probe.inner.lock().unwrap_or_else(|e| e.into_inner());
            let due = inner.next_due.map_or(true, |due| Instant::now() >= due);
            if !self.probe.enabled.load(Ordering::Acquire)
                || !due
                || self.probe.in_flight.load(Ordering::Acquire)
            {
                return;
            }
            self.probe.in_flight.store(true, Ordering::Release);
            (
                self.probe.generation.load(Ordering::Acquire),
                self.client.clone(),
            )
        };
        let Some(client) = client else {
            self.probe.in_flight.store(false, Ordering::Release);
            return;
        };
        let probe = Arc::clone(&self.probe);
        tauri::async_runtime::spawn(async move {
            let mut success = false;
            let mut rtt_ms = 0.0;
            let mut target = 0;
            for (index, url) in PROBE_URLS.iter().enumerate() {
                target = index;
                let started = Instant::now();
                let response = client.get(*url).timeout(PROBE_TIMEOUT).send().await;
                success = response
                    .map(|response| response.status().is_success())
                    .unwrap_or(false);
                if success {
                    rtt_ms = started.elapsed().as_secs_f64() * 1000.0;
                    break;
                }
            }
            probe.finish(generation, success, rtt_ms, target);
        });
    }

    /// 读取缓存并视采样到期情况派生后台探测，不等待任何网络请求。
    pub fn snapshot(&self) -> NetworkStatusOut {
        let native = self.native();
        self.maybe_spawn_probe(native.link);
        let probing_enabled = self.probe.enabled.load(Ordering::Acquire);
        let mut probe = if probing_enabled {
            self.probe.snapshot()
        } else {
            ProbeSnapshot {
                internet: InternetState::Disabled,
                latency_p50_ms: None,
                latency_p95_ms: None,
                jitter_ms: None,
                failure_pct: None,
                attempts: 0,
                successes: 0,
                sampled_at_ms: 0,
            }
        };
        probe.internet = resolve_internet_state(
            probe.internet,
            native.link,
            probing_enabled,
            self.client.is_some(),
        );
        NetworkStatusOut {
            native,
            internet: probe.internet,
            latency_p50_ms: probe.latency_p50_ms,
            latency_p95_ms: probe.latency_p95_ms,
            jitter_ms: probe.jitter_ms,
            failure_pct: probe.failure_pct,
            attempts: probe.attempts,
            successes: probe.successes,
            sampled_at_ms: probe.sampled_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn succeed(probe: &ProbeState, rtt_ms: f64) {
        probe.finish(probe.generation.load(Ordering::Acquire), true, rtt_ms, 0);
    }

    #[test]
    fn percentiles_use_nearest_rank_after_five_successes() {
        let probe = ProbeState::default();
        for value in [50.0, 10.0, 40.0, 20.0, 30.0] {
            succeed(&probe, value);
        }
        let snapshot = probe.snapshot();
        assert_eq!(snapshot.latency_p50_ms, Some(30.0));
        assert_eq!(snapshot.latency_p95_ms, Some(50.0));
        assert_eq!(snapshot.failure_pct, Some(0.0));
        assert_eq!(snapshot.internet, InternetState::Online);
    }

    #[test]
    fn latency_warmup_and_failure_ratio_are_explicit() {
        let probe = ProbeState::default();
        for value in [10.0, 20.0, 30.0, 40.0] {
            succeed(&probe, value);
        }
        probe.finish(probe.generation.load(Ordering::Acquire), false, 3_000.0, 0);
        let snapshot = probe.snapshot();
        assert_eq!(snapshot.latency_p50_ms, None);
        assert_eq!(snapshot.latency_p95_ms, None);
        assert_eq!(snapshot.failure_pct, Some(20.0));
        assert_eq!(snapshot.internet, InternetState::Unreachable);
        assert_eq!(snapshot.jitter_ms, None);
    }

    #[test]
    fn jitter_uses_ewma_and_resets_after_failure() {
        let probe = ProbeState::default();
        succeed(&probe, 100.0);
        succeed(&probe, 120.0);
        assert_eq!(probe.snapshot().jitter_ms, Some(20.0));
        succeed(&probe, 80.0);
        let jitter = probe.snapshot().jitter_ms.unwrap_or(-1.0);
        assert!((jitter - 21.25).abs() < 0.001);
        probe.finish(probe.generation.load(Ordering::Acquire), false, 3_000.0, 0);
        assert_eq!(probe.snapshot().jitter_ms, None);
    }

    #[test]
    fn changing_probe_target_resets_jitter() {
        let probe = ProbeState::default();
        succeed(&probe, 100.0);
        succeed(&probe, 120.0);
        assert_eq!(probe.snapshot().jitter_ms, Some(20.0));
        probe.finish(probe.generation.load(Ordering::Acquire), true, 50.0, 1);
        assert_eq!(probe.snapshot().jitter_ms, None);
        probe.finish(probe.generation.load(Ordering::Acquire), true, 70.0, 1);
        assert_eq!(probe.snapshot().jitter_ms, Some(20.0));
    }

    #[test]
    fn attempts_outside_five_minutes_do_not_affect_statistics() {
        let probe = ProbeState::default();
        probe
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .history
            .push_back(Attempt {
                at: Instant::now() - Duration::from_secs(301),
                rtt_ms: None,
                target: 0,
            });
        succeed(&probe, 25.0);
        let snapshot = probe.snapshot();
        assert_eq!(snapshot.attempts, 1);
        assert_eq!(snapshot.successes, 1);
        assert_eq!(snapshot.failure_pct, Some(0.0));
    }

    #[test]
    fn disabled_sampler_never_starts_http_work() {
        let sampler = NetworkStatusSampler::new(false);
        let status = sampler.snapshot();
        assert_eq!(status.internet, InternetState::Disabled);
        assert!(!sampler.probe.in_flight.load(Ordering::Acquire));
        assert_eq!(status.attempts, 0);
    }

    #[test]
    fn down_and_missing_client_only_override_enabled_probes() {
        for client_available in [false, true] {
            assert_eq!(
                resolve_internet_state(
                    InternetState::Disabled,
                    LinkState::Down,
                    false,
                    client_available,
                ),
                InternetState::Disabled
            );
        }
        assert_eq!(
            resolve_internet_state(InternetState::Disabled, LinkState::Up, false, false),
            InternetState::Disabled
        );
        assert_eq!(
            resolve_internet_state(InternetState::Online, LinkState::Down, true, true),
            InternetState::Stale
        );
        assert_eq!(
            resolve_internet_state(InternetState::Online, LinkState::Up, true, false),
            InternetState::Stale
        );
        assert_eq!(
            resolve_internet_state(InternetState::Online, LinkState::Up, true, true),
            InternetState::Online
        );
    }

    #[test]
    fn unchanged_config_preserves_samples_and_toggle_discards_old_completion() {
        let sampler = NetworkStatusSampler::new(true);
        succeed(&sampler.probe, 40.0);
        let generation = sampler.probe.generation.load(Ordering::Acquire);
        sampler.set_probe_enabled(true);
        assert_eq!(
            sampler
                .probe
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .history
                .len(),
            1
        );
        assert_eq!(sampler.probe.generation.load(Ordering::Acquire), generation);

        sampler.set_probe_enabled(false);
        sampler.set_probe_enabled(true);
        sampler.probe.finish(generation, true, 200.0, 0);
        let inner = sampler
            .probe
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(inner.history.is_empty());
        assert!(inner.latest.is_none());
    }

    #[test]
    fn no_inflight_and_no_result_is_not_reported_as_checking() {
        let probe = ProbeState::default();
        assert_eq!(probe.snapshot().internet, InternetState::Stale);
    }

    #[test]
    fn expired_rtt_does_not_seed_new_jitter() {
        let probe = ProbeState::default();
        {
            let mut inner = probe.inner.lock().unwrap_or_else(|e| e.into_inner());
            for rtt_ms in [100.0, 120.0] {
                inner.history.push_back(Attempt {
                    at: Instant::now() - Duration::from_secs(301),
                    rtt_ms: Some(rtt_ms),
                    target: 0,
                });
            }
        }
        succeed(&probe, 25.0);
        assert_eq!(probe.snapshot().jitter_ms, None);
    }

    #[test]
    fn missing_http_client_is_a_safe_stale_state() {
        let mut sampler = NetworkStatusSampler::new(true);
        sampler.client = None;
        assert_eq!(sampler.snapshot().internet, InternetState::Stale);
    }

    #[test]
    fn serialized_status_matches_frontend_contract() {
        let sampler = NetworkStatusSampler::new(false);
        let value = serde_json::to_value(sampler.snapshot()).expect("序列化网络状态");
        assert_eq!(value["link"], "unknown");
        assert_eq!(value["medium"], "unknown");
        assert_eq!(value["band"], "unknown");
        assert_eq!(value["vpn"], false);
        assert_eq!(value["internet"], "disabled");
        assert!(value["latency_p50_ms"].is_null());
        assert!(value["jitter_ms"].is_null());
    }

    /// 人工实时探针：只验证后台请求完成，不要求公网服务始终可达。
    #[test]
    #[ignore]
    fn live_probe_completes_in_background() {
        tauri::async_runtime::block_on(async {
            let sampler = NetworkStatusSampler::new(true);
            let _ = sampler.snapshot();
            for _ in 0..80 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let status = sampler.snapshot();
                if status.attempts > 0 {
                    println!("{status:?}");
                    return;
                }
            }
            panic!("后台 HTTPS 探测在 8 秒内没有完成");
        });
    }

    #[test]
    fn stale_result_is_not_reported_as_current_online() {
        let probe = ProbeState::default();
        probe.inner.lock().unwrap_or_else(|e| e.into_inner()).latest = Some(ProbeResult {
            at: Instant::now() - PROBE_STALE_AFTER - Duration::from_secs(1),
            wall_ms: 123,
            success: true,
        });
        assert_eq!(probe.snapshot().internet, InternetState::Stale);
    }
}
