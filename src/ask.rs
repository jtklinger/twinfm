//! TwinFM's own questions (P12-M3), drawn in the look by app.slint's question dialog in place of
//! Windows' message boxes and the hand-built password dialog: the replace question for a zip, the
//! permanent delete inside a zip, a zip's or a PDF's password, and "Open N items at once?".
//!
//! A worker asks with `ask`, which hands the question to the UI thread (`invoke_from_event_loop`)
//! and waits for the answer on a one-slot channel; the UI thread asks with `ask_here` and goes on in
//! the reply. One question shows at a time and the others wait in the order asked. Quitting answers
//! Cancel to every question shown or waiting, so no worker waits for ever, and in a scripted run a
//! question left `SCRIPT_CANCEL` is answered Cancel, so a script never hangs on one. A worker may
//! wait for the UI thread only through `ask`, and the UI thread never waits for a thread that can
//! call it. The password typed is cleared when the question closes and never written to a line.
//!
//! `[ask]` lines: `shown <kind> "<title>" (from <thread>, K waiting)`, `<kind> answered <button>
//! after T ms`, `<kind> unanswered after 30 s: Cancel (scripted run)`, `quit: K question(s) answered
//! Cancel`, `<kind> asked on the UI thread: Cancel` (a mistake that would otherwise hang it).

use crate::App;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::OnceLock;
use std::thread::ThreadId;
use std::time::{Duration, Instant};

/// A scripted run's question left this long is answered Cancel.
pub const SCRIPT_CANCEL: Duration = Duration::from_secs(30);

/// The dialog's button slots: the buttons sit in the last ones, right-aligned.
const SLOTS: usize = 3;

/// One question.
#[derive(Clone, Debug)]
pub struct Question {
    /// For the `[ask]` lines: "replace", "zip delete", "password", "open many".
    pub kind: &'static str,
    pub title: String,
    pub text: String,
    /// A masked field under the text.
    pub password: bool,
    /// One to three buttons, left to right; `primary` is the filled one (Return), `cancel` the one
    /// Escape, quitting and a scripted run's 30 s give.
    pub buttons: Vec<&'static str>,
    pub primary: usize,
    pub cancel: usize,
}

/// The button pressed (an index into the question's `buttons`) and the password typed (empty
/// without a field).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub button: usize,
    pub password: String,
}

impl Question {
    /// The answer Escape, quitting and the scripted 30 s give.
    pub fn cancelled(&self) -> Answer {
        Answer { button: self.cancel, password: String::new() }
    }

    /// The dialog's slots, the buttons in the last ones and "" for an empty slot.
    pub fn slots(&self) -> Vec<SharedString> {
        let empty = SLOTS - self.buttons.len();
        (0..SLOTS).map(|s| if s < empty { SharedString::new() } else { self.buttons[s - empty].into() }).collect()
    }

    /// The slot button `b` sits in.
    pub fn slot_of(&self, b: usize) -> usize {
        b + SLOTS - self.buttons.len()
    }

    /// The button in slot `s`, if there is one.
    pub fn button_of(&self, s: usize) -> Option<usize> {
        (s + self.buttons.len()).checked_sub(SLOTS).filter(|&b| b < self.buttons.len())
    }
}

/// The questions: the one shown, then the others in the order asked.
pub struct Queue<T> {
    shown: Option<T>,
    waiting: VecDeque<T>,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Queue { shown: None, waiting: VecDeque::new() }
    }
}

impl<T> Queue<T> {
    /// Add one; true when it is shown at once (none was).
    pub fn push(&mut self, t: T) -> bool {
        if self.shown.is_none() {
            self.shown = Some(t);
            true
        } else {
            self.waiting.push_back(t);
            false
        }
    }

    /// The shown one is answered: it, with the next in line shown in its place.
    pub fn answer(&mut self) -> Option<T> {
        let done = self.shown.take();
        self.shown = self.waiting.pop_front();
        done
    }

