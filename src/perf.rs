//! Frame-time ring buffer and one-shot timers, printed to stderr.
//! Frame samples come from the renderer's BeforeRendering/AfterRendering notifications.

use std::cell::RefCell;
use std::time::{Duration, Instant};

const RING: usize = 4096;

#[derive(Default)]
pub struct Perf {
    /// CPU time spent inside the render pass (BeforeRendering -> AfterRendering).
    render: Vec<Duration>,
    /// Wall time between consecutive rendered frames, only while frames are back to back
    /// (< 100 ms apart), i.e. during scrolling or streaming. Idle gaps are not frames.
    interval: Vec<Duration>,
    last_before: Option<Instant>,
    last_frame: Option<Instant>,
    frames: u64,
}

thread_local! {
    pub static PERF: RefCell<Perf> = RefCell::new(Perf::default());
}

pub fn before_rendering() {
    PERF.with(|p| {
        let mut p = p.borrow_mut();
        let now = Instant::now();
        if let Some(prev) = p.last_frame {
            let dt = now - prev;
            if dt < Duration::from_millis(100) {
                push(&mut p.interval, dt);
            }
        }
        p.last_frame = Some(now);
        p.last_before = Some(now);
    });
}

pub fn after_rendering() {
    PERF.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(b) = p.last_before.take() {
            let d = b.elapsed();
            push(&mut p.render, d);
            p.frames += 1;
        }
    });
}

fn push(v: &mut Vec<Duration>, d: Duration) {
    if v.len() >= RING {
        v.remove(0);
    }
    v.push(d);
}

fn pct(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

fn stats(v: &[Duration]) -> String {
    if v.is_empty() {
        return "n=0".into();
    }
    let mut s = v.to_vec();
    s.sort();
    format!(
        "n={} p50={:.2}ms p95={:.2}ms max={:.2}ms",
        s.len(),
        pct(&s, 0.50).as_secs_f64() * 1e3,
        pct(&s, 0.95).as_secs_f64() * 1e3,
        s[s.len() - 1].as_secs_f64() * 1e3
    )
}

/// Print and clear the ring buffers. Called from a 5 s slint::Timer.
pub fn report() {
    PERF.with(|p| {
        let mut p = p.borrow_mut();
        if p.render.is_empty() && p.interval.is_empty() {
            return;
        }
        eprintln!(
            "[perf] frames_total={} render[{}] interval[{}]",
            p.frames,
            stats(&p.render),
            stats(&p.interval)
        );
        p.render.clear();
        p.interval.clear();
    });
}

pub fn mark(label: &str) -> Instant {
    let now = Instant::now();
    eprintln!("[mark] {label}");
    now
}

pub fn elapsed_ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1e3
}
