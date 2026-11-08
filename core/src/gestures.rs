//! Turns button presses and releases into gestures: a single press, a
//! double press, or a hold.
//!
//! A button with only a single action fires at once on press, with no
//! delay. A button with a double action waits after a short press to see if
//! a second press comes; a button with a hold action waits to see if the
//! press lasts. Only those buttons pay the wait.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const DOUBLE_WINDOW: Duration = Duration::from_millis(300);
pub const HOLD_TIME: Duration = Duration::from_millis(500);
/// A press this soon after a release is a bounce of the button, not a
/// second press: no person presses that fast. A Switch Pro controller
/// sometimes reports one press as two, which turned Y into "Y double".
pub const BOUNCE: Duration = Duration::from_millis(40);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Single,
    Double,
    Hold,
}

impl Gesture {
    /// The suffix in mapping.json: "X", "X double", "Y hold".
    pub fn key(self, button: &str) -> String {
        match self {
            Gesture::Single => button.to_string(),
            Gesture::Double => format!("{button} double"),
            Gesture::Hold => format!("{button} hold"),
        }
    }
}

/// Which gestures a button has an action for.
#[derive(Debug, Clone, Copy, Default)]
pub struct Has {
    pub double: bool,
    pub hold: bool,
}

#[derive(Default)]
struct ButtonState {
    down_since: Option<Instant>,
    /// The press already produced a gesture; its release does nothing.
    consumed: bool,
    /// A short press ended, and a second press would make a double.
    waiting_double: bool,
    /// Bumped on every press and release, so stale timers do nothing.
    generation: u64,
    released_at: Option<Instant>,
    /// The current press is a bounce; its release does nothing either.
    bounce: bool,
}

pub struct Recognizer {
    buttons: Mutex<HashMap<String, ButtonState>>,
    fire: Box<dyn Fn(&str, Gesture) + Send + Sync>,
}

impl Recognizer {
    pub fn new(fire: impl Fn(&str, Gesture) + Send + Sync + 'static) -> Arc<Recognizer> {
        Arc::new(Recognizer { buttons: Mutex::new(HashMap::new()), fire: Box::new(fire) })
    }

    pub fn press(self: &Arc<Self>, button: &str, has: Has) {
        let fire_now = {
            let mut buttons = self.buttons.lock().unwrap();
            let state = buttons.entry(button.to_string()).or_default();
            let gap = state.released_at.map(|t| t.elapsed());
            if state.waiting_double && gap.is_some_and(|g| g < BOUNCE) {
                // Leave the state and its timer as they are.
                eprintln!("{button}: bounce after {} ms, ignored", gap.unwrap_or_default().as_millis());
                state.bounce = true;
                return;
            }
            state.generation += 1;
            state.down_since = Some(Instant::now());
            if state.waiting_double {
                state.waiting_double = false;
                state.consumed = true;
                eprintln!("{button}: second press {} ms after the first ended", gap.unwrap_or_default().as_millis());
                Some(Gesture::Double)
            } else if !has.double && !has.hold {
                state.consumed = true;
                Some(Gesture::Single)
            } else {
                state.consumed = false;
                if has.hold {
                    self.after(button, state.generation, HOLD_TIME, Timer::Hold);
                }
                None
            }
        };
        if let Some(gesture) = fire_now {
            (self.fire)(button, gesture);
        }
    }

    pub fn release(self: &Arc<Self>, button: &str, has: Has) {
        let fire_now = {
            let mut buttons = self.buttons.lock().unwrap();
            let state = buttons.entry(button.to_string()).or_default();
            if state.bounce {
                state.bounce = false;
                return;
            }
            state.generation += 1;
            state.down_since = None;
            state.released_at = Some(Instant::now());
            if state.consumed {
                state.consumed = false;
                None
            } else if has.double {
                state.waiting_double = true;
                self.after(button, state.generation, DOUBLE_WINDOW, Timer::Double);
                None
            } else {
                Some(Gesture::Single)
            }
        };
        if let Some(gesture) = fire_now {
            (self.fire)(button, gesture);
        }
    }

    fn after(self: &Arc<Self>, button: &str, generation: u64, delay: Duration, timer: Timer) {
        let me = Arc::clone(self);
        let button = button.to_string();
        thread::spawn(move || {
            thread::sleep(delay);
            let gesture = {
                let mut buttons = me.buttons.lock().unwrap();
                let Some(state) = buttons.get_mut(&button) else { return };
                if state.generation != generation {
                    return;
                }
                match timer {
                    // Still down after HOLD_TIME: a hold.
                    Timer::Hold if state.down_since.is_some() => {
                        state.consumed = true;
                        Gesture::Hold
                    }
                    // No second press within DOUBLE_WINDOW: a single press.
                    Timer::Double if state.waiting_double => {
                        state.waiting_double = false;
                        Gesture::Single
                    }
                    _ => return,
                }
            };
            (me.fire)(&button, gesture);
        });
    }
}

#[derive(Clone, Copy)]
enum Timer {
    Hold,
    Double,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorder() -> (Arc<Recognizer>, Arc<Mutex<Vec<(String, Gesture)>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&log);
        let r = Recognizer::new(move |b, g| sink.lock().unwrap().push((b.to_string(), g)));
        (r, log)
    }

    #[test]
    fn single_without_extras_fires_on_press() {
        let (r, log) = recorder();
        r.press("A", Has::default());
        assert_eq!(*log.lock().unwrap(), vec![("A".into(), Gesture::Single)]);
        r.release("A", Has::default());
        assert_eq!(log.lock().unwrap().len(), 1);
    }

    #[test]
    fn bounce_is_one_press() {
        let has = Has { double: true, hold: true };
        let (r, log) = recorder();
        r.press("Y", has);
        r.release("Y", has);
        r.press("Y", has); // a bounce, a few ms later
        r.release("Y", has);
        thread::sleep(DOUBLE_WINDOW + Duration::from_millis(100));
        assert_eq!(*log.lock().unwrap(), vec![("Y".into(), Gesture::Single)]);
    }

    #[test]
    fn double_and_single_and_hold() {
        let has = Has { double: true, hold: true };
        let (r, log) = recorder();
        r.press("X", has);
        r.release("X", has);
        thread::sleep(BOUNCE + Duration::from_millis(20));
        r.press("X", has);
        r.release("X", has);
        thread::sleep(DOUBLE_WINDOW + Duration::from_millis(100));
        assert_eq!(*log.lock().unwrap(), vec![("X".into(), Gesture::Double)]);

        log.lock().unwrap().clear();
        r.press("X", has);
        r.release("X", has);
        thread::sleep(DOUBLE_WINDOW + Duration::from_millis(100));
        assert_eq!(*log.lock().unwrap(), vec![("X".into(), Gesture::Single)]);

        log.lock().unwrap().clear();
        r.press("X", has);
        thread::sleep(HOLD_TIME + Duration::from_millis(100));
        r.release("X", has);
        thread::sleep(DOUBLE_WINDOW + Duration::from_millis(100));
        assert_eq!(*log.lock().unwrap(), vec![("X".into(), Gesture::Hold)]);
    }
}
