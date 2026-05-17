use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::{mpsc, watch};

use super::ErrorKind;
use crate::sync::{Clock, TokioClock};

const DEFAULT_PROBE_CADENCE: Duration = Duration::from_secs(15);

pub trait NetProbe: Send + Sync {
    fn head<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<StatusCode>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct ReqwestNetProbe {
    client: reqwest::Client,
}

impl ReqwestNetProbe {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for ReqwestNetProbe {
    fn default() -> Self {
        Self::new(reqwest::Client::new())
    }
}

impl NetProbe for ReqwestNetProbe {
    fn head<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<StatusCode>> + Send + 'a>> {
        Box::pin(async move {
            let response = self.client.head(url).send().await?;
            Ok(response.status())
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NetState {
    #[default]
    Online,
    Offline {
        error_kind: ErrorKind,
    },
}

#[derive(Clone)]
pub struct NetworkMonitor {
    state_tx: watch::Sender<NetState>,
    command_tx: mpsc::Sender<MonitorCommand>,
}

impl NetworkMonitor {
    pub fn start(probe_url: impl Into<String>) -> Self {
        Self::start_with_probe_and_clock(
            probe_url.into(),
            Arc::new(ReqwestNetProbe::default()),
            Arc::new(TokioClock),
            DEFAULT_PROBE_CADENCE,
        )
    }

    pub fn start_with_probe(probe_url: impl Into<String>, probe: Arc<dyn NetProbe>) -> Self {
        Self::start_with_probe_and_clock(
            probe_url.into(),
            probe,
            Arc::new(TokioClock),
            DEFAULT_PROBE_CADENCE,
        )
    }

    pub fn start_with_probe_and_clock(
        probe_url: String,
        probe: Arc<dyn NetProbe>,
        clock: Arc<dyn Clock>,
        cadence: Duration,
    ) -> Self {
        let (state_tx, _) = watch::channel(NetState::Online);
        let (command_tx, command_rx) = mpsc::channel::<MonitorCommand>(128);
        tokio::spawn(run_monitor(
            probe_url,
            probe,
            clock,
            cadence,
            state_tx.clone(),
            command_rx,
        ));
        Self {
            state_tx,
            command_tx,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<NetState> {
        self.state_tx.subscribe()
    }

    pub fn state(&self) -> NetState {
        self.state_tx.borrow().clone()
    }

    pub async fn report_api_error(&self, error_kind: ErrorKind) {
        let _ = self
            .command_tx
            .send(MonitorCommand::ApiError { error_kind })
            .await;
    }

    pub async fn traffic_started(&self) {
        let _ = self.command_tx.send(MonitorCommand::TrafficStarted).await;
    }

    pub async fn traffic_finished(&self) {
        let _ = self.command_tx.send(MonitorCommand::TrafficFinished).await;
    }

    pub async fn probe_now(&self) {
        let _ = self.command_tx.send(MonitorCommand::ProbeNow).await;
    }
}

#[derive(Debug)]
enum MonitorCommand {
    ApiError { error_kind: ErrorKind },
    TrafficStarted,
    TrafficFinished,
    ProbeNow,
}

async fn run_monitor(
    probe_url: String,
    probe: Arc<dyn NetProbe>,
    clock: Arc<dyn Clock>,
    cadence: Duration,
    state_tx: watch::Sender<NetState>,
    mut command_rx: mpsc::Receiver<MonitorCommand>,
) {
    let mut traffic_in_flight = 0_u64;
    loop {
        let sleep = clock.sleep(cadence);
        tokio::pin!(sleep);

        tokio::select! {
            _ = &mut sleep => {
                if traffic_in_flight == 0 {
                    apply_probe_result(&state_tx, probe.head(&probe_url).await);
                }
            }
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                match command {
                    MonitorCommand::ApiError { error_kind } => {
                        set_state_if_changed(
                            &state_tx,
                            NetState::Offline { error_kind },
                        );
                    }
                    MonitorCommand::TrafficStarted => {
                        traffic_in_flight = traffic_in_flight.saturating_add(1);
                    }
                    MonitorCommand::TrafficFinished => {
                        traffic_in_flight = traffic_in_flight.saturating_sub(1);
                    }
                    MonitorCommand::ProbeNow => {
                        if traffic_in_flight == 0 {
                            apply_probe_result(&state_tx, probe.head(&probe_url).await);
                        }
                    }
                }
            }
        }
    }
}

fn apply_probe_result(state_tx: &watch::Sender<NetState>, result: Result<StatusCode>) {
    match result {
        Ok(status) if status == StatusCode::OK => {
            if matches!(*state_tx.borrow(), NetState::Offline { .. }) {
                set_state_if_changed(state_tx, NetState::Online);
            }
        }
        Ok(status) => {
            set_state_if_changed(
                state_tx,
                NetState::Offline {
                    error_kind: error_kind_from_status(status),
                },
            );
        }
        Err(_) => {
            set_state_if_changed(
                state_tx,
                NetState::Offline {
                    error_kind: ErrorKind::Network,
                },
            );
        }
    }
}

fn set_state_if_changed(state_tx: &watch::Sender<NetState>, next: NetState) {
    if *state_tx.borrow() != next {
        let _ = state_tx.send(next);
    }
}

pub fn error_kind_from_status(status: StatusCode) -> ErrorKind {
    match status.as_u16() {
        401 | 403 => ErrorKind::Auth,
        404 => ErrorKind::NotFound,
        409 | 422 => ErrorKind::Conflict,
        429 => ErrorKind::RateLimited,
        400..=499 => ErrorKind::Other(format!("http_{}", status.as_u16())),
        _ => ErrorKind::Server,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    #[derive(Default)]
    struct QueueProbe {
        queue: Mutex<VecDeque<Result<StatusCode>>>,
    }

    impl QueueProbe {
        fn push(&self, result: Result<StatusCode>) {
            self.queue
                .lock()
                .expect("probe queue lock poisoned")
                .push_back(result);
        }
    }

    impl NetProbe for QueueProbe {
        fn head<'a>(
            &'a self,
            _url: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<StatusCode>> + Send + 'a>> {
            Box::pin(async move {
                self.queue
                    .lock()
                    .expect("probe queue lock poisoned")
                    .pop_front()
                    .unwrap_or(Ok(StatusCode::OK))
            })
        }
    }

    #[derive(Default)]
    struct PendingClock;

    impl Clock for PendingClock {
        fn sleep<'a>(
            &'a self,
            _duration: Duration,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(std::future::pending())
        }
    }

    #[tokio::test]
    async fn probe_transitions_offline_to_online() {
        let probe = Arc::new(QueueProbe::default());
        probe.push(Ok(StatusCode::SERVICE_UNAVAILABLE));
        probe.push(Ok(StatusCode::OK));
        let monitor = NetworkMonitor::start_with_probe_and_clock(
            "http://localhost:1234".to_string(),
            probe,
            Arc::new(PendingClock),
            Duration::from_secs(15),
        );
        let mut rx = monitor.subscribe();

        monitor.probe_now().await;
        rx.changed().await.expect("offline transition");
        assert!(matches!(
            *rx.borrow(),
            NetState::Offline {
                error_kind: ErrorKind::Server
            }
        ));

        monitor.probe_now().await;
        rx.changed().await.expect("online transition");
        assert_eq!(*rx.borrow(), NetState::Online);
    }

    #[tokio::test]
    async fn api_error_marks_offline_immediately() {
        let probe = Arc::new(QueueProbe::default());
        let monitor = NetworkMonitor::start_with_probe_and_clock(
            "http://localhost:1234".to_string(),
            probe,
            Arc::new(PendingClock),
            Duration::from_secs(15),
        );
        let mut rx = monitor.subscribe();
        monitor.report_api_error(ErrorKind::Conflict).await;
        rx.changed().await.expect("offline transition");
        assert!(matches!(
            *rx.borrow(),
            NetState::Offline {
                error_kind: ErrorKind::Conflict
            }
        ));
    }
}
