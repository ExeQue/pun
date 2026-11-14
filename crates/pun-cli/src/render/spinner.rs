use super::Look;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const DELAY: Duration = Duration::from_secs(1);
const FRAME: Duration = Duration::from_millis(80);

pub(crate) struct Spinner {
    done: Arc<AtomicUsize>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl Spinner {
    pub(crate) fn stderr(total: usize, look: &Look, tty: bool) -> Option<Spinner> {
        tty.then(|| Spinner::start(Box::new(std::io::stderr()), total, look.unicode, DELAY))
    }

    pub(crate) fn start(
        mut out: Box<dyn Write + Send>,
        total: usize,
        unicode: bool,
        delay: Duration,
    ) -> Spinner {
        let done = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let (count, signal) = (done.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let frames: Vec<char> = if unicode {
                "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏".chars().collect()
            } else {
                "|/-\\".chars().collect()
            };
            let wait = |timeout: Duration| {
                let (lock, cvar) = &*signal;
                let guard = lock.lock().expect("spinner lock");
                *cvar
                    .wait_timeout_while(guard, timeout, |stopped| !*stopped)
                    .expect("spinner lock")
                    .0
            };
            if wait(delay) {
                return;
            }
            for frame in frames.iter().cycle() {
                let _ = write!(
                    out,
                    "\r{frame} {}/{total} files",
                    count.load(Ordering::Relaxed)
                );
                let _ = out.flush();
                if wait(FRAME) {
                    break;
                }
            }
            let _ = write!(out, "\r\x1b[2K");
            let _ = out.flush();
        });
        Spinner {
            done,
            stop,
            thread: Some(thread),
        }
    }

    pub(crate) fn tick(&self) {
        self.done.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.stop;
        *lock.lock().expect("spinner lock") = true;
        cvar.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
