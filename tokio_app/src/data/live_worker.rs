//! Фоновый поток: COM → sync → DSP @ 200 Hz.
//! UI только забирает готовые сэмплы (как Java SerialService + Timeline).

use crate::com_port::decode::decode_frame;
use crate::com_port::serial::SerialConfig;
use crate::com_port::sync::FrameSynchronizer;
use crate::data::rcm_pipeline::{
    ChannelFilterFlags, RcmOutSample, RcmPipeline, RcmProfile, SAMPLE_RATE_HZ,
};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct LivePoint {
    pub time_seconds: f64,
    pub rheo1: f64,
    pub base1: f64,
    pub ecg: f64,
    pub base2: f64,
    pub rheo2: f64,
    pub qs1: f64,
    pub qs2: f64,
}

pub enum LiveEvent {
    Batch(Vec<LivePoint>),
    Error(String),
}

enum LiveCmd {
    Stop,
    SetChannelFilters(ChannelFilterFlags),
    SetProfile(RcmProfile),
}

pub struct LiveWorker {
    cmd_tx: Sender<LiveCmd>,
    event_rx: Receiver<LiveEvent>,
    join: Option<JoinHandle<()>>,
}

impl LiveWorker {
    pub fn start(
        port_name: String,
        baud_rate: u32,
        profile: RcmProfile,
        flags: ChannelFilterFlags,
    ) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        let join = thread::Builder::new()
            .name("rcm-live".into())
            .spawn(move || {
                worker_loop(port_name, baud_rate, profile, flags, cmd_rx, event_tx);
            })
            .map_err(|e| format!("не запустить live-поток: {e}"))?;

        Ok(Self {
            cmd_tx,
            event_rx,
            join: Some(join),
        })
    }

    pub fn stop(mut self) {
        let _ = self.cmd_tx.send(LiveCmd::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }

    pub fn set_channel_filters(&self, flags: ChannelFilterFlags) {
        let _ = self.cmd_tx.send(LiveCmd::SetChannelFilters(flags));
    }

    pub fn set_profile(&self, profile: RcmProfile) {
        let _ = self.cmd_tx.send(LiveCmd::SetProfile(profile));
    }

    pub fn poll(&self) -> Vec<LiveEvent> {
        let mut out = Vec::new();
        loop {
            match self.event_rx.try_recv() {
                Ok(ev) => out.push(ev),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    out.push(LiveEvent::Error("live-поток завершился".into()));
                    break;
                }
            }
        }
        out
    }
}

impl Drop for LiveWorker {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(LiveCmd::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn worker_loop(
    port_name: String,
    baud_rate: u32,
    profile: RcmProfile,
    flags: ChannelFilterFlags,
    cmd_rx: Receiver<LiveCmd>,
    event_tx: Sender<LiveEvent>,
) {
    let cfg = SerialConfig::rcm_7n1(baud_rate);
    let mut port = match cfg.open(&port_name) {
        Ok(p) => p,
        Err(e) => {
            let _ = event_tx.send(LiveEvent::Error(format!("не открыть {port_name}: {e}")));
            return;
        }
    };

    let mut trash = [0u8; 512];
    for _ in 0..8 {
        match port.read(&mut trash) {
            Ok(0) | Err(_) => break,
            Ok(_) => continue,
        }
    }

    let mut sync = if profile == RcmProfile::Rcms {
        FrameSynchronizer::new_rcms()
    } else {
        FrameSynchronizer::new()
    };
    let mut pipeline = RcmPipeline::new_with_flags(profile, flags);
    let mut sample_index: u64 = 0;
    let mut batch = Vec::with_capacity(64);
    let mut last_flush = Instant::now();
    let mut buf = [0u8; 1024];

    loop {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                LiveCmd::Stop => {
                    flush_batch(&event_tx, &mut batch);
                    return;
                }
                LiveCmd::SetChannelFilters(f) => {
                    pipeline.set_flags(f);
                    sample_index = 0;
                }
                LiveCmd::SetProfile(p) => {
                    pipeline.set_profile(p);
                    sync.set_rcms(p == RcmProfile::Rcms);
                    sync.clear();
                    sample_index = 0;
                }
            }
        }

        match port.read(&mut buf) {
            Ok(n) if n > 0 => {
                let frames = sync.push_bytes(&buf[..n]);
                for raw in frames {
                    for s in pipeline.process_raw(&raw) {
                        let t = sample_index as f64 / SAMPLE_RATE_HZ;
                        sample_index += 1;
                        batch.push(to_point(t, &s));
                    }
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                let _ = event_tx.send(LiveEvent::Error(format!("чтение COM: {e}")));
                return;
            }
        }

        if !batch.is_empty() && last_flush.elapsed() >= Duration::from_millis(25) {
            flush_batch(&event_tx, &mut batch);
            last_flush = Instant::now();
        }

        if batch.is_empty() {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

fn flush_batch(tx: &Sender<LiveEvent>, batch: &mut Vec<LivePoint>) {
    if batch.is_empty() {
        return;
    }
    let mut take = Vec::new();
    std::mem::swap(&mut take, batch);
    let _ = tx.send(LiveEvent::Batch(take));
}

fn to_point(time_seconds: f64, s: &RcmOutSample) -> LivePoint {
    // РЕО: INVERSE для Ohm и для сырого signed → как decode_frame bipolar.
    LivePoint {
        time_seconds,
        rheo1: -(s.rheo1 as f64) / 1000.0, // В мОм
        base1: s.base1 as f64,
        ecg: s.ecg as f64 / 1000.0, //В В
        base2: s.base2 as f64, 
        rheo2: -(s.rheo2 as f64) / 1000.0, // В мОм
        qs1: s.qs1 as f64,
        qs2: s.qs2 as f64,
    }
}

#[allow(dead_code)]
fn raw_point(time_seconds: f64, raw: &[u8; 20]) -> LivePoint {
    let f = decode_frame(raw);
    LivePoint {
        time_seconds,
        rheo1: f.rheo1 as f64,
        base1: f.base1 as f64,
        ecg: f.ecg as f64,
        base2: f.base2 as f64,
        rheo2: f.rheo2 as f64,
        qs1: 0.0,
        qs2: 0.0,
    }
}