    pub fn shown(&self) -> Option<&T> {
        self.shown.as_ref()
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Everything, the shown one first, emptying the queue (quit).
    pub fn drain(&mut self) -> Vec<T> {
        self.shown.take().into_iter().chain(self.waiting.drain(..)).collect()
    }
}

/// A scripted run's question shown since `since` is answered Cancel at `now`.
pub fn overdue(since: Instant, now: Instant) -> bool {
    now.duration_since(since) >= SCRIPT_CANCEL
}

struct Pending {
    id: u64,
    q: Question,
    from: String,
    /// When it was shown (when it was asked, until then).
    since: Instant,
    reply: Box<dyn FnOnce(Answer)>,
}

#[derive(Default)]
struct Asker {
    app: Option<slint::Weak<App>>,
    queue: Queue<Pending>,
    next_id: u64,
    /// The question on screen (0: none), so one is never shown twice.
    on_screen: u64,
    timer: slint::Timer,
}

thread_local! {
    static ASKER: RefCell<Asker> = RefCell::new(Asker::default());
}

/// The UI thread, which must never wait in `ask`.
static UI: OnceLock<ThreadId> = OnceLock::new();

/// On the UI thread, once the window exists: the dialog's answers come here.
pub fn init(app: &App) {
    let _ = UI.set(std::thread::current().id());
    ASKER.with(|a| a.borrow_mut().app = Some(app.as_weak()));
    app.on_question_answered(|slot| answered(usize::try_from(slot).unwrap_or(usize::MAX)));
}

/// Ask from a worker thread and wait for the answer (Cancel when the window is gone).
pub fn ask(q: Question) -> Answer {
    let cancelled = q.cancelled();
    if UI.get() == Some(&std::thread::current().id()) {
        eprintln!("[ask] {} asked on the UI thread: Cancel", q.kind);
        return cancelled;
    }
    let from = std::thread::current().name().unwrap_or("a worker").to_string();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let sent = slint::invoke_from_event_loop(move || {
        ask_here(q, &from, move |a| {
            let _ = tx.send(a);
        })
    });
    if sent.is_err() {
        return cancelled;
    }
    // A reply dropped unsent (the UI thread gone) reads as Cancel.
    rx.recv().unwrap_or(cancelled)
}

/// Ask on the UI thread: the question is shown (or waits its turn) and `reply` gets the answer.
pub fn ask_here(q: Question, from: &str, reply: impl FnOnce(Answer) + 'static) {
    let first = ASKER.with(|a| {
        let mut a = a.borrow_mut();
        a.next_id += 1;
        let id = a.next_id;
        a.queue.push(Pending { id, q, from: from.to_string(), since: Instant::now(), reply: Box::new(reply) })
    });
    if first {
        show();
    }
}

/// A question is on screen (the driver's `waitquestion`).
pub fn shown() -> bool {
    ASKER.with(|a| a.borrow().on_screen != 0)
}

/// The driver's `answer <button>`: press the shown question's button reading `label` (any case).
/// False when no question shows or none of its buttons reads so.
pub fn answer_label(label: &str) -> bool {
    let slot = ASKER.with(|a| {
        let a = a.borrow();
        let p = a.queue.shown().filter(|p| p.id == a.on_screen)?;
        let b = p.q.buttons.iter().position(|b| b.eq_ignore_ascii_case(label))?;
        Some(p.q.slot_of(b))
    });
    match slot {
        Some(s) => {
            answered(s);
            true
        }
        None => false,
    }
}

/// After the event loop: every question shown or waiting is answered Cancel.
pub fn quit() {
    let all = ASKER.with(|a| {
        let mut a = a.borrow_mut();
        a.timer.stop();
        a.on_screen = 0;
        a.queue.drain()
    });
    if !all.is_empty() {
        eprintln!("[ask] quit: {} question(s) answered Cancel", all.len());
    }
    for p in all {
        let c = p.q.cancelled();
        (p.reply)(c);
    }
}

/// Put the queue's shown question on screen, unless it is there already.
fn show() {
    let info = ASKER.with(|a| {
        let mut a = a.borrow_mut();
        let app = a.app.as_ref()?.upgrade()?;
        let waiting = a.queue.waiting();
        let on_screen = a.on_screen;
        let p = a.queue.shown.as_mut().filter(|p| p.id != on_screen)?;
        p.since = Instant::now();
        let shown = (p.id, p.q.clone(), p.from.clone());
        a.on_screen = shown.0;
        Some((app, shown, waiting))
    });
    let Some((app, (id, q, from), waiting)) = info else { return };
    app.set_question_title(q.title.as_str().into());
    app.set_question_text(q.text.as_str().into());
    app.set_question_password(q.password);
    app.set_question_secret(SharedString::new());
    app.set_question_buttons(ModelRc::new(VecModel::from(q.slots())));
    app.set_question_primary(q.slot_of(q.primary) as i32);
    app.set_question_cancel(q.slot_of(q.cancel) as i32);
    app.invoke_show_question();
    eprintln!("[ask] shown {} \"{}\" (from {from}, {waiting} waiting)", q.kind, q.title);
    // A scripted run looks once a second whether the question has waited too long.
    if std::env::var_os("TWINFM_SCRIPT").is_some() {
        ASKER.with(|a| a.borrow().timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || unanswered(id)));
    }
}

/// A scripted run's tick: question `id`, still shown past `SCRIPT_CANCEL`, is answered Cancel.
fn unanswered(id: u64) {
    let cancel = ASKER.with(|a| {
        let a = a.borrow();
        let p = a.queue.shown().filter(|p| p.id == id && a.on_screen == id)?;
        overdue(p.since, Instant::now()).then(|| (p.q.kind, p.q.slot_of(p.q.cancel)))
    });
    if let Some((kind, slot)) = cancel {
        eprintln!("[ask] {kind} unanswered after 30 s: Cancel (scripted run)");
        answered(slot);
    }
}

