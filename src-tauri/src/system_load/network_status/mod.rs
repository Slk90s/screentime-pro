//! 网络详情：原生链路缓存 + 用户可选的 HTTPS 质量探测。
//!
//! 原生接口信息最多每 10 秒刷新一次。HTTPS 探测只在 `snapshot()` 被调用且已到
//! 采样时间时派生单飞后台任务，IPC 本身不等待网络；悬浮条隐藏后没有调用也就
//! 没有新请求。失败只表示检测服务不可达，不等价于整台设备断网。

pub mod platform;

use platform::{Backend, LinkState, NativeBackend, NativeStatus};
use std::collections::VecDeque;
use std::net::{IpAddr, UdpSocket};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROBE_URLS: &[&str] = &[
    "https://www.gstatic.com/generate_204",
    "https://connect.rom.miui.com/generate_204",
];
const PROBE_WARMUP_INTERVAL: Duration = Duration::from_secs(2);
const PROBE_INTERVAL: Duration = Duration::from_secs(10);
const PROBE_FAILURE_INTERVAL: Duration = Duration::from_secs(15);
const PROBE_MAX_FAILURE_INTERVAL: Duration = Duration::from_secs(60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const PUBLIC_INFO_URL: &str = "https://ipinfo.io/json";
const PUBLIC_INFO_INTERVAL: Duration = Duration::from_secs(300);
const PUBLIC_INFO_FAILURE_INTERVAL: Duration = Duration::from_secs(60);
const PACKET_LOSS_INTERVAL: Duration = Duration::from_secs(15);
const PACKET_LOSS_FAILURE_INTERVAL: Duration = Duration::from_secs(60);
const PACKET_LOSS_HOST: &str = "1.1.1.1";
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
    /// 仅在用户单独开启公网信息查询时返回；默认 null。
    pub public_ip: Option<String>,
    pub region: Option<String>,
    /// 系统 ping 实测丢包率；不可用或权限不足时为 null。
    pub packet_loss_pct: Option<f64>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct NetworkProbeConfig {
    pub enabled: bool,
    /// 公网 IP/地区查询独立开关，与质量探测分开授权。
    pub public_ip_enabled: bool,
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
    consecutive_failures: u32,
}

#[derive(Default)]
struct ProbeState {
    enabled: AtomicBool,
    in_flight: AtomicBool,
    generation: AtomicU64,
    inner: Mutex<ProbeInner>,
}

#[derive(Default)]
struct PublicInfoInner {
    next_due: Option<Instant>,
    public_ip: Option<String>,
    region: Option<String>,
}

#[derive(Default)]
struct PublicInfoState {
    enabled: AtomicBool,
    in_flight: AtomicBool,
    generation: AtomicU64,
    inner: Mutex<PublicInfoInner>,
}

#[derive(Default)]
struct PacketLossInner {
    next_due: Option<Instant>,
    value: Option<f64>,
}

#[derive(Default)]
struct PacketLossState {
    in_flight: AtomicBool,
    inner: Mutex<PacketLossInner>,
}

#[derive(serde::Deserialize)]
struct PublicInfoResponse {
    ip: String,
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
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
            let interval = if success {
                inner.consecutive_failures = 0;
                if inner.history.len() < 5 {
                    PROBE_WARMUP_INTERVAL
                } else {
                    PROBE_INTERVAL
                }
            } else {
                inner.consecutive_failures = inner.consecutive_failures.saturating_add(1).min(4);
                let exponent = inner.consecutive_failures.saturating_sub(1);
                (PROBE_FAILURE_INTERVAL * (1u32 << exponent)).min(PROBE_MAX_FAILURE_INTERVAL)
            };
            inner.next_due = Some(now + interval);
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

fn local_ip_address() -> Option<String> {
    for (bind, destination) in [("0.0.0.0:0", "1.1.1.1:80"), ("[::]:0", "[2001:db8::1]:80")] {
        let Ok(socket) = UdpSocket::bind(bind) else {
            continue;
        };
        if socket.connect(destination).is_ok() {
            if let Ok(address) = socket.local_addr() {
                return Some(address.ip().to_string());
            }
        }
    }
    None
}

fn region_from_response(response: PublicInfoResponse) -> Option<String> {
    let locality = response.region.or(response.city);
    let text = match (locality, response.country) {
        (Some(locality), Some(country)) => format!("{locality}, {country}"),
        (Some(locality), None) => locality,
        (None, Some(country)) => country,
        (None, None) => return None,
    };
    Some(text.chars().take(96).collect())
}

impl PublicInfoState {
    fn finish(&self, generation: u64, response: Option<PublicInfoResponse>) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if generation == self.generation.load(Ordering::Acquire) {
            match response {
                Some(response) => {
                    inner.public_ip = response
                        .ip
                        .parse::<IpAddr>()
                        .ok()
                        .map(|address| address.to_string());
                    inner.region = region_from_response(response);
                }
                None => {
                    inner.public_ip = None;
                    inner.region = None;
                }
            }
            inner.next_due = Some(
                now + if inner.public_ip.is_some() {
                    PUBLIC_INFO_INTERVAL
                } else {
                    PUBLIC_INFO_FAILURE_INTERVAL
                },
            );
        }
        self.in_flight.store(false, Ordering::Release);
    }

    fn snapshot(&self) -> (Option<String>, Option<String>) {
        if !self.enabled.load(Ordering::Acquire) {
            return (None, None);
        }
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        (inner.public_ip.clone(), inner.region.clone())
    }
}

impl PacketLossState {
    fn finish(&self, value: Option<f64>) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.value = value;
        inner.next_due = Some(
            now + if value.is_some() {
                PACKET_LOSS_INTERVAL
            } else {
                PACKET_LOSS_FAILURE_INTERVAL
            },
        );
        self.in_flight.store(false, Ordering::Release);
    }

    fn snapshot(&self) -> Option<f64> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).value
    }
}

