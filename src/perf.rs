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
    frames_at_report: u64,
    cpu_at_report: f64,
}

thread_local! {
    pub static PERF: RefCell<Perf> = RefCell::new(Perf::default());
}

/// Which heap the process's default heap is (Rust's allocator uses it): the segment heap the
/// manifest asks for (`assets/twinfm.exe.manifest`, the Phase 12 memory follow-up), or the NT heap
/// Windows gives a program that does not ask. Read from the heap's header: both kinds keep a
/// signature at offset 0x10 (`_SEGMENT_HEAP.Signature`, `_HEAP.SegmentSignature`).
pub fn process_heap() -> String {
    // SAFETY: the process heap's header stays mapped and readable for the life of the process.
    let signature = unsafe {
        let heap = windows_sys::Win32::System::Memory::GetProcessHeap() as *const u8;
        std::ptr::read_volatile(heap.add(0x10) as *const u32)
    };
    match signature {
        0xDDEE_DDEE => "segment heap".into(),
        0xFFEE_FFEE => "NT heap".into(),
        s => format!("unknown (signature 0x{s:08X})"),
    }
}

/// The process's working set and private bytes, in MB (P13-M3: the sizes view's walk is held to
/// its memory). psapi's call through kernel32, declared here rather than enabling windows-sys's
/// ProcessStatus feature for one call.
pub fn memory_mb() -> (f64, f64) {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        quota: [usize; 4],
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: std::mem::size_of::<Counters>() as u32, ..Default::default() };
    // SAFETY: the current process's pseudo-handle (-1) and a PROCESS_MEMORY_COUNTERS_EX-shaped out
    // buffer of the size given.
    unsafe { K32GetProcessMemoryInfo(-1, &mut c, c.cb) };
    let mb = |b: usize| b as f64 / (1024.0 * 1024.0);
    (mb(c.working_set), mb(c.private))
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

/// Frames rendered so far (the counter `after_rendering` already keeps; read, never per frame).
pub fn frames() -> u64 {
    PERF.with(|p| p.try_borrow().map_or(0, |p| p.frames))
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
        let cpu_now = thread_cpu_secs();
        let dframes = p.frames - p.frames_at_report;
        let dcpu = cpu_now - p.cpu_at_report;
        p.frames_at_report = p.frames;
        p.cpu_at_report = cpu_now;
        eprintln!(
            "[perf] frames_total={} render[{}] interval[{}] ui_cpu={:.0}ms over 5s = {:.2}ms/frame",
            p.frames,
            stats(&p.render),
            stats(&p.interval),
            dcpu * 1e3,
            if dframes > 0 { dcpu * 1e3 / dframes as f64 } else { 0.0 }
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

/// CPU time (user + kernel) consumed so far by the calling thread, in seconds.
pub fn thread_cpu_secs() -> f64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut c, mut e, mut k, mut u) = (zero, zero, zero, zero);
    // SAFETY: plain Win32 call on the current thread with valid out-pointers.
    unsafe { GetThreadTimes(GetCurrentThread(), &mut c, &mut e, &mut k, &mut u) };
    let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64 * 100e-9;
    ft(k) + ft(u)
}

/// Report UI-thread busy % over a window: call with the start sample after `secs` seconds.
pub struct BusyWindow {
    label: String,
    cpu0: f64,
    wall0: Instant,
}

impl BusyWindow {
    pub fn start(label: &str) -> BusyWindow {
        BusyWindow { label: label.to_string(), cpu0: thread_cpu_secs(), wall0: Instant::now() }
    }
    pub fn finish(&self) {
        let cpu = thread_cpu_secs() - self.cpu0;
        let wall = self.wall0.elapsed().as_secs_f64();
        eprintln!("[busy] {}: UI thread {:.1}% busy over {:.1} s (cpu {:.3} s)", self.label, cpu / wall * 100.0, wall, cpu);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_process_runs_on_the_segment_heap() {
        // The manifest is linked into the test binaries too (winresource's link argument), so this
        // process asks for the segment heap as twinfm.exe does.
        assert_eq!(super::process_heap(), "segment heap");
    }
}