/// The dialog's button in `slot` (or Return, Escape, the driver): the shown question is answered,
/// its reply runs, and the next one shows.
fn answered(slot: usize) {
    let done = ASKER.with(|a| {
        let mut a = a.borrow_mut();
        if a.on_screen == 0 {
            return None;
        }
        a.timer.stop();
        a.on_screen = 0;
        let app = a.app.as_ref().and_then(|w| w.upgrade());
        a.queue.answer().map(|p| (p, app))
    });
    let Some((p, app)) = done else { return };
    let button = p.q.button_of(slot).unwrap_or(p.q.cancel);
    let mut password = String::new();
    if let Some(app) = &app {
        if p.q.password {
            password = app.get_question_secret().to_string();
        }
        app.set_question_secret(SharedString::new());
        app.invoke_close_question();
    }
    eprintln!("[ask] {} answered {} after {} ms", p.q.kind, p.q.buttons[button], p.since.elapsed().as_millis());
    (p.reply)(Answer { button, password });
    show();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn q(kind: &'static str, buttons: Vec<&'static str>) -> Question {
        let n = buttons.len();
        Question { kind, title: kind.into(), text: String::new(), password: false, buttons, primary: n - 1, cancel: 0 }
    }

    #[test]
    fn two_askers_are_shown_one_at_a_time_in_order() {
        let mut queue = Queue::default();
        assert!(queue.push("first"), "nothing shown: the first shows at once");
        assert!(!queue.push("second"), "the second waits");
        assert!(!queue.push("third"));
        assert_eq!((queue.shown(), queue.waiting()), (Some(&"first"), 2));
        assert_eq!(queue.answer(), Some("first"));
        assert_eq!((queue.shown(), queue.waiting()), (Some(&"second"), 1), "the next in line shows");
        assert_eq!(queue.answer(), Some("second"));
        assert_eq!(queue.answer(), Some("third"));
        assert_eq!(queue.shown(), None);
        assert_eq!(queue.answer(), None);
        assert!(queue.push("later"), "an empty queue shows the next at once");
    }

    #[test]
    fn quitting_answers_cancel_to_every_question_in_order() {
        let answers: Rc<RefCell<Vec<(&str, Answer)>>> = Rc::default();
        let mut queue: Queue<(Question, Box<dyn FnOnce(Answer)>)> = Queue::default();
        for (kind, buttons) in [("replace", vec!["Cancel", "Skip", "Replace"]), ("password", vec!["Cancel", "OK"])] {
            let to = answers.clone();
            queue.push((q(kind, buttons), Box::new(move |a| to.borrow_mut().push((kind, a)))));
        }
        for (question, reply) in queue.drain() {
            reply(question.cancelled());
        }
        let got = answers.borrow();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], ("replace", Answer { button: 0, password: String::new() }), "the shown one first");
        assert_eq!(got[1], ("password", Answer { button: 0, password: String::new() }));
        assert_eq!(queue.shown().map(|_| ()), None, "the queue is empty");
    }

    #[test]
    fn open_many_keeps_its_items_until_the_answer() {
        // The UI thread's question goes on in its reply, with the items it would have opened.
        let opened: Rc<RefCell<Vec<String>>> = Rc::default();
        let items = vec!["a.txt".to_string(), "b.txt".to_string(), "sub".to_string()];
        let mut queue: Queue<Box<dyn FnOnce(Answer)>> = Queue::default();
        let (to, kept) = (opened.clone(), items.clone());
        queue.push(Box::new(move |a: Answer| {
            if a.button == 1 {
                to.borrow_mut().extend(kept);
            }
        }));
        assert!(opened.borrow().is_empty(), "nothing opened before the answer");
        let reply = queue.answer().expect("shown");
        reply(Answer { button: 1, password: String::new() });
        assert_eq!(*opened.borrow(), items);
    }

    #[test]
    fn a_scripted_question_is_cancelled_after_30_s() {
        let t0 = Instant::now();
        assert!(!overdue(t0, t0 + Duration::from_secs(29)));
        assert!(overdue(t0, t0 + SCRIPT_CANCEL));
        assert!(overdue(t0, t0 + Duration::from_secs(31)));
    }

    #[test]
    fn buttons_sit_in_the_last_slots() {
        let three = q("replace", vec!["Cancel", "Skip", "Replace"]);
        assert_eq!(three.slots(), vec![SharedString::from("Cancel"), "Skip".into(), "Replace".into()]);
        assert_eq!((three.slot_of(2), three.button_of(0), three.button_of(2)), (2, Some(0), Some(2)));
        let two = q("zip delete", vec!["Cancel", "Delete"]);
        assert_eq!(two.slots(), vec![SharedString::new(), "Cancel".into(), "Delete".into()]);
        assert_eq!((two.slot_of(0), two.slot_of(1)), (1, 2));
        assert_eq!((two.button_of(0), two.button_of(1), two.button_of(2)), (None, Some(0), Some(1)));
        assert_eq!(two.button_of(9), None);
        assert_eq!(two.cancelled(), Answer { button: 0, password: String::new() });
    }
}
