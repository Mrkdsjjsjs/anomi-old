//! Matrix and voice task ownership shared by native UI adapters.

use std::{path::PathBuf, sync::Arc, time::Duration};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
    chat::{self, Command, Event, Start},
    runtime::{BackgroundRuntime, BackgroundTask},
};

/// How long dropping the backend, as the program quits, waits for it to
/// leave a call and close its connections; see "Quitting during a call" in
/// docs/discord/voice.md. Discord's part takes a few milliseconds on a
/// working network; without one, the program quits all the same.
pub const QUIT_WAIT: Duration = Duration::from_millis(500);

/// Keep this handle for the signed-in client's lifetime, independently of the
/// selected conversation. Drop it on the client thread to end network work:
/// the actor leaves a call and closes Discord's gateway, for up to
/// [`QUIT_WAIT`], and is then cancelled.
/// Explicit logout is a command, so the session is revoked only after voice and
/// sync have stopped successfully.
pub struct Backend {
    /// Taken as the backend drops, which ends the actor's loop.
    commands: Option<UnboundedSender<Command>>,
    /// Disconnects once the actor has ended, or was dropped.
    ended: std::sync::mpsc::Receiver<()>,
    _actor: BackgroundTask<()>,
    // The actor must be cancelled before its runtime is released.
    _runtime: Arc<BackgroundRuntime>,
}

impl Backend {
    pub fn start(
        runtime: Arc<BackgroundRuntime>,
        root: PathBuf,
        start: Start,
    ) -> (Self, UnboundedReceiver<Event>) {
        let (commands, receiver) = mpsc::unbounded_channel();
        let (sender, events) = mpsc::unbounded_channel();
        let actor = chat::run(root, start, receiver, sender);
        (Self::spawn(runtime, commands, actor), events)
    }

    /// Runs `actor`, which ends once `commands` closes.
    fn spawn(
        runtime: Arc<BackgroundRuntime>,
        commands: UnboundedSender<Command>,
        actor: impl Future<Output = ()> + Send + 'static,
    ) -> Self {
        let (running, ended) = std::sync::mpsc::channel::<()>();
        let actor = runtime.spawn(async move {
            // Dropped as the actor ends, or is dropped.
            let _running = running;
            actor.await;
        });
        Self {
            commands: Some(commands),
            ended,
            _actor: actor,
            _runtime: runtime,
        }
    }

    /// Return the command on failure so the application can clear pending state.
    pub fn send(&self, command: Command) -> Result<(), Command> {
        match &self.commands {
            Some(commands) => commands.send(command).map_err(|error| error.0),
            None => Err(command),
        }
    }
}

impl Drop for Backend {
    /// Closing the commands ends the actor's loop: it leaves Discord's call
    /// and closes the gateway, then the call's devices go (`chat::run`).
    /// This waits for that, up to [`QUIT_WAIT`], before the actor is
    /// cancelled and the program may end.
    fn drop(&mut self) {
        use std::sync::mpsc::RecvTimeoutError;
        self.commands.take();
        if self.ended.recv_timeout(QUIT_WAIT) == Err(RecvTimeoutError::Timeout) {
            tracing::warn!(
                waited_ms = QUIT_WAIT.as_millis() as u64,
                "the backend didn't finish quitting in time; stopping it"
            );
        }
    }
}

/// Calls `quit` on SIGTERM, SIGINT or SIGHUP: the desktop session ending,
/// Ctrl+C in the terminal it runs in, or that terminal closing. The program
/// then quits as from its tray's Quit, and its backend leaves a call for
/// everyone as it drops. A second signal, for a quit that doesn't come,
/// ends the process at once. Keep the task while the program runs. Only
/// Unix has these signals; elsewhere there's no task.
pub fn quit_on_signals(
    runtime: &BackgroundRuntime,
    quit: impl Fn() + Send + 'static,
) -> Option<BackgroundTask<()>> {
    #[cfg(unix)]
    return Some(runtime.spawn(listen_to_quit(quit)));
    #[cfg(not(unix))]
    {
        let _ = (runtime, quit);
        None
    }
}