fn loss_from_ping_output(input: &str) -> Option<f64> {
    let received = input
        .lines()
        .filter(|line| line.to_lowercase().contains("ttl="))
        .count();
    if received > 4 {
        return None;
    }
    Some((4 - received) as f64 * 25.0)
}

fn measure_packet_loss() -> Option<f64> {
    #[cfg(target_os = "windows")]
    let args = ["-n", "4", "-w", "1000", PACKET_LOSS_HOST];
    #[cfg(target_os = "macos")]
    let args = ["-c", "4", "-W", "1000", PACKET_LOSS_HOST];
    #[cfg(target_os = "linux")]
    let args = ["-c", "4", "-W", "1", PACKET_LOSS_HOST];
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return None;
    let output = Command::new("ping").args(args).output().ok()?;
    loss_from_ping_output(&String::from_utf8_lossy(&output.stdout))
}

/// 网络详情采样器；所有外部请求都在后台单飞执行，调用方只读缓存。
pub struct NetworkStatusSampler {
    native_cache: Mutex<Option<(Instant, NativeStatus)>>,
    probe: Arc<ProbeState>,
    public_info: Arc<PublicInfoState>,
    packet_loss: Arc<PacketLossState>,
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
            public_info: Arc::new(PublicInfoState::default()),
            packet_loss: Arc::new(PacketLossState::default()),
            client,
        }
    }

    /// 返回质量探测配置，不包含端点或任何隐私数据。
    pub fn probe_config(&self) -> NetworkProbeConfig {
        NetworkProbeConfig {
            enabled: self.probe.enabled.load(Ordering::Acquire),
            public_ip_enabled: self.public_info.enabled.load(Ordering::Acquire),
        }
    }

    /// 独立设置公网 IP/地区查询；相同值不清空缓存，实际切换时丢弃旧请求结果。
    pub fn set_public_ip_enabled(&self, enabled: bool) {
        let mut inner = self
            .public_info
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if self.public_info.enabled.load(Ordering::Acquire) == enabled {
            return;
        }
        self.public_info.enabled.store(enabled, Ordering::Release);
        self.public_info.generation.fetch_add(1, Ordering::AcqRel);
        inner.public_ip = None;
        inner.region = None;
        inner.next_due = enabled.then(Instant::now);
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
        inner.consecutive_failures = 0;
        inner.next_due = enabled.then(Instant::now);
        let mut packet_loss = self
            .packet_loss
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        packet_loss.value = None;
        packet_loss.next_due = enabled.then(Instant::now);
    }

    fn native(&self) -> NativeStatus {
        let mut cache = self.native_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((sampled_at, status)) = cache.as_ref() {
            if sampled_at.elapsed() < NATIVE_CACHE_FOR {
                return status.clone();
            }
        }
        let mut status = Backend::snapshot();
        status.local_ip = local_ip_address();
        *cache = Some((Instant::now(), status.clone()));
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

    fn maybe_spawn_public_info(&self, link: LinkState) {
        if link == LinkState::Down {
            return;
        }
        let (generation, client) = {
            let inner = self
                .public_info
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let due = inner.next_due.map_or(true, |due| Instant::now() >= due);
            if !self.public_info.enabled.load(Ordering::Acquire)
                || !due
                || self.public_info.in_flight.load(Ordering::Acquire)
            {
                return;
            }
            self.public_info.in_flight.store(true, Ordering::Release);
            (
                self.public_info.generation.load(Ordering::Acquire),
                self.client.clone(),
            )
        };
        let Some(client) = client else {
            self.public_info.in_flight.store(false, Ordering::Release);
            return;
        };
        let public_info = Arc::clone(&self.public_info);
        tauri::async_runtime::spawn(async move {
            let response = client
                .get(PUBLIC_INFO_URL)
                .timeout(PROBE_TIMEOUT)
                .send()
                .await;
            let parsed = match response {
                Ok(response) if response.status().is_success() => {
                    response.json::<PublicInfoResponse>().await.ok()
                }
                _ => None,
            };
            public_info.finish(generation, parsed);
        });
    }

    fn maybe_spawn_packet_loss(&self, link: LinkState) {
        if link == LinkState::Down || !self.probe.enabled.load(Ordering::Acquire) {
            return;
        }
        {
            let mut inner = self
                .packet_loss
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let due = inner.next_due.map_or(true, |due| Instant::now() >= due);
            if !due || self.packet_loss.in_flight.load(Ordering::Acquire) {
                return;
            }
            self.packet_loss.in_flight.store(true, Ordering::Release);
            inner.next_due = Some(Instant::now() + PACKET_LOSS_INTERVAL);
        }
        let packet_loss = Arc::clone(&self.packet_loss);
        tauri::async_runtime::spawn_blocking(move || {
            packet_loss.finish(measure_packet_loss());
        });
    }

    /// 读取缓存并视采样到期情况派生后台探测，不等待任何网络请求。
    pub fn snapshot(&self) -> NetworkStatusOut {
        let native = self.native();
        self.maybe_spawn_probe(native.link);
        self.maybe_spawn_public_info(native.link);
        self.maybe_spawn_packet_loss(native.link);
        let (public_ip, region) = self.public_info.snapshot();
        let packet_loss_pct = self
            .probe
            .enabled
            .load(Ordering::Acquire)
            .then(|| self.packet_loss.snapshot())
            .flatten();
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
            public_ip,
            region,
            packet_loss_pct,
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

    fn next_delay_secs(probe: &ProbeState) -> u64 {
        probe
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .next_due
            .map(|due| due.duration_since(Instant::now()).as_secs())
            .unwrap_or(u64::MAX)
    }

    #[test]
    fn schedule_warms_up_quickly_and_backs_off_to_one_minute() {
        let probe = ProbeState::default();
        for _ in 0..4 {
            succeed(&probe, 20.0);
            assert!(next_delay_secs(&probe) <= 2);
        }
        succeed(&probe, 20.0);
        assert!(next_delay_secs(&probe) <= 10);
        probe.finish(probe.generation.load(Ordering::Acquire), false, 3_000.0, 0);
        assert!(next_delay_secs(&probe) <= 15);
        probe.finish(probe.generation.load(Ordering::Acquire), false, 3_000.0, 0);
        assert!(next_delay_secs(&probe) <= 30);
        probe.finish(probe.generation.load(Ordering::Acquire), false, 3_000.0, 0);
        assert!(next_delay_secs(&probe) <= 60);
        probe.finish(probe.generation.load(Ordering::Acquire), false, 3_000.0, 0);
        assert!(next_delay_secs(&probe) <= 60);
    }

    #[test]
    fn public_info_region_is_bounded_and_old_completion_is_discarded() {
        let response = PublicInfoResponse {
            ip: "203.0.113.8".to_string(),
            city: Some("Shanghai".to_string()),
            region: Some("Shanghai".to_string()),
            country: Some("CN".to_string()),
        };
        assert_eq!(
            region_from_response(response),
            Some("Shanghai, CN".to_string())
        );

        let sampler = NetworkStatusSampler::new(false);
        sampler.set_public_ip_enabled(true);
        let generation = sampler.public_info.generation.load(Ordering::Acquire);
        sampler.set_public_ip_enabled(false);
        sampler.public_info.finish(
            generation,
            Some(PublicInfoResponse {
                ip: "203.0.113.8".to_string(),
                city: None,
                region: None,
                country: Some("CN".to_string()),
            }),
        );
        assert_eq!(sampler.public_info.snapshot(), (None, None));
    }

    #[test]
    fn packet_loss_counts_ttl_replies_not_http_failures() {
        let windows = "来自 1.1.1.1 的回复: 字节=32 时间<1ms TTL=128\n来自 1.1.1.1 的回复: 字节=32 时间=2ms TTL=128";
        assert_eq!(loss_from_ping_output(windows), Some(50.0));
        let unix = "64 bytes from 1.1.1.1: icmp_seq=0 ttl=128\n64 bytes from 1.1.1.1: icmp_seq=1 ttl=128\n64 bytes from 1.1.1.1: icmp_seq=2 ttl=128\n64 bytes from 1.1.1.1: icmp_seq=3 ttl=128";
        assert_eq!(loss_from_ping_output(unix), Some(0.0));
        assert_eq!(loss_from_ping_output("Request timeout"), Some(100.0));
        assert_eq!(
            loss_from_ping_output("TTL=1\nTTL=1\nTTL=1\nTTL=1\nTTL=1"),
            None
        );
    }

    #[test]
    #[ignore]
    fn live_packet_loss_probe() {
        println!("packet_loss_pct={:?}", measure_packet_loss());
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