/// [`quit_on_signals`]' task, on the runtime, which `signal` needs.
#[cfg(unix)]
async fn listen_to_quit(quit: impl Fn() + Send + 'static) {
    use tokio::signal::unix::{SignalKind, signal};
    let listen = |kind: SignalKind| {
        signal(kind)
            .inspect_err(|error| tracing::warn!(%error, "could not listen for a signal"))
            .ok()
    };
    let mut term = listen(SignalKind::terminate());
    let mut interrupt = listen(SignalKind::interrupt());
    let mut hangup = listen(SignalKind::hangup());
    let mut asked = false;
    loop {
        let name = tokio::select! {
            () = next_signal(&mut term) => "SIGTERM",
            () = next_signal(&mut interrupt) => "SIGINT",
            () = next_signal(&mut hangup) => "SIGHUP",
        };
        if asked {
            tracing::warn!(signal = name, "asked to quit again; ending at once");
            std::process::exit(1);
        }
        tracing::info!(signal = name, "asked to quit");
        asked = true;
        quit();
    }
}

/// The next time `signal` comes; never for one not listened to.
#[cfg(unix)]
async fn next_signal(signal: &mut Option<tokio::signal::unix::Signal>) {
    if let Some(signal) = signal
        && signal.recv().await.is_some()
    {
        return;
    }
    std::future::pending().await
}

#[cfg(test)]
mod quitting {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    /// Says when it drops: the actor was cancelled.
    struct Dropped(Arc<AtomicBool>);

    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// Dropping the backend waits for its actor to finish what it does as
    /// the commands close, and no longer than that.
    #[test]
    fn dropping_waits_for_the_actor_to_finish_quitting() {
        let runtime = Arc::new(BackgroundRuntime::new().unwrap());
        let (commands, mut received) = mpsc::unbounded_channel();
        let finished = Arc::new(AtomicBool::new(false));
        let done = finished.clone();
        // The test keeps the runtime, so the time below is the backend's
        // wait alone, not the runtime's shutdown.
        let backend = Backend::spawn(runtime.clone(), commands, async move {
            while received.recv().await.is_some() {}
            // Leaving a call and closing the gateway, as it were.
            tokio::time::sleep(Duration::from_millis(100)).await;
            done.store(true, Ordering::SeqCst);
        });
        assert!(backend.send(Command::RetrySync).is_ok());
        let started = Instant::now();
        drop(backend);
        let waited = started.elapsed();
        assert!(
            finished.load(Ordering::SeqCst),
            "it quit before the actor finished"
        );
        assert!(waited >= Duration::from_millis(100), "{waited:?}");
        assert!(waited < QUIT_WAIT, "it waited out the bound: {waited:?}");
    }

    /// An actor that doesn't finish, as with the network gone, holds the
    /// program up for [`QUIT_WAIT`] at most, and is then cancelled.
    #[test]
    fn dropping_gives_up_on_an_actor_that_doesnt_finish() {
        let runtime = Arc::new(BackgroundRuntime::new().unwrap());
        let (commands, _received) = mpsc::unbounded_channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let guard = Dropped(cancelled.clone());
        let backend = Backend::spawn(runtime.clone(), commands, async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        });
        let started = Instant::now();
        drop(backend);
        let waited = started.elapsed();
        assert!(waited >= QUIT_WAIT, "{waited:?}");
        assert!(waited < QUIT_WAIT + Duration::from_secs(1), "{waited:?}");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cancelled.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "the actor wasn't cancelled");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// An actor already ended, as after a sign-out, keeps nothing waiting.
    #[test]
    fn dropping_an_ended_backend_waits_for_nothing() {
        let runtime = Arc::new(BackgroundRuntime::new().unwrap());
        let (commands, _received) = mpsc::unbounded_channel();
        // Kept, as above: the runtime's shutdown joins its threads, which
        // under load takes longer than the backend's own drop.
        let backend = Backend::spawn(runtime.clone(), commands, async {});
        // Until it has run.
        let deadline = Instant::now() + Duration::from_secs(2);
        while backend.ended.try_recv() != Err(std::sync::mpsc::TryRecvError::Disconnected) {
            assert!(Instant::now() < deadline, "the actor didn't run");
            std::thread::sleep(Duration::from_millis(5));
        }
        let started = Instant::now();
        drop(backend);
        assert!(started.elapsed() < Duration::from_millis(50));
    }
}
