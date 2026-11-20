//! Go Live runs in a worker process, `yapfast --stream-worker`, so screen
//! capture, video codecs and stream connections take memory only while
//! sharing or watching: the process exits when that stops and the memory
//! goes with it. Each worker shares this account's screen or shows one
//! other person's stream.
//!
//! Yapfast starts the worker with one [`Job`] as a JSON line on its standard
//! input, and reads [`Report`]s back from its standard output. Once Discord
//! has started the stream, a [`Control::Send`] line hands the worker the
//! stream's server, and it sends the video there; a [`Control::Switch`] line
//! shares another screen, window or camera on the same stream. A `stop` line, or
//! closing its input, ends the job. Frames go to the UI through [`crate::frames`].
//!
//! What Settings changes while workers run reaches each of them as a line
//! too: the cores they may use ([`set_cpus`]), where a stream encodes
//! ([`set_encode_on`]) and whether its bitrate may pass Discord's
//! documented cap ([`unlock_bitrate`]).

use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc as channel,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use yapfast_discord::{
    congestion,
    rtp::Codec,
    stream::StreamServer,
    stream_connection::{self, Link, StreamReceiver, StreamSender},
};

use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};

use crate::{
    adapters::{self, Adapter, EncoderApi},
    cpu::CpuMask,
    decode::Decoder,
    encode::{self, Device, Effort, Encoder},
    frames::{Frames, Writer},
    picture::{I420, Turn},
    stream_audio,
    voice::{Resolution, StreamAudio, StreamQuality, StreamStats},
};
use yapfast_lang::tr;

/// The argument that starts Yapfast as a stream worker.
pub const ARGUMENT: &str = "--stream-worker";
/// The variable that tells a stream worker the language Yapfast chose.
const LANGUAGE: &str = "YAPFAST_WORKER_LANGUAGE";

/// Set for a worker whose UI wants to be woken for each frame: it then says
/// so on its reports' line ([`crate::frames::FRAME_LINE`]).
const FRAME_LINES: &str = "YAPFAST_FRAME_LINES";

/// The largest frame shared at 1080p. Discord's own client trials 1080p at
/// 30 fps for accounts without Nitro, up to 5 Mbit/s (its "frontier tuning"
/// experiment); larger screens are scaled down to it.
pub const SHARE_WIDTH: u32 = 1920;
pub const SHARE_HEIGHT: u32 = 1080;
/// The largest preview of the shared screen the UI shows.
pub const PREVIEW_WIDTH: u32 = 1280;
pub const PREVIEW_HEIGHT: u32 = 720;
/// The largest picture shown of someone else's stream; larger ones are
/// scaled down to it.
pub const WATCH_WIDTH: u32 = 1920;
pub const WATCH_HEIGHT: u32 = 1080;
/// The largest preview Discord shows of a stream, and how often its client
/// sends a new one.
const THUMBNAIL_WIDTH: u32 = 512;
const THUMBNAIL_HEIGHT: u32 = 288;
const THUMBNAIL_EVERY: Duration = Duration::from_secs(5 * 60);
/// How long a shared stream whose connection ended waits for a server
/// Discord moved it to.
const MOVE_GRACE: Duration = Duration::from_secs(5);
/// How often a shared screen reports how it goes out, for the call's details.
const SENDING_EVERY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    /// Captures `source`, writing frames that fit `width` by `height` to the
    /// frames file at `frames`.
    Share {
        source: Source,
        /// The sound shared with it; never any with the test pattern.
        #[serde(default)]
        audio: StreamAudio,
        frames: PathBuf,
        width: u32,
        height: u32,
        fps: u32,
        /// How hard the encoder works on each frame.
        #[serde(default)]
        effort: Effort,
        /// Where it encodes, as Settings chose.
        #[serde(default)]
        encode_on: EncodeOn,
        /// The most it sends, in kbit/s, within Discord's cap; none for
        /// Auto. See [`stream_connection::cap`].
        #[serde(default)]
        max_kbps: Option<u32>,
        /// The limit may pass Discord's documented 9 Mbit/s, up to 50, as
        /// Settings' "Unlocked bitrate" lets it ([`unlock_bitrate`]).
        #[serde(default)]
        bitrate_unlocked: bool,
    },
    /// Receives and decodes the stream at `server`, writing pictures that
    /// fit `width` by `height` to the frames file at `frames`.
    Watch {
        server: StreamServer,
        frames: PathBuf,
        width: u32,
        height: u32,
        /// Where the stream's sound plays: the call's speakers, or the
        /// default output.
        #[serde(default)]
        speakers: Option<String>,
        /// Deafened in the call, so the stream's sound stays quiet.
        #[serde(default)]
        deafened: bool,
        /// How loud its sound plays; see [`crate::volume`].
        #[serde(default = "as_it_comes")]
        volume: f32,
        /// How its sound is cleaned before it plays, as its tile's menu
        /// chose ([`crate::stream_cleaning`]); nothing unless chosen.
        #[serde(default)]
        cleaning: crate::volume::Cleaning,
    },
    /// Plays the video file at `file`, which may still be downloading,
    /// writing pictures that fit `width` by `height` to the frames file at
    /// `frames`; see [`crate::video_player`].
    Play {
        file: PathBuf,
        /// Its size, when known before it's downloaded.
        size: Option<u64>,
        frames: PathBuf,
        width: u32,
        height: u32,
        speakers: Option<String>,
        volume: f32,
    },
    /// Takes a picture of each screen in `screens`, by the ids [`shareable`]
    /// listed them with, scaled down to fit `width` by `height`, for the
    /// share menu to show. Each comes as a [`Report::Screen`].
    Glance {
        screens: Vec<u64>,
        width: u32,
        height: u32,
    },
}

impl Job {
    /// What the job is, for the log.
    fn kind(&self) -> &'static str {
        match self {
            Self::Share { .. } => "share",
            Self::Watch { .. } => "watch",
            Self::Play { .. } => "play",
            Self::Glance { .. } => "glance",
        }
    }

    /// The size its pictures fit in.
    fn bounds(&self) -> (u32, u32) {
        match *self {
            Self::Share { width, height, .. }
            | Self::Watch { width, height, .. }
            | Self::Play { width, height, .. }
            | Self::Glance { width, height, .. } => (width, height),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A screen or window the person picks in the system's dialog, or what
    /// they picked from [`shareable`] where there's none.
    Screen,
    /// A screen by the id [`shareable`] listed it with.
    Display(u64),
    /// A window by the id [`shareable`] listed it with.
    Window(u64),
    /// A camera by its id in [`crate::camera::list`], such as OBS's virtual
    /// camera.
    Camera(String),
    /// Moving colour bars, for testing without a screen to capture.
    TestPattern,
    /// Where [`portal_picks`], a screen the system's dialog picks, offering
    /// screens alone, as Discord's "Share entire screen" asks it; elsewhere
    /// the first screen.
    PortalScreen,
    /// Where [`portal_picks`], a window the system's dialog picks, offering
    /// windows alone, as Discord's "Share a window" asks it; elsewhere the
    /// first window.
    PortalWindow,
}

/// Whether the system's dialog, the screen sharing portal, picks the screen or
/// window to share, rather than Yapfast listing them: in a Wayland session.
/// libwebrtc captures through the portal only there, and through X11 in any
/// other Linux session, as it decides by the same variables; its X11
/// capturers list what there is, as Windows' do. XFCE, for one, has no
/// screen sharing portal on X11.
pub fn portal_picks() -> bool {
    cfg!(target_os = "linux")
        && wayland_session(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
        )
}

/// Whether a session of this `kind`, in `XDG_SESSION_TYPE`, with a Wayland
/// display named or not, is one libwebrtc captures through the portal in.
fn wayland_session(kind: Option<&str>, wayland_display: bool) -> bool {
    kind.is_some_and(|kind| kind.starts_with("wayland")) && wayland_display
}

/// A screen, window or camera [`shareable`] lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shareable {
    pub source: Source,
    pub title: String,
}

/// What Yapfast can share: the screens, then the windows, unless
/// [`portal_picks`], then the cameras. On Windows, Yapfast's own windows
/// aren't among them, nor its console's.
pub fn shareable() -> Vec<Shareable> {
    let mut listed = screens_and_windows();
    listed.extend(crate::camera::list().into_iter().map(|device| Shareable {
        source: Source::Camera(device.id),
        title: device.name,
    }));
    listed
}

fn screens_and_windows() -> Vec<Shareable> {
    if portal_picks() {
        return Vec::new();
    }
    use livekit::webrtc::desktop_capturer::DesktopCaptureSourceType;
    let screens = sources(DesktopCaptureSourceType::Screen);
    let count = screens.len();
    let screens = screens
        .into_iter()
        .enumerate()
        .map(|(index, screen)| Shareable {
            source: Source::Display(screen.id()),
            title: if count == 1 {
                tr!("Entire screen").to_owned()
            } else {
                tr!("Screen {}", index + 1)
            },
        });
    // The console belongs to its host, so libwebrtc lists it as another
    // application's window.
    #[cfg(windows)]
    // SAFETY: no arguments; it only reads this process's console.
    let console = unsafe { windows::Win32::System::Console::GetConsoleWindow() }.0 as u64;
    #[cfg(not(windows))]
    let console = 0;
    let windows = sources(DesktopCaptureSourceType::Window)
        .into_iter()
        .filter_map(|window| {
            let title = window.title();
            (!title.trim().is_empty() && window.id() != console).then(|| Shareable {
                source: Source::Window(window.id()),
                title,
            })
        });
    screens.chain(windows).collect()
}

fn sources(
    kind: livekit::webrtc::desktop_capturer::DesktopCaptureSourceType,
) -> Vec<livekit::webrtc::desktop_capturer::CaptureSource> {
    livekit::webrtc::desktop_capturer::DesktopCapturer::new(
        livekit::webrtc::desktop_capturer::DesktopCapturerOptions::new(kind),
    )
    .map(|capturer| capturer.get_source_list())
    .unwrap_or_default()
}

/// What [`Source::Screen`] shares: the pick from [`shareable`] in the share
/// menu, until another. Where [`portal_picks`], the pick may be
/// [`Source::Screen`] itself, which the system's dialog picks.
static PICKED: Mutex<Source> = Mutex::new(Source::Screen);

/// Shares `source` from now on where [`Source::Screen`] is asked for.
pub fn pick(source: Source) {
    *PICKED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = source;
}

/// `source`, with [`Source::Screen`] as what was picked.
fn picked(source: Source) -> Source {
    match source {
        Source::Screen => PICKED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        source => source,
    }
}

/// What Yapfast tells a running job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    /// Send the video to the server Discord started the stream on.
    Send(StreamServer),
    /// Share this screen or window in place of the one shared, once it gives
    /// a picture; where [`portal_picks`], what the system's dialog picks. The
    /// stream goes on with the one shared until then, and when the dialog is
    /// cancelled.
    Switch(Source),
    /// Share this sound from now on, keeping the picture and the stream.
    Audio(StreamAudio),
    /// Share at this quality from now on, keeping the stream and what it
    /// shares: frames that fit `width` by `height`, `fps` a second, encoded
    /// with `effort`, at most `max_kbps` (none for Auto).
    Quality {
        width: u32,
        height: u32,
        fps: u32,
        effort: Effort,
        #[serde(default)]
        max_kbps: Option<u32>,
    },
    /// Encode where Settings now says, from the next frame on, keeping the
    /// stream: an encoder opens there as for a new quality.
    EncodeOn(EncodeOn),
    /// Let the bitrate limit past Discord's documented 9 Mbit/s, or no
    /// more, from the next frame on, keeping the stream
    /// ([`unlock_bitrate`]).
    BitrateUnlocked(bool),
    /// Run on these logical CPUs, or on every one Yapfast started with when
    /// none; see [`set_cpus`]. Any job takes it, from the line that brings
    /// it.
    Cpus(Option<CpuMask>),
    /// Play a watched stream's sound, or not, as being deafened changes.
    Hear(bool),
    /// Play a watched stream's or a video's sound this loud.
    Volume(f32),
    /// Clean a watched stream's sound so from now on
    /// ([`crate::stream_cleaning`]).
    Cleaning(crate::volume::Cleaning),
    /// What the call sent, tick by tick, for a watched stream's echo gate:
    /// the line's reader hands it straight to the stream's player
    /// ([`crate::stream_cleaning::sent`]), past the job's loop.
    Echo(Vec<crate::echo_gate::Ours>),
    /// Play the video, from the start once it has ended.
    Play,
    Pause,
    /// Move to this many milliseconds into the video.
    Seek(u64),
    /// Draw the video's pictures to fit this.
    Size {
        width: u32,
        height: u32,
    },
    /// These bytes of the video's file arrived, `from` up to `to`; `size`
    /// is the whole file's, once known.
    Arrived {
        from: u64,
        to: u64,
        size: Option<u64>,
    },
    /// The video's file finished downloading, with this size; none when the
    /// download failed.
    Downloaded {
        size: Option<u64>,
    },
}

fn as_it_comes() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Report {
    /// The worker opened the frames file.
    Ready,
    /// Frames are flowing, at this size after scaling.
    Sharing { width: u32, height: u32 },
    /// Sharing: the sound asked for couldn't be heard, and the stream goes
    /// on without it.
    AudioError(String),
    /// Sharing: how the stream encodes isn't what Settings asked for, as
    /// when the graphics card chosen couldn't encode it, or x264 can't keep
    /// up on the cores chosen, and the stream goes on. The dock shows it as
    /// a stream's error.
    Notice(String),
    /// Sharing: the video reaches the stream server. Watching: the first
    /// picture is on screen.
    Streaming,
    /// Sharing: how the stream goes out, once a second while it's live.
    Sending(StreamStats),
    /// Sharing: a preview of the stream for Discord to show, a JPEG as a
    /// data URI. The first comes once a picture isn't black, then one every
    /// 5 minutes, as Discord's client sends them.
    Preview { thumbnail: String },
    /// The video file opened: how long it plays, when it says, its
    /// pictures' size, and whether its sound plays.
    Opened {
        duration_ms: Option<u64>,
        width: u32,
        height: u32,
        sound: bool,
    },
    /// The video waits for this byte of its file, which hasn't arrived.
    Needs { at: u64 },
    /// The video played to its end, and waits there.
    Finished,
    /// A picture of the screen [`shareable`] listed by `id`, for
    /// [`Job::Glance`]: RGBA pixels, `width` by `height`.
    Screen {
        id: u64,
        width: u32,
        height: u32,
        #[serde(with = "base64_pixels")]
        rgba: Vec<u8>,
    },
    /// The job ended: without an error when it was told to stop.
    Ended { error: Option<String> },
}

/// Runs this process as a stream worker until its job ends.
pub fn run() -> Result<()> {
    // The stream connection uses TLS; nothing else here has started it.
    let _ = crate::init_tls();
    // What it reports, such as why the stream's sound can't be heard, shows
    // in Yapfast's language.
    if let Some(code) = std::env::var_os(LANGUAGE) {
        yapfast_lang::choose(&code.to_string_lossy());
    }
    if std::env::var_os(FRAME_LINES).is_some() {
        crate::frames::tell_each_frame(|| {
            let mut output = std::io::stdout().lock();
            let _ = writeln!(output, "{}", crate::frames::FRAME_LINE);
            let _ = output.flush();
        });
    }
    let mut output = std::io::stdout();
    serve(Box::new(BufReader::new(std::io::stdin())), &mut |report| {
        // Yapfast is gone if this fails; the job ends when its input closes.
        let _ = writeln!(
            output,
            "{}",
            serde_json::to_string(&report).unwrap_or_default()
        );
        let _ = output.flush();
    });
    Ok(())
}

fn serve(input: Box<dyn BufRead + Send>, report: &mut dyn FnMut(Report)) {
    serve_on(input, report, confine);
}

/// Puts the whole worker on `cpus`, as Yapfast chose them, or on every CPU
/// Yapfast started with for none: whatever its job, and wherever its stream
/// encodes, x264 on the processor too.
fn confine(cpus: Option<CpuMask>) {
    crate::cpu::apply(cpus.as_ref());
}

/// Runs the job the first line of `input` names, with the [`Control`]s on
/// the lines after; each [`Control::Cpus`] goes to `confine`.
fn serve_on(
    mut input: Box<dyn BufRead + Send>,
    report: &mut dyn FnMut(Report),
    confine: fn(Option<CpuMask>),
) {
    let mut line = String::new();
    let job = match input.read_line(&mut line) {
        Ok(_) => {
            serde_json::from_str::<Job>(&line).map_err(|error| anyhow!("unknown job: {error}"))
        }
        Err(error) => Err(error.into()),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = stop.clone();
    let (controls, received) = channel::channel();
    // Reading stdin blocks, so a thread waits for controls, the stop line or the end.
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if line.trim() == "stop" {
                break;
            }
            match serde_json::from_str::<Control>(&line) {
                // The whole process's, whatever its job.
                Ok(Control::Cpus(cpus)) => confine(cpus),
                // Each tick keeps the echo gate's time, so it doesn't wait
                // for the job's loop, which looks every 100 ms at most.
                Ok(Control::Echo(ticks)) => crate::stream_cleaning::sent(ticks),
                Ok(control) => {
                    let _ = controls.send(control);
                }
                Err(error) => tracing::warn!(%error, "unknown stream worker control"),
            }
        }
        stopper.store(true, Ordering::Relaxed);
    });
    let result = job.and_then(|job| match job {
        Job::Share {
            source,
            audio,
            frames,
            width,
            height,
            fps,
            effort,
            encode_on,
            max_kbps,
            bitrate_unlocked,
        } => {
            let mut sink = Sink::new(
                Writer::open(&frames)?,
                received,
                (width, height),
                fps,
                effort,
                encode_on,
                max_kbps,
            );
            sink.bitrate_unlocked = bitrate_unlocked;
            sink.sound = (source != Source::TestPattern).then_some(audio);
            report(Report::Ready);
            match source {
                Source::TestPattern => test_pattern(sink, &stop, report),
                source => capture_screen(source, sink, &stop, report),
            }
        }
        Job::Watch {
            server,
            frames,
            width,
            height,
            speakers,
            deafened,
            volume,
            cleaning,
        } => {
            let writer = Writer::open(&frames)?;
            report(Report::Ready);
            let hearing = Arc::new(AtomicBool::new(!deafened));
            let sound = Sound {
                speakers,
                hearing,
                volume: Arc::new(stream_audio::Volume::new(volume)),
                cleaning: Arc::new(crate::stream_cleaning::Chosen::new(cleaning)),
                controls: received,
            };
            watch(server, writer, (width, height), sound, &stop, report)
        }
        Job::Play {
            file,
            size,
            frames,
            width,
            height,
            speakers,
            volume,
        } => {
            let writer = Writer::open(&frames)?;
            report(Report::Ready);
            let job = crate::video_player::Playing {
                path: file,
                size,
                width,
                height,
                sound: true,
                speakers,
                volume,
            };
            crate::video_player::play(job, writer, received, &stop, report)
        }
        Job::Glance {
            screens,
            width,
            height,
        } => {
            report(Report::Ready);
            glance(&screens, width, height, &stop, report);
            Ok(())
        }
    });
    report(Report::Ended {
        error: result.err().map(|error| format!("{error:#}")),
    });
}

/// Where each frame goes: the UI's preview, then Discord once the stream
/// has a server.
struct Sink {
    writer: Writer,
    controls: channel::Receiver<Control>,
    /// The server to send to from the next frame, which gives the stream its size.
    server: Option<StreamServer>,
    /// The sound shared with the picture, which [`Control::Audio`] changes;
    /// none for the test pattern, which shares only its bars.
    sound: Option<StreamAudio>,
    /// Why the stream's sound couldn't start, to report.
    sound_failed: Option<String>,
    video: Option<Video>,
    /// The largest frame shared, which [`Control::Quality`] changes with
    /// the frame rate, the effort and the bitrate limit.
    bounds: (u32, u32),
    fps: u32,
    effort: Effort,
    /// The most the stream sends, in kbit/s, within Discord's cap; none for
    /// Auto.
    max_kbps: Option<u32>,
    /// The limit may pass Discord's documented cap, which
    /// [`Control::BitrateUnlocked`] changes.
    bitrate_unlocked: bool,
    /// Where Settings puts the encoder, which [`Control::EncodeOn`] changes.
    encode_on: EncodeOn,
    /// The graphics encoders to try for `encode_on`, best first, once read.
    candidates: Option<Vec<Candidate>>,
    /// The adapter `encode_on` names wasn't there, which the next encoder
    /// placed says.
    missing: bool,
    /// Where the stream encodes.
    placed: Placed,
    /// What the dock should say about how the stream encodes, not reported
    /// yet.
    notice: Option<String>,
    /// How x264 keeps up with the stream on the cores chosen, while it
    /// encodes.
    pace: Pace,
    /// The dock was told x264 can't keep up: once a stream.
    told_behind: bool,
    started: Instant,
    streaming: bool,
    /// When the stream's next preview is due: from when it starts.
    preview_due: Option<Instant>,
    /// A preview made but not reported yet.
    thumbnail: Option<String>,
    /// When the stream connection ended, while Discord may still name a
    /// server it moved the stream to.
    lost: Option<Instant>,
}

/// A graphics encoder a stream may encode on, with its adapter's name for
/// the dock; none for whichever graphics encoder opens first, where no
/// adapters could be listed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    device: Device,
    name: Option<String>,
}

/// Where a shared stream encodes: every codec on one adapter, so the others,
/// the one a game draws on among them, stay free.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Placed {
    /// Not yet, or not since Settings changed: on the first candidate that
    /// opens.
    Anywhere,
    On(Candidate),
    /// On the processor, until Settings changes.
    Processor,
}

struct Video {
    sender: StreamSender,
    /// None between closing one encoder and opening the next, and after
    /// the next failed to open.
    encoder: Option<Encoder>,
    /// The stream's sound, from the first time there's any to share.
    sound: Option<stream_audio::Capture>,
    /// When the stream last reported how it goes out, and the bytes it had
    /// sent then.
    measured: (Instant, u64),
}

impl Video {
    /// How the stream goes out, with the kbit/s sent since it last said;
    /// nothing without an encoder.
    fn sending(&mut self, fps: u32) -> Option<StreamStats> {
        let encoder = self.encoder.as_ref()?;
        let sent = self.sender.sent();
        let (since, before) = self.measured;
        let bits = sent.video_bytes.saturating_sub(before) * 8;
        let milliseconds = since.elapsed().as_millis().max(1) as u64;
        self.measured = (Instant::now(), sent.video_bytes);
        let (width, height) = encoder.size();
        Some(StreamStats {
            engine: encoder.engine().to_owned(),
            codec: encoder.codec().name().to_owned(),
            width,
            height,
            fps,
            target_kbps: encoder.bitrate(),
            sent_kbps: (bits / milliseconds) as u32,
            keyframe_requests: sent.keyframe_requests,
            resent: sent.resent,
        })
    }
}

/// The codecs only a GPU encodes, most wanted first, offered before H264
/// where the GPU's encoder for them opens.
const GPU_CODECS: [Codec; 2] = [Codec::Av1, Codec::H265];

/// The graphics encoders `on` tries, best first, and whether the adapter it
/// names is missing: that adapter, then Auto's order
/// ([`adapters::encoder_order`], which [`adapters::auto_encoder`] heads);
/// where no adapters can be listed, whichever graphics encoder opens first.
/// Reading them loads no driver.
fn candidates_for(on: &EncodeOn) -> (Vec<Candidate>, bool) {
    if *on == EncodeOn::Software {
        return (Vec::new(), false);
    }
    let listed = adapters::list();
    if listed.is_empty() {
        let any = Candidate {
            device: Device::AnyGpu,
            name: None,
        };
        return (vec![any], matches!(on, EncodeOn::Adapter(_)));
    }
    let candidate = |adapter: &Adapter| {
        device_of(&listed, adapter).map(|device| Candidate {
            device,
            name: Some(adapter.name.clone()),
        })
    };
    let chosen = match on {
        EncodeOn::Adapter(key) => Some(adapters::find(&listed, key).and_then(candidate)),
        _ => None,
    };
    let mut order: Vec<Candidate> = chosen.clone().flatten().into_iter().collect();
    for adapter in adapters::encoder_order(&listed, adapters::machine()) {
        // AMF can't be pointed at an adapter: two AMD cards are one choice.
        if let Some(next) = candidate(adapter)
            && !order.iter().any(|tried| tried.device == next.device)
        {
            order.push(next);
        }
    }
    (order, chosen.is_some_and(|found| found.is_none()))
}

/// Where `adapter` encodes, among `listed`: NVENC on its CUDA number,
/// VA-API on its render node, AMF on Windows. None where this build has no
/// encoder for it.
fn device_of(listed: &[Adapter], adapter: &Adapter) -> Option<Device> {
    match adapter.encoder_api()? {
        EncoderApi::Nvenc => Some(Device::Nvenc {
            index: adapters::cuda_index(listed, adapter)?,
        }),
        EncoderApi::Amf => Some(Device::Amf),
        EncoderApi::Vaapi => Some(Device::Vaapi {
            node: adapter.render_node.clone()?,
            kernel: adapter.kernel_driver.clone(),
        }),
    }
}

fn log_encoder(encoder: &Encoder) {
    let (width, height) = encoder.size();
    tracing::info!(
        engine = encoder.engine(),
        codec = ?encoder.codec(),
        width,
        height,
        "encoding the stream"
    );
}

impl Sink {
    /// A sink for frames that fit `bounds`, `fps` a second, encoded with
    /// `effort` where `encode_on` says, at most `max_kbps`; without sound
    /// until it's given one.
    fn new(
        writer: Writer,
        controls: channel::Receiver<Control>,
        bounds: (u32, u32),
        fps: u32,
        effort: Effort,
        encode_on: EncodeOn,
        max_kbps: Option<u32>,
    ) -> Self {
        Self {
            writer,
            controls,
            server: None,
            sound: None,
            sound_failed: None,
            video: None,
            bounds,
            fps: fps.clamp(1, 60),
            effort,
            max_kbps,
            bitrate_unlocked: false,
            encode_on,
            candidates: None,
            missing: false,
            placed: Placed::Anywhere,
            notice: None,
            pace: Pace::default(),
            told_behind: false,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        }
    }

    /// Takes a frame at the size it's sent: the preview shows it, scaled
    /// down to fit, and Discord gets it once the stream has a server.
    fn frame(&mut self, picture: &mut I420Buffer) -> Result<()> {
        let (width, height) = (picture.width(), picture.height());
        let thumbnail_due = self.preview_due.is_some_and(|due| due <= Instant::now());
        // With the window behind another nobody sees the preview, so it
        // isn't scaled and converted at all, but for a thumbnail that's due.
        if !self.writer.paused() || thumbnail_due {
            let preview = fit(width, height, PREVIEW_WIDTH, PREVIEW_HEIGHT);
            let mut smaller = (preview != (width, height))
                .then(|| picture.scale(preview.0 as i32, preview.1 as i32));
            let shown = match smaller.as_mut() {
                Some(smaller) => smaller,
                None => &mut *picture,
            };
            self.writer
                .write(preview.0, preview.1, |pixels| to_rgba(shown, pixels));
            if thumbnail_due && !is_black(shown) {
                match thumbnail(shown) {
                    Ok(thumbnail) => self.thumbnail = Some(thumbnail),
                    Err(error) => {
                        tracing::warn!(error = format!("{error:#}"), "could not make a preview")
                    }
                }
                self.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
            }
        }
        if let Some(server) = self.server.take() {
            let (offers, encoder) = self.offers(width, height)?;
            let sender = StreamSender::start(server, width, height, self.fps, offers)?;
            sender.set_ceiling(self.max_kbps, self.bitrate_unlocked);
            let sound = match self.sound {
                Some(audio) if audio != StreamAudio::Off => {
                    share_sound(&sender, audio, &mut self.sound_failed)
                }
                _ => None,
            };
            self.video = Some(Video {
                sender,
                encoder: Some(encoder),
                sound,
                measured: (Instant::now(), 0),
            });
        }
        let Some(video) = self.video.as_ref() else {
            return Ok(());
        };
        // Discord's choice of codec changes as viewers come and go.
        let current = video.encoder.as_ref().map(Encoder::codec);
        let codec = (video.sender.codec().or(current)).unwrap_or(stream_connection::SEND_CODEC);
        // A new size, frame rate or effort (`Control::Quality`), another
        // place to encode (`Control::EncodeOn`, which closed the encoder),
        // cores that give x264 another count of threads (`Control::Cpus`),
        // or, with the unlock, a ceiling past the one the encoder's level
        // was chosen for, needs another encoder, which starts with a
        // keyframe.
        let reach = self.reach(height);
        let fits = video.encoder.as_ref().is_some_and(|encoder| {
            encoder.codec() == codec
                && encoder.size() == (width, height)
                && encoder.tuned(self.fps, self.effort)
                && encoder.fits_threads(crate::cpu::threads())
                && encoder.held() >= reach
        });
        if !fits {
            // Its pace starts again with it.
            self.pace = Pace::default();
            // The old encoder closes before the next opens: NVIDIA's crashed
            // the worker seconds after one closed while another ran.
            self.video.as_mut().expect("checked above").encoder = None;
            let opened = self.encoder(codec, width, height);
            let video = self.video.as_mut().expect("checked above");
            match opened {
                Ok(encoder) => {
                    video.encoder = Some(encoder);
                    video.sender.resize(width, height);
                }
                // A GPU codec that opened at the start may not open again,
                // or at another size: Discord then chooses another, which
                // the next frame opens.
                Err(error) if codec != stream_connection::SEND_CODEC => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        ?codec,
                        "the GPU's encoder didn't open again"
                    );
                    video.sender.withdraw(codec);
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
        let Some(Video {
            sender,
            encoder: Some(encoder),
            ..
        }) = self.video.as_mut()
        else {
            return Ok(());
        };
        let wanted = aim(
            sender.target(),
            height,
            self.max_kbps,
            self.bitrate_unlocked,
        );
        let aimed = encoder.bitrate();
        // Not on every wobble, which would churn the rate control.
        if wanted.abs_diff(aimed) > aimed / 10 {
            match encoder.set_bitrate(wanted) {
                Ok(()) => tracing::debug!(kbps = wanted, "the encoder follows the stream's target"),
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "could not change the bitrate")
                }
            }
        }
        let pts = self.started.elapsed().as_micros() as u64 * encode::CLOCK / 1_000_000;
        let keyframe = sender.take_keyframe_request();
        let codec = encoder.codec();
        let encoding = Instant::now();
        let encoded = encoder.encode_i420(picture, pts, keyframe);
        let took = encoding.elapsed();
        match encoded {
            Ok(Some(frame)) => sender.send(stream_connection::Frame {
                data: frame.data.to_vec(),
                codec,
                timestamp: pts as u32,
                keyframe: frame.keyframe,
            }),
            Ok(None) => {}
            // Only the GPU encodes AV1 and H265: Discord chooses another
            // codec instead, which the next frame encodes.
            Err(error) if codec != stream_connection::SEND_CODEC => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    ?codec,
                    "the GPU's encoder failed"
                );
                sender.withdraw(codec);
            }
            Err(error) => return Err(error),
        }
        // x264 took over from the graphics encoder, which failed (encode.rs).
        let on_processor = *encoder.device() == Device::Processor;
        if on_processor && matches!(self.placed, Placed::On(_)) {
            self.say(
                tr!("The graphics card's encoder failed, so the processor encodes the stream now.")
                    .to_owned(),
            );
            self.place(Placed::Processor, None);
        }
        if on_processor {
            self.encoded_in(took, Instant::now());
        } else {
            self.pace = Pace::default();
        }
        Ok(())
    }

    /// Counts a frame x264 took `took` to encode, at `now`, and has the
    /// dock say, once a stream, when it can't keep up on the cores chosen
    /// ([`Pace`]); nothing while it keeps up.
    fn encoded_in(&mut self, took: Duration, now: Instant) {
        if self.told_behind {
            return;
        }
        let Some(share) = self.pace.encoded(took, self.fps, now) else {
            return;
        };
        self.told_behind = true;
        tracing::info!(
            share_percent = share,
            fps = self.fps,
            threads = crate::cpu::threads(),
            "x264 can't keep up with the stream on the cores chosen"
        );
        self.say(
            tr!("The processor can't keep up encoding this stream on the cores chosen: they're too few for this quality. Lower the stream's quality or frame rate, or in Settings, Advanced, choose more cores or a graphics card to encode streams.")
                .to_owned(),
        );
    }

    /// The codecs to offer Discord, most wanted first, and an encoder for
    /// the first, all from one place: the first graphics adapter of
    /// [`Self::candidates`] that opens AV1 or H265, or else H264, with AV1
    /// then H265 where they open and H264 after them; or else the processor,
    /// with H264 alone. Discord streams the first every viewer decodes. Each
    /// is opened to see that it opens, and only the first kept.
    fn offers(&mut self, width: u32, height: u32) -> Result<(Vec<Codec>, Encoder)> {
        let mut failed = None;
        for candidate in self.candidates() {
            let mut offers = vec![];
            let mut first = None;
            for codec in GPU_CODECS {
                if let Ok(encoder) = self.open_on(codec, width, height, &candidate.device) {
                    offers.push(codec);
                    first.get_or_insert(encoder);
                }
            }
            let first = match first {
                Some(encoder) => Some(encoder),
                None => (self.open_on(
                    stream_connection::SEND_CODEC,
                    width,
                    height,
                    &candidate.device,
                ))
                .ok(),
            };
            if let Some(encoder) = first {
                offers.push(stream_connection::SEND_CODEC);
                self.place(Placed::On(candidate), failed.as_ref());
                log_encoder(&encoder);
                return Ok((offers, encoder));
            }
            failed.get_or_insert(candidate);
        }
        self.place(Placed::Processor, failed.as_ref());
        let encoder = self.open_on(
            stream_connection::SEND_CODEC,
            width,
            height,
            &Device::Processor,
        )?;
        log_encoder(&encoder);
        Ok((vec![stream_connection::SEND_CODEC], encoder))
    }

    /// An encoder for the stream in `codec`, as [`Self::open`] opens it.
    fn encoder(&mut self, codec: Codec, width: u32, height: u32) -> Result<Encoder> {
        let encoder = self.open(codec, width, height)?;
        log_encoder(&encoder);
        Ok(encoder)
    }

    /// An encoder in `codec` where the stream encodes: on its adapter, and
    /// when that fails, H264 on the next candidate that opens it, then on
    /// the processor; AV1 and H265 have no other place, as the stream's
    /// codecs all come from one. After a change in Settings, on the first
    /// candidate that opens it.
    fn open(&mut self, codec: Codec, width: u32, height: u32) -> Result<Encoder> {
        let mut failed = None;
        match self.placed.clone() {
            Placed::On(candidate) => {
                let opened = self.open_on(codec, width, height, &candidate.device);
                match opened {
                    Ok(encoder) => return Ok(encoder),
                    Err(error) if codec != stream_connection::SEND_CODEC => return Err(error),
                    Err(_) => failed = Some(candidate),
                }
            }
            Placed::Processor => return self.open_on(codec, width, height, &Device::Processor),
            Placed::Anywhere => {}
        }
        for candidate in self.candidates() {
            if failed.as_ref() == Some(&candidate) {
                continue;
            }
            match self.open_on(codec, width, height, &candidate.device) {
                Ok(encoder) => {
                    self.place(Placed::On(candidate), failed.as_ref());
                    return Ok(encoder);
                }
                Err(_) => {
                    failed.get_or_insert(candidate);
                }
            }
        }
        if codec != stream_connection::SEND_CODEC {
            bail!("no graphics card here encodes {}", codec.name());
        }
        self.place(Placed::Processor, failed.as_ref());
        self.open_on(codec, width, height, &Device::Processor)
    }

    /// An encoder in `codec` on `device`, at the stream's target, or what
    /// a stream of `height` lines starts at before it has one; never above
    /// the cap of `height` lines and the bitrate limit, which the stream's
    /// target follows once a frame of the new size goes out. With the
    /// unlock past 9 Mbit/s it opens holding that cap ([`Self::reach`]).
    fn open_on(&self, codec: Codec, width: u32, height: u32, device: &Device) -> Result<Encoder> {
        let (max_kbps, unlocked) = (self.max_kbps, self.bitrate_unlocked);
        let target = self.video.as_ref().map_or_else(
            || congestion::start(stream_connection::cap(height, max_kbps, unlocked).into()),
            |video| video.sender.target(),
        );
        Encoder::holding(
            codec,
            width,
            height,
            self.fps,
            aim(target, height, max_kbps, unlocked),
            self.reach(height),
            self.effort,
            device,
        )
    }

    /// The kbit/s an encoder for frames `height` lines tall opens holding,
    /// so the level its stream signals takes what the stream may rise to:
    /// the cap, where the unlock takes it past Discord's documented
    /// 9 Mbit/s; else 0, as every level a stream's size takes holds 9 and
    /// encoders open at their aim, as they always have.
    fn reach(&self, height: u32) -> u32 {
        let cap = stream_connection::cap(height, self.max_kbps, self.bitrate_unlocked);
        if cap > stream_connection::DOCUMENTED_MAX {
            cap / 1000
        } else {
            0
        }
    }

    /// The graphics encoders to try for the stream, best first, read once
    /// for each choice in Settings.
    fn candidates(&mut self) -> Vec<Candidate> {
        if self.candidates.is_none() {
            let (candidates, missing) = candidates_for(&self.encode_on);
            if missing {
                tracing::info!(on = ?self.encode_on, "the graphics card chosen for streams isn't there");
            }
            self.missing = missing;
            self.candidates = Some(candidates);
        }
        self.candidates.clone().unwrap_or_default()
    }

    /// Encodes on `placed` from now on, and says in the dock how that isn't
    /// what Settings asked for: the adapter chosen isn't there, or `failed`,
    /// tried before, didn't open. The worker's cores stay those chosen
    /// wherever it encodes.
    fn place(&mut self, placed: Placed, failed: Option<&Candidate>) {
        let missing = std::mem::take(&mut self.missing);
        let failed = failed.and_then(|candidate| candidate.name.as_deref());
        let notice = match (&placed, missing, failed) {
            (Placed::On(Candidate { name: Some(used), .. }), true, _) => Some(tr!(
                "The graphics card chosen for streams isn't there, so {} encodes this one.",
                used
            )),
            (Placed::On(_), true, _) => Some(
                tr!("The graphics card chosen for streams isn't there, so another one encodes this one.")
                    .to_owned(),
            ),
            (Placed::Processor, true, _) => Some(
                tr!("The graphics card chosen for streams isn't there, so the processor encodes this one.")
                    .to_owned(),
            ),
            (Placed::On(Candidate { name: Some(used), .. }), false, Some(failed)) => Some(tr!(
                "{} couldn't encode the stream, so {} encodes it.",
                failed,
                used
            )),
            (Placed::Processor, false, Some(failed)) => Some(tr!(
                "{} couldn't encode the stream, so the processor encodes it.",
                failed
            )),
            _ => None,
        };
        if let Some(notice) = notice {
            self.say(notice);
        }
        self.placed = placed;
    }

    /// Has the dock say `notice`, after what it hasn't said yet.
    fn say(&mut self, notice: String) {
        tracing::info!(
            notice = notice.as_str(),
            "telling Yapfast how the stream encodes"
        );
        self.notice = Some(match self.notice.take() {
            Some(said) => format!("{said} {notice}"),
            None => notice,
        });
    }

    /// Takes Yapfast's controls, reports the stream starting, and fails once
    /// its connection ended. Gives the source a [`Control::Switch`] asked for.
    fn check(&mut self, report: &mut dyn FnMut(Report)) -> Result<Option<Source>> {
        let mut switch = None;
        while let Ok(control) = self.controls.try_recv() {
            match control {
                Control::Send(server) => self.server = Some(server),
                Control::Switch(source) => switch = Some(source),
                // The next frame fits the new bounds and reopens the
                // encoder; Discord hears of the frame rate and the bitrate
                // limit with it, and the encoder follows the limit without
                // opening again.
                Control::Quality {
                    width,
                    height,
                    fps,
                    effort,
                    max_kbps,
                } => {
                    self.bounds = (width, height);
                    self.fps = fps.clamp(1, 60);
                    self.effort = effort;
                    self.max_kbps = max_kbps;
                    if let Some(video) = &self.video {
                        video.sender.set_fps(self.fps);
                        video.sender.set_ceiling(max_kbps, self.bitrate_unlocked);
                    }
                }
                // As a new limit: the target drops under Discord's cap at
                // once, or may rise past it as the uplink allows; an
                // encoder whose level wouldn't hold the new cap opens again
                // with the next frame.
                Control::BitrateUnlocked(on) if on != self.bitrate_unlocked => {
                    tracing::info!(on, "the bitrate limit's unlock changed");
                    self.bitrate_unlocked = on;
                    if let Some(video) = &self.video {
                        video.sender.set_ceiling(self.max_kbps, on);
                    }
                }
                // The encoder closes, and the next frame opens one where
                // Settings now says, with a keyframe.
                Control::EncodeOn(on) if on != self.encode_on => {
                    tracing::info!(?on, "encoding the stream elsewhere");
                    self.encode_on = on;
                    (self.candidates, self.missing) = (None, false);
                    self.placed = Placed::Anywhere;
                    if let Some(video) = self.video.as_mut() {
                        video.encoder = None;
                    }
                }
                // The sound's thread makes the change, so the picture never
                // waits for it.
                Control::Audio(audio) if self.sound.is_some() => {
                    self.sound = Some(audio);
                    if let Some(video) = self.video.as_mut() {
                        match &video.sound {
                            Some(sound) => sound.hear(audio),
                            None if audio != StreamAudio::Off => {
                                video.sound =
                                    share_sound(&video.sender, audio, &mut self.sound_failed);
                            }
                            None => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let failed = self.sound_failed.take().or_else(|| {
            self.video
                .as_ref()
                .and_then(|video| video.sound.as_ref())
                .and_then(stream_audio::Capture::failure)
        });
        if let Some(error) = failed {
            report(Report::AudioError(error));
        }
        if let Some(notice) = self.notice.take() {
            report(Report::Notice(notice));
        }
        match self.video.as_ref().map(|video| video.sender.link()) {
            Some(Link::Connected) if !self.streaming => {
                self.streaming = true;
                self.preview_due = Some(Instant::now());
                report(Report::Streaming);
            }
            // Discord ends the connection when it moves the stream to
            // another server, which then arrives as another Send.
            Some(Link::Ended(reason)) => {
                let lost = *self.lost.get_or_insert_with(Instant::now);
                if lost.elapsed() >= MOVE_GRACE {
                    bail!(tr!("Lost the stream connection: {}", reason));
                }
            }
            _ => self.lost = None,
        }
        if let Some(thumbnail) = self.thumbnail.take() {
            report(Report::Preview { thumbnail });
        }
        if let Some(video) = self.video.as_mut()
            && self.streaming
            && video.measured.0.elapsed() >= SENDING_EVERY
            && let Some(sending) = video.sending(self.fps)
        {
            report(Report::Sending(sending));
        }
        Ok(switch)
    }
}

/// Starts the stream's sound on `sender`, or keeps why it couldn't start.
fn share_sound(
    sender: &StreamSender,
    audio: StreamAudio,
    failed: &mut Option<String>,
) -> Option<stream_audio::Capture> {
    stream_audio::Capture::start(sender.sound(), audio)
        .inspect_err(|error| {
            tracing::warn!(error = format!("{error:#}"), "sharing without sound");
            *failed = Some(format!("{error:#}"));
        })
        .ok()
}

/// Whether a picture is black, as a screen is before what's shared draws:
/// Discord's client waits for one that isn't before sending a preview.
fn is_black(picture: &I420Buffer) -> bool {
    let (y, _, _) = picture.data();
    // Black's luma is 16; some of a picture of anything is brighter.
    y.iter().step_by(7).all(|&luma| luma <= 24)
}

/// A preview of a picture as Discord's client makes one: scaled down to fit
/// 512 by 288, a JPEG as a data URI.
fn thumbnail(picture: &mut I420Buffer) -> Result<String> {
    use base64::Engine;
    use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};
    let (width, height) = fit(
        picture.width(),
        picture.height(),
        THUMBNAIL_WIDTH,
        THUMBNAIL_HEIGHT,
    );
    let small = picture.scale(width as i32, height as i32);
    let mut rgba = vec![0; (width * height * 4) as usize];
    to_rgba(&small, &mut rgba);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let mut jpeg = vec![];
    JpegEncoder::new_with_quality(&mut jpeg, 92).encode(
        &rgb,
        width,
        height,
        ExtendedColorType::Rgb8,
    )?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    ))
}

/// How a watched stream's sound plays, and Yapfast's word on hearing it.
struct Sound {
    speakers: Option<String>,
    hearing: Arc<AtomicBool>,
    volume: Arc<stream_audio::Volume>,
    /// How it's cleaned, which the player reads once a frame.
    cleaning: Arc<crate::stream_cleaning::Chosen>,
    controls: channel::Receiver<Control>,
}

/// Writes a stream's picture into the shared frame, turned upright as its
/// sender said and scaled down if it doesn't fit; returns the size shown.
fn show_stream_picture(
    writer: &mut Writer,
    picture: I420<'_>,
    turn: Turn,
    maximum: (u32, u32),
    scratch: &mut Vec<u8>,
    strip: &mut Vec<u8>,
) -> (u32, u32) {
    let (width, height) = turn.upright(picture.width, picture.height);
    let size = if writer.fits(width, height) && width <= maximum.0 && height <= maximum.1 {
        (width, height)
    } else {
        fit(width, height, maximum.0, maximum.1)
    };
    writer.write(size.0, size.1, |pixels| {
        picture.to_rgba(scratch, strip, pixels, size, turn);
    });
    size
}

/// Shows someone's stream: whole frames from the connection, decoded, then
/// scaled down if they don't fit, and plays its sound.
fn watch(
    server: StreamServer,
    mut writer: Writer,
    (max_width, max_height): (u32, u32),
    sound: Sound,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    let mut receiver = StreamReceiver::start(server, crate::decode::DECODES.to_vec())?;
    let _player = receiver.take_sounds().and_then(|sounds| {
        stream_audio::Player::start(
            sounds,
            sound.speakers,
            sound.hearing.clone(),
            sound.volume.clone(),
            sound.cleaning.clone(),
        )
        .inspect_err(|error| tracing::warn!(error = format!("{error:#}"), "watching without sound"))
        .ok()
    });
    let mut decoder: Option<Decoder> = None;
    // A frame that doesn't fit scales through `scratch`, a picture of the
    // size shown; one that does goes straight into the shared frame. One a
    // phone sends turned goes through `strip`, a few rows of it.
    let (mut scratch, mut strip) = (vec![], vec![]);
    let mut shown = false;
    let mut failures = 0u64;
    while !stop.load(Ordering::Relaxed) {
        while let Ok(control) = sound.controls.try_recv() {
            match control {
                Control::Hear(hearing) => sound.hearing.store(hearing, Ordering::Relaxed),
                Control::Volume(volume) => sound.volume.set(volume),
                Control::Cleaning(cleaning) => sound.cleaning.set(cleaning),
                _ => {}
            }
        }
        if let Link::Ended(reason) = receiver.link() {
            bail!(tr!("Lost the stream: {}", reason));
        }
        let Some(frame) = receiver.next(Duration::from_millis(100)) else {
            continue;
        };
        if decoder
            .as_ref()
            .is_none_or(|decoder| decoder.codec() != frame.codec)
        {
            decoder = Some(Decoder::new(frame.codec)?);
        }
        let Some(decoder) = decoder.as_mut() else {
            continue;
        };
        let picture = match decoder.decode(&frame.data) {
            Ok(Some(picture)) => picture,
            Ok(None) => continue,
            Err(error) => {
                failures += 1;
                // The first of a run, and then now and again.
                if failures == 1 || failures.is_multiple_of(100) {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        failures,
                        keyframe = frame.keyframe,
                        size = frame.data.len(),
                        "a stream frame didn't decode"
                    );
                }
                receiver.request_keyframe();
                continue;
            }
        };
        let (width, height) = show_stream_picture(
            &mut writer,
            picture.i420(),
            Turn {
                turns: frame.orientation.turns,
                flip: frame.orientation.flip,
            },
            (max_width, max_height),
            &mut scratch,
            &mut strip,
        );
        if !shown {
            shown = true;
            tracing::info!(width, height, codec = ?frame.codec, "showing the stream");
            report(Report::Streaming);
        }
    }
    Ok(())
}

/// Sleeps until the next frame is due, or returns false once told to stop.
fn next_frame(started: Instant, frame: u64, fps: u32, stop: &AtomicBool) -> bool {
    let due = started + Duration::from_secs(frame) / fps;
    if let Some(wait) = due.checked_duration_since(Instant::now()) {
        std::thread::sleep(wait);
    }
    !stop.load(Ordering::Relaxed)
}

/// Colour bars at the sink's bounds, 640 by 360 at most, which follow a
/// change of quality as a screen's frames do.
fn test_pattern(mut sink: Sink, stop: &AtomicBool, report: &mut dyn FnMut(Report)) -> Result<()> {
    let (mut width, mut height) = (0, 0);
    let (mut rgba, mut picture) = (Vec::new(), I420Buffer::new(2, 2));
    let (mut started, mut fps) = (Instant::now(), sink.fps);
    let (mut frame, mut drawn) = (0u64, 0u64);
    while next_frame(started, frame, fps, stop) {
        let bounds = (sink.bounds.0.min(640), sink.bounds.1.min(360));
        if bounds != (width, height) {
            (width, height) = bounds;
            rgba = vec![0; (width * height * 4) as usize];
            picture = I420Buffer::new(width, height);
            report(Report::Sharing { width, height });
        }
        let shift = (drawn * 4) as u32;
        for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            let bar = ((x + shift) * 8 / width) % 8;
            let level = 255 - (y * 128 / height) as u8;
            let on = |bit: u32| if bar & bit != 0 { level } else { 32 };
            pixel.copy_from_slice(&[on(4), on(2), on(1), 255]);
        }
        let (y_stride, u_stride, v_stride) = picture.strides();
        let (y, u, v) = picture.data_mut();
        // libyuv names formats by their order in a little-endian word: its
        // ABGR is R, G, B, A in memory.
        yuv_helper::abgr_to_i420(
            &rgba,
            width * 4,
            y,
            y_stride,
            u,
            u_stride,
            v,
            v_stride,
            width as i32,
            height as i32,
        );
        sink.frame(&mut picture)?;
        // Colour bars are all it shares.
        let _switch = sink.check(report)?;
        (frame, drawn) = (frame + 1, drawn + 1);
        if sink.fps != fps {
            (started, fps, frame) = (Instant::now(), sink.fps, 0);
        }
    }
    Ok(())
}

/// The kbit/s to encode frames of `height` lines at, for a stream aiming
/// at `target` bit/s: never above that size's cap, nor the bitrate limit
/// `max_kbps`, `unlocked` or not ([`stream_connection::cap`]). After a
/// smaller size or a lower limit the stream's target follows the new cap
/// only once its next frame goes out, and the encoder would otherwise go
/// back up to the old one.
fn aim(target: u64, height: u32, max_kbps: Option<u32>, unlocked: bool) -> u32 {
    (target.min(stream_connection::cap(height, max_kbps, unlocked).into()) / 1000) as u32
}

/// How hard the encoder works at `quality`.
fn effort(quality: StreamQuality) -> Effort {
    if quality.high_quality {
        Effort::High
    } else {
        Effort::Light
    }
}

/// The largest frame shared at `resolution`; screens of another shape fit
/// inside it, as [`fit`] scales them: a 3440 by 1440 ultrawide goes out at
/// 1440p as 2560 by 1070, its width filling the bounds.
fn bounds(resolution: Resolution) -> (u32, u32) {
    match resolution {
        Resolution::P720 => (1280, 720),
        Resolution::P1080 => (SHARE_WIDTH, SHARE_HEIGHT),
        Resolution::P1440 => (2560, 1440),
        Resolution::Source => (3840, 2160),
    }
}

/// The size `width` by `height` takes when scaled down to fit `max_width`
/// by `max_height`, keeping its shape. Codecs want even sizes.
pub fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(width.max(1)),
            f64::from(max_height) / f64::from(height.max(1)),
        ),
    );
    let even = |value: f64| ((value as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

/// Converts an I420 picture into RGBA pixels of its size.
fn to_rgba(picture: &I420Buffer, pixels: &mut [u8]) {
    let (y, u, v) = picture.data();
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (width, height) = (picture.width(), picture.height());
    yuv_helper::i420_to_abgr(
        y,
        y_stride,
        u,
        u_stride,
        v,
        v_stride,
        pixels,
        width * 4,
        width as i32,
        height as i32,
    );
}

// libwebrtc asks the screen sharing portal over D-Bus through GIO, but
// webrtc-sys only compiles against its headers.
#[cfg(target_os = "linux")]
#[link(name = "gio-2.0")]
#[link(name = "gobject-2.0")]
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_main_context_iteration(
        context: *mut std::ffi::c_void,
        may_block: std::ffi::c_int,
    ) -> std::ffi::c_int;
}

/// Runs what's due on GLib's main context. The portal's D-Bus replies arrive
/// there, and nothing else runs it in this process: without this the
/// dialog never opens.
fn run_glib() {
    #[cfg(target_os = "linux")]
    // SAFETY: the default context; only the capture thread iterates it.
    while unsafe { g_main_context_iteration(std::ptr::null_mut(), 0) } != 0 {}
}

#[derive(Default)]
struct Captured {
    size: Option<(u32, u32)>,
    /// Capture stopped for good, or a frame couldn't be sent.
    failed: Option<String>,
}

fn capture_screen(
    source: Source,
    sink: Sink,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    // The frame rate and bounds shared at, which `Control::Quality` changes.
    let mut tuned = (sink.fps, sink.bounds);
    let sink = Arc::new(Mutex::new(sink));
    let mut capture = Capture::start(source, &sink)?;
    // A screen, window or camera to share in its place, until it gives a
    // picture.
    let mut switching: Option<Capture> = None;
    let mut started = Instant::now();
    let mut frame = 0u64;
    let mut reported = None;
    while next_frame(started, frame, tuned.0, stop) {
        run_glib();
        capture.capture_frame();
        if let Some(next) = &mut switching {
            next.capture_frame();
        }
        frame += 1;
        let seen = capture.seen();
        if let Some(failure) = &seen.failed {
            bail!(if reported.is_none() && portal_picks() {
                tr!("Screen sharing was cancelled, or this desktop has no screen sharing dialog.")
                    .to_owned()
            } else {
                failure.clone()
            });
        }
        if seen.size != reported
            && let Some((width, height)) = seen.size
        {
            reported = seen.size;
            report(Report::Sharing { width, height });
        }
        drop(seen);
        if let Some(next) = &switching {
            let seen = next.seen();
            if let Some(failure) = &seen.failed {
                tracing::info!(failure, "sharing the same screen or window");
                drop(seen);
                switching = None;
            } else if seen.size.is_some() {
                drop(seen);
                capture = switching.take().expect("switching");
                // Discord shows what's shared now.
                let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if sink.streaming {
                    sink.preview_due = Some(Instant::now());
                }
            }
        }
        let (switch, wanted) = {
            let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            (sink.check(report)?, (sink.fps, sink.bounds))
        };
        if wanted != tuned {
            // A new frame rate counts its frames afresh.
            if wanted.0 != tuned.0 {
                (started, frame) = (Instant::now(), 0);
            }
            tuned = wanted;
            // A camera films at the size and rate it opened with: it opens
            // again at the new ones, once the old one has let go of it.
            if let Some((id, camera)) = capture.camera.take() {
                drop(camera);
                capture = Capture::camera(&id, &sink)?;
            }
        }
        if let Some(source) = switch {
            // A switch asked for while another waits replaces it.
            switching = None;
            match Capture::start(source, &sink) {
                Ok(next) => switching = Some(next),
                Err(error) => tracing::warn!(
                    error = format!("{error:#}"),
                    "sharing the same screen or window"
                ),
            }
        }
    }
    Ok(())
}

/// The largest frame the sink takes now.
fn bounds_of(sink: &Mutex<Sink>) -> (u32, u32) {
    sink.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .bounds
}

/// A capturer sending a screen's, window's or camera's frames to the sink.
struct Capture {
    /// A screen's or window's, which gives a frame when asked.
    capturer: Option<livekit::webrtc::desktop_capturer::DesktopCapturer>,
    /// A camera's by its id, which sends its frames from its own thread.
    camera: Option<(String, crate::camera::Camera)>,
    captured: Arc<Mutex<Captured>>,
}

impl Capture {
    /// Starts capturing `source`, fitting each frame in the sink's bounds
    /// as it comes; where [`portal_picks`], a screen or window the system's
    /// dialog picks.
    fn start(source: Source, sink: &Arc<Mutex<Sink>>) -> Result<Self> {
        if let Source::Camera(id) = &source {
            return Self::camera(id, sink);
        }
        use livekit::webrtc::desktop_capturer::{
            CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
        };
        let portal = portal_picks();
        let kind = match source {
            // The system's screen sharing dialog picks one of a kind: under
            // Wayland libwebrtc's screen and window capturers are the
            // portal's, asking it for screens or for windows alone.
            Source::PortalWindow => DesktopCaptureSourceType::Window,
            Source::PortalScreen => DesktopCaptureSourceType::Screen,
            // The system's screen sharing dialog picks a screen or window.
            // libwebrtc has the generic capturer only on Linux and macOS, and
            // `portal_picks` is false elsewhere.
            #[cfg(target_os = "linux")]
            _ if portal => DesktopCaptureSourceType::Generic,
            Source::Window(_) => DesktopCaptureSourceType::Window,
            _ => DesktopCaptureSourceType::Screen,
        };
        let mut options = DesktopCapturerOptions::new(kind);
        options.set_include_cursor(true);
        let mut capturer = DesktopCapturer::new(options)
            .context(tr!("Screen sharing isn't available on this system."))?;
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (into, seen) = (sink.clone(), captured.clone());
        let source = if portal {
            None
        } else {
            let mut listed = capturer.get_source_list().into_iter();
            let found = match source {
                Source::Display(id) | Source::Window(id) => listed.find(|listed| listed.id() == id),
                _ => listed.next(),
            };
            Some(found.context(tr!("That screen or window is gone."))?)
        };
        capturer.start_capture(source, move |result| {
            let mut seen = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match result {
                Ok(frame) => {
                    let mut picture = picture_of(&frame);
                    let (width, height) = (picture.width(), picture.height());
                    let (max_width, max_height) = bounds_of(&into);
                    let size = fit(width, height, max_width, max_height);
                    if size != (width, height) {
                        picture = picture.scale(size.0 as i32, size.1 as i32);
                    }
                    let taken = into
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(&mut picture);
                    match taken {
                        Ok(()) => seen.size = Some(size),
                        Err(error) => seen.failed = Some(format!("{error:#}")),
                    }
                }
                // While the dialog is open, or between frames.
                Err(CaptureError::Temporary) => {}
                Err(CaptureError::Permanent) => {
                    seen.failed
                        .get_or_insert_with(|| tr!("Screen sharing stopped.").into());
                }
            }
        });
        Ok(Self {
            capturer: Some(capturer),
            camera: None,
            captured,
        })
    }

    /// Films with the camera `id` into the sink, at the stream's size and
    /// frame rate at most; a new quality opens it again (`capture_screen`).
    /// Each picture goes into a buffer kept for the next.
    fn camera(id: &str, sink: &Arc<Mutex<Sink>>) -> Result<Self> {
        let ((max_width, max_height), fps) = {
            let sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            (sink.bounds, sink.fps)
        };
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (into, seen) = (sink.clone(), captured.clone());
        let mut kept: Option<I420Buffer> = None;
        let camera = crate::camera::Camera::open_at(
            Some(id),
            (max_width, max_height, fps),
            Box::new(move |frame| {
                let size = (frame.width, frame.height);
                if kept
                    .as_ref()
                    .is_none_or(|picture| (picture.width(), picture.height()) != size)
                {
                    kept = Some(I420Buffer::new(frame.width, frame.height));
                }
                let picture = kept.as_mut().expect("made above");
                copy_planes(frame, picture);
                // The camera fits the stream already, but for frames taken
                // as a new quality arrives; an odd size, which codecs can't
                // take, loses a row or a column.
                let (max_width, max_height) = bounds_of(&into);
                let fitted = fit(size.0, size.1, max_width, max_height);
                let taken = if fitted == size {
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(picture)
                } else {
                    let mut scaled = picture.scale(fitted.0 as i32, fitted.1 as i32);
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(&mut scaled)
                };
                let mut seen = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match taken {
                    Ok(()) => seen.size = Some(fitted),
                    Err(error) => seen.failed = Some(format!("{error:#}")),
                }
            }),
        )?;
        Ok(Self {
            capturer: None,
            camera: Some((id.to_owned(), camera)),
            captured,
        })
    }

    /// Asks a screen's or window's capturer for a frame; a camera sends its
    /// own.
    fn capture_frame(&mut self) {
        if let Some(capturer) = &mut self.capturer {
            capturer.capture_frame();
        }
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, Captured> {
        self.captured
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Copies a camera's picture into `picture`, of the same size.
fn copy_planes(frame: &crate::picture::I420<'_>, picture: &mut I420Buffer) {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
    let (y_stride, u_stride, v_stride) = picture.strides();
    let into_strides = [y_stride, u_stride, v_stride];
    let (y, u, v) = picture.data_mut();
    for (plane, (from, into)) in [(frame.y, y), (frame.u, u), (frame.v, v)]
        .into_iter()
        .enumerate()
    {
        let (width, height) = if plane == 0 {
            (width, height)
        } else {
            (chroma_width, chroma_height)
        };
        let rows = from
            .chunks(frame.strides[plane] as usize)
            .zip(into.chunks_mut(into_strides[plane] as usize));
        for (from, into) in rows.take(height) {
            into[..width].copy_from_slice(&from[..width]);
        }
    }
}

/// A captured frame as an I420 picture of its size.
fn picture_of(frame: &livekit::webrtc::desktop_capturer::DesktopFrame) -> I420Buffer {
    let (width, height) = (frame.width().max(1) as u32, frame.height().max(1) as u32);
    let mut picture = I420Buffer::new(width, height);
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (y, u, v) = picture.data_mut();
    // libyuv's ARGB is B, G, R, A in memory, as the capturer's frames are.
    yuv_helper::argb_to_i420(
        frame.data(),
        frame.stride(),
        y,
        y_stride,
        u,
        u_stride,
        v,
        v_stride,
        width as i32,
        height as i32,
    );
    picture
}

/// How long [`glance`] waits for a picture of a screen that isn't black,
/// before showing the last it took, or none.
const GLANCE_WAIT: Duration = Duration::from_millis(1500);

/// Takes a picture of each of `screens`, as [`Job::Glance`] asks. A screen
/// that gives none goes without, and the rest still come.
fn glance(
    screens: &[u64],
    max_width: u32,
    max_height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) {
    for &id in screens {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match glance_at(id, stop) {
            Ok(Some(mut picture)) => {
                let (width, height) = fit(picture.width(), picture.height(), max_width, max_height);
                if (width, height) != (picture.width(), picture.height()) {
                    picture = picture.scale(width as i32, height as i32);
                }
                let mut rgba = vec![0; (width * height * 4) as usize];
                to_rgba(&picture, &mut rgba);
                report(Report::Screen {
                    id,
                    width,
                    height,
                    rgba,
                });
            }
            Ok(None) => tracing::info!(id, "no picture of the screen came"),
            Err(error) => tracing::warn!(id, "no picture of the screen: {error:#}"),
        }
    }
}

/// A picture of the screen listed by `id`, at its own size.
fn glance_at(id: u64, stop: &AtomicBool) -> Result<Option<I420Buffer>> {
    use livekit::webrtc::desktop_capturer::{
        CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
    };
    let mut capturer = DesktopCapturer::new(DesktopCapturerOptions::new(
        DesktopCaptureSourceType::Screen,
    ))
    .context(tr!("Screen sharing isn't available on this system."))?;
    let source = capturer
        .get_source_list()
        .into_iter()
        .find(|listed| listed.id() == id)
        .context(tr!("That screen is gone."))?;
    // The last picture taken, and whether capture stopped for good.
    let taken = Arc::new(Mutex::new((None::<I420Buffer>, false)));
    let into = taken.clone();
    capturer.start_capture(Some(source), move |result| {
        let mut taken = into.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match result {
            Ok(frame) => taken.0 = Some(picture_of(&frame)),
            Err(CaptureError::Temporary) => {}
            Err(CaptureError::Permanent) => taken.1 = true,
        }
    });
    // A screen's first picture can come black, before the system has
    // drawn it for the capturer.
    let deadline = Instant::now() + GLANCE_WAIT;
    loop {
        capturer.capture_frame();
        let mut taken = taken
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let lit = taken.0.as_ref().is_some_and(|picture| !is_black(picture));
        if lit || taken.1 || Instant::now() >= deadline || stop.load(Ordering::Relaxed) {
            return Ok(taken.0.take());
        }
        drop(taken);
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Pixels in a report as base64: a third bigger than they are, where JSON's
/// array of numbers would be up to four times.
mod base64_pixels {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(pixels: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(pixels))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        // base64 has nothing JSON escapes, so the line lends it as it is.
        let text = <&str>::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

/// Where Settings' "Encode streams on" puts a stream's encoder, as
/// `layout.json`'s `stream_encoder` keeps it: `"auto"`, `"software"`, or an
/// adapter's [`Adapter::key`] such as `"pci:0000:73:00.0"`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodeOn {
    /// The adapters in [`adapters::encoder_order`]'s order, the first of
    /// which is [`adapters::auto_encoder`]'s, then the processor.
    #[default]
    Auto,
    /// The processor: x264, which encodes H264 alone.
    Software,
    /// This adapter, then Auto's order, then the processor.
    Adapter(String),
}

impl EncodeOn {
    /// The choice `stream_encoder` names; Auto for anything else.
    pub fn parse(setting: &str) -> Self {
        match setting {
            "software" => Self::Software,
            key if key.starts_with("pci:") || key.starts_with("luid:") => {
                Self::Adapter(key.to_owned())
            }
            _ => Self::Auto,
        }
    }
}

/// Where streams encode: Settings' choice, Auto until the UI says.
static ENCODE_ON: Mutex<EncodeOn> = Mutex::new(EncodeOn::Auto);

/// Encodes streams where `setting` says, as `layout.json`'s
/// `stream_encoder` keeps it ([`EncodeOn::parse`]): those started from now
/// on, and those going on, which open their encoder there with the next
/// frame without stopping.
pub fn set_encode_on(setting: &str) {
    let on = EncodeOn::parse(setting);
    let changed = {
        let mut chosen = ENCODE_ON
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::replace(&mut *chosen, on.clone()) != on
    };
    if changed {
        tell_running(&Control::EncodeOn(on));
    }
}

/// Encodes streams on a graphics card, as Auto picks it, or on the
/// processor: the Slint window's switch, which [`set_encode_on`] replaces.
pub fn set_hardware_encoding(on: bool) {
    set_encode_on(if on { "auto" } else { "software" });
}

/// Whether a stream's bitrate limit may pass Discord's documented
/// 9 Mbit/s: Settings' "Unlocked bitrate", off until the UI says.
static BITRATE_UNLOCKED: AtomicBool = AtomicBool::new(false);

/// Lets streams' bitrate limit past Discord's documented 9 Mbit/s, up to
/// 50 ([`stream_connection::cap`]), or holds it to 9 again, as `layout.json`'s
/// `stream_bitrate_unlocked` keeps it: for those started from now on, and
/// those going on, from their next frame. A limit kept past 9 Mbit/s is
/// left as it is, and counts as 9 while it's off.
pub fn unlock_bitrate(on: bool) {
    if BITRATE_UNLOCKED.swap(on, Ordering::Relaxed) != on {
        tell_running(&Control::BitrateUnlocked(on));
    }
}

/// Whether [`unlock_bitrate`] let the limit past 9 Mbit/s.
pub fn bitrate_unlocked() -> bool {
    BITRATE_UNLOCKED.load(Ordering::Relaxed)
}

/// The logical CPUs Settings chose for Yapfast, which its workers run on
/// too; none for every one.
static CPUS: Mutex<Option<CpuMask>> = Mutex::new(None);

/// Runs the workers on `mask`, as [`crate::cpu::choose`] gave it, or on
/// every CPU Yapfast started with when none: those started from now on, and
/// those running, at once. A stream x264 encodes stays on them too, its
/// threads as many as they give ([`crate::cpu::threads`]); where it can't
/// keep up there, the dock says so.
pub fn set_cpus(mask: Option<CpuMask>) {
    let changed = {
        let mut chosen = CPUS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::replace(&mut *chosen, mask) != mask
    };
    if changed {
        tell_running(&Control::Cpus(mask));
    }
}

/// The CPUs [`set_cpus`] chose, for a worker starting.
fn chosen_cpus() -> Option<CpuMask> {
    *CPUS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The lines a worker starts with: its job, then the CPUs it runs on,
/// `cpus` as [`set_cpus`] chose them, whatever the job and wherever it
/// encodes. Every CPU, for none, is said too: a child takes its parent's
/// cores on Linux, but may not on Windows.
fn opening(job: &Job, cpus: Option<CpuMask>) -> Result<String> {
    Ok(format!(
        "{}\n{}\n",
        serde_json::to_string(job)?,
        serde_json::to_string(&Control::Cpus(cpus))?
    ))
}

/// The inputs of the workers running, for what Settings changes while they
/// run: their cores, and where a stream encodes. A worker's goes when it's
/// dropped.
static RUNNING: Mutex<Vec<Weak<Mutex<ChildStdin>>>> = Mutex::new(Vec::new());

/// Tells every running worker `control`, forgetting those gone.
fn tell_running(control: &Control) {
    let Ok(line) = serde_json::to_string(control) else {
        return;
    };
    let mut running = RUNNING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    running.retain(|input| {
        let Some(input) = input.upgrade() else {
            return false;
        };
        let mut input = input
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        writeln!(input, "{line}").is_ok()
    });
}

/// Ticks of what the call sent that wait for the echo feed's thread: 2 s.
const ECHO_QUEUE: usize = 100;

/// What the call sends, on its way to a watching worker's echo gate
/// ([`Worker::echo_feed`]).
pub struct EchoFeed {
    ticks: std::sync::mpsc::SyncSender<crate::echo_gate::Ours>,
}

impl EchoFeed {
    /// Hands the feed what the call sent this tick, never waiting: a tick
    /// the feed has no room for goes. Whether its thread is still there.
    pub fn send(&self, ours: crate::echo_gate::Ours) -> bool {
        !matches!(
            self.ticks.try_send(ours),
            Err(std::sync::mpsc::TrySendError::Disconnected(_))
        )
    }
}

/// Yapfast's handle on a running worker. Dropping it stops the worker.
pub struct Worker {
    /// Shared with the thread reading its reports, which waits on it when
    /// it ends without saying so.
    child: Arc<Mutex<Child>>,
    /// Shared with [`RUNNING`], which holds it weakly.
    stdin: Option<Arc<Mutex<ChildStdin>>>,
}

impl Worker {
    /// Starts sharing `source`, with frames for the UI in a new [`Frames`].
    pub fn share(
        source: Source,
        quality: StreamQuality,
        audio: StreamAudio,
    ) -> Result<(Self, Frames, mpsc::UnboundedReceiver<Report>)> {
        let source = picked(source);
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT)?;
        let (width, height) = bounds(quality.resolution);
        let encode_on = ENCODE_ON
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let job = Job::Share {
            source,
            audio,
            frames: frames.path().context("the video frames file has no name")?,
            width,
            height,
            fps: quality.fps,
            effort: effort(quality),
            encode_on,
            max_kbps: quality.max_kbps,
            bitrate_unlocked: bitrate_unlocked(),
        };
        let (worker, reports) = Self::start(&job)?;
        Ok((worker, frames, reports))
    }

    /// Starts taking pictures of `screens` for the share menu, each fitting
    /// `width` by `height`. They come as [`Report::Screen`]s, then
    /// [`Report::Ended`].
    pub fn glance(
        screens: Vec<u64>,
        width: u32,
        height: u32,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Report>)> {
        Self::start(&Job::Glance {
            screens,
            width,
            height,
        })
    }

    /// Starts watching the stream at `server`, with pictures for the UI in a
    /// new [`Frames`], its sound at `volume` and cleaned as `cleaning` says.
    pub fn watch(
        server: &StreamServer,
        speakers: Option<String>,
        deafened: bool,
        volume: f32,
        cleaning: crate::volume::Cleaning,
    ) -> Result<(Self, Frames, mpsc::UnboundedReceiver<Report>)> {
        let frames = Frames::create(WATCH_WIDTH, WATCH_HEIGHT)?;
        let job = Job::Watch {
            server: server.clone(),
            frames: frames.path().context("the video frames file has no name")?,
            width: WATCH_WIDTH,
            height: WATCH_HEIGHT,
            speakers,
            deafened,
            volume,
            cleaning,
        };
        let (worker, reports) = Self::start(&job)?;
        Ok((worker, frames, reports))
    }

    /// Starts playing the video file at `file`, which may still be
    /// downloading, with pictures that fit `width` by `height` for the UI in
    /// a new [`Frames`].
    pub fn play(
        file: PathBuf,
        size: Option<u64>,
        (width, height): (u32, u32),
        speakers: Option<String>,
        volume: f32,
    ) -> Result<(Self, Frames, mpsc::UnboundedReceiver<Report>)> {
        let frames = Frames::create(WATCH_WIDTH, WATCH_HEIGHT)?;
        let job = Job::Play {
            file,
            size,
            frames: frames.path().context("the video frames file has no name")?,
            width: width.min(WATCH_WIDTH),
            height: height.min(WATCH_HEIGHT),
            speakers,
            volume,
        };
        let (worker, reports) = Self::start(&job)?;
        Ok((worker, frames, reports))
    }

    /// Tells a playing video worker what to do.
    pub fn tell(&mut self, control: &Control) -> Result<()> {
        self.control(control)
    }

    /// Tells a sharing worker to share at `quality` from now on, its
    /// bitrate limit included.
    pub fn quality(&mut self, quality: StreamQuality) -> Result<()> {
        let (width, height) = bounds(quality.resolution);
        self.control(&Control::Quality {
            width,
            height,
            fps: quality.fps,
            effort: effort(quality),
            max_kbps: quality.max_kbps,
        })
    }

    /// Tells a watching worker whether to play the stream's sound.
    pub fn hear(&mut self, hearing: bool) -> Result<()> {
        self.control(&Control::Hear(hearing))
    }

    /// Tells a watching worker how loud to play the stream's sound.
    pub fn volume(&mut self, volume: f32) -> Result<()> {
        self.control(&Control::Volume(volume))
    }

    /// Tells a watching worker how to clean the stream's sound.
    pub fn cleaning(&mut self, cleaning: crate::volume::Cleaning) -> Result<()> {
        self.control(&Control::Cleaning(cleaning))
    }

    /// A feed of what the call sends to a watching worker's echo gate,
    /// whose stream's sound takes this account's echo out: the call's thread
    /// hands each tick to [`EchoFeed::send`], which neither waits nor
    /// allocates, and a thread of its own writes the ticks to the worker as
    /// they come, a line each. The thread ends with the feed, or with the
    /// worker; none while its input is gone.
    pub fn echo_feed(&self) -> Option<EchoFeed> {
        let input = Arc::downgrade(self.stdin.as_ref()?);
        let (ticks, taken) = std::sync::mpsc::sync_channel::<crate::echo_gate::Ours>(ECHO_QUEUE);
        let started = std::thread::Builder::new()
            .name("yapfast-echo-feed".into())
            .spawn(move || {
                while let Ok(first) = taken.recv() {
                    // Ticks that waited go with it, a line for them all.
                    let ticks = std::iter::once(first).chain(taken.try_iter()).collect();
                    let Ok(line) = serde_json::to_string(&Control::Echo(ticks)) else {
                        continue;
                    };
                    let Some(stdin) = input.upgrade() else {
                        return;
                    };
                    let mut stdin = stdin
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if writeln!(stdin, "{line}").is_err() {
                        return;
                    }
                }
            });
        match started {
            Ok(_) => Some(EchoFeed { ticks }),
            Err(error) => {
                tracing::warn!(%error, "could not feed the stream's echo gate");
                None
            }
        }
    }

    /// Hands the worker the server Discord started the stream on.
    pub fn send(&mut self, server: &StreamServer) -> Result<()> {
        self.control(&Control::Send(server.clone()))
    }

    /// Tells a sharing worker to share `source` in place of what it shares.
    pub fn switch(&mut self, source: Source) -> Result<()> {
        self.control(&Control::Switch(picked(source)))
    }

    fn control(&mut self, control: &Control) -> Result<()> {
        let line = serde_json::to_string(control)?;
        let stdin = self
            .stdin
            .as_ref()
            .context("the stream worker has stopped")?;
        let mut stdin = stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        writeln!(stdin, "{line}").context("could not reach the stream worker")?;
        Ok(())
    }

    fn start(job: &Job) -> Result<(Self, mpsc::UnboundedReceiver<Report>)> {
        // Tests run from their own binary, so they name Yapfast's.
        let program = match std::env::var_os("YAPFAST_STREAM_WORKER") {
            Some(program) => PathBuf::from(program),
            None => running_yapfast()?,
        };
        let mut command = Command::new(program);
        command
            .arg(ARGUMENT)
            .env(LANGUAGE, yapfast_lang::chosen().code)
            // NVENC's device is counted as `adapters::cuda_index` counts it,
            // in PCI bus order, not CUDA's fastest first.
            .env("CUDA_DEVICE_ORDER", "PCI_BUS_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(crate::logging::for_worker());
        if crate::picture::wakes_for_frames() {
            command.env(FRAME_LINES, "1");
        }
        let mut child = command
            .spawn()
            .context("could not start the stream worker")?;
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let lines = opening(job, chosen_cpus())?;
        if let Some(stdin) = stdin.as_mut() {
            stdin
                .write_all(lines.as_bytes())
                .context("could not reach the stream worker")?;
        }
        let stdin = stdin.map(|stdin| Arc::new(Mutex::new(stdin)));
        if let Some(stdin) = &stdin {
            let mut running = RUNNING
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            running.retain(|input| input.strong_count() > 0);
            running.push(Arc::downgrade(stdin));
        }
        let (reports, receiver) = mpsc::unbounded_channel();
        let unexpected = match job {
            Job::Play { .. } => tr!("The video stopped unexpectedly."),
            _ => tr!("Screen sharing stopped unexpectedly."),
        };
        let child = Arc::new(Mutex::new(child));
        let mut last = Last::new(job);
        let waited = child.clone();
        // The caller's, so a test's logger hears this thread too.
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        std::thread::Builder::new()
            .name("yapfast-stream-reports".into())
            .spawn(move || {
                let _logging = tracing::dispatcher::set_default(&dispatch);
                let mut ended = false;
                for line in stdout
                    .into_iter()
                    .flat_map(|stdout| BufReader::new(stdout).lines().map_while(Result::ok))
                {
                    if line == crate::frames::FRAME_LINE {
                        crate::picture::frame_made();
                        continue;
                    }
                    match serde_json::from_str::<Report>(&line) {
                        Ok(report) => {
                            ended |= matches!(report, Report::Ended { .. });
                            last.saw(&report);
                            if reports.send(report).is_err() {
                                return;
                            }
                        }
                        Err(error) => tracing::warn!(%error, "unknown stream worker report"),
                    }
                }
                if !ended {
                    last.log(exited(&waited));
                    let _ = reports.send(Report::Ended {
                        error: Some(unexpected.into()),
                    });
                }
            })
            .context("could not start the stream worker")?;
        Ok((Self { child, stdin }, receiver))
    }
}

/// The Yapfast that's running, to start as the worker. On Linux that's
/// `/proc/self/exe`, which still opens after a rebuild or an update replaced
/// Yapfast's file, where its path names a deleted file; it also keeps the worker
/// the same version as the window it talks to.
fn running_yapfast() -> Result<PathBuf> {
    if cfg!(target_os = "linux") {
        return Ok(PathBuf::from("/proc/self/exe"));
    }
    std::env::current_exe().context("could not find Yapfast to start streaming")
}

/// What a worker's reports last said about its job, to log if it ends
/// without saying why.
struct Last {
    job: &'static str,
    /// The encoder's engine and codec, once a share reports how it goes out.
    encoder: Option<(String, String)>,
    /// The size reported, or the size the job fits in until one is.
    size: (u32, u32),
    reported: bool,
}

impl Last {
    fn new(job: &Job) -> Self {
        Self {
            job: job.kind(),
            encoder: None,
            size: job.bounds(),
            reported: false,
        }
    }

    fn saw(&mut self, report: &Report) {
        match report {
            Report::Sharing { width, height } | Report::Opened { width, height, .. } => {
                (self.size, self.reported) = ((*width, *height), true);
            }
            Report::Sending(stats) => {
                (self.size, self.reported) = ((stats.width, stats.height), true);
                self.encoder = Some((stats.engine.clone(), stats.codec.clone()));
            }
            _ => {}
        }
    }

    /// Warns that the worker ended without a [`Report::Ended`], and how.
    /// What it wrote to standard error before that is already in the log,
    /// through [`crate::logging::for_worker`].
    fn log(&self, exit: String) {
        let (width, height) = self.size;
        let (engine, codec) = self
            .encoder
            .as_ref()
            .map_or(("unknown", "unknown"), |(engine, codec)| {
                (engine.as_str(), codec.as_str())
            });
        let size = if self.reported {
            format!("{width}x{height}")
        } else {
            format!("up to {width}x{height}")
        };
        tracing::warn!(
            job = self.job,
            engine,
            codec,
            size,
            exit,
            "the stream worker ended without saying why"
        );
    }
}

/// How the worker ended, once its output closed: it has exited by then, or
/// is about to.
fn exited(child: &Mutex<Child>) -> String {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let waited = child.lock().map(|mut child| child.try_wait());
        match waited {
            Ok(Ok(Some(status))) => return how(status),
            Ok(Ok(None)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(Ok(None)) => return "still running".into(),
            Ok(Err(error)) => return format!("unknown ({error})"),
            Err(_) => return "unknown".into(),
        }
    }
}

/// An exit status for the log: the signal that killed it on Unix, such as
/// `signal=11 (SIGSEGV)`, or its exit code.
fn how(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            let core = if status.core_dumped() {
                ", core dumped"
            } else {
                ""
            };
            return match signal_name(signal) {
                Some(name) => format!("signal={signal} ({name}{core})"),
                None => format!("signal={signal}{core}"),
            };
        }
    }
    match status.code() {
        // Windows reports a crash as an NTSTATUS code, such as 0xC0000005
        // for an access violation.
        #[cfg(windows)]
        Some(code) if code < 0 => format!("code={code} ({:#010X})", code as u32),
        Some(code) => format!("code={code}"),
        None => status.to_string(),
    }
}

/// The name of a signal that commonly ends a process.
#[cfg(unix)]
fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        #[cfg(target_os = "linux")]
        7 => "SIGBUS",
        #[cfg(not(target_os = "linux"))]
        10 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        15 => "SIGTERM",
        _ => return None,
    })
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Its input closes as this lets go of it: RUNNING's is weak.
        if let Some(stdin) = self.stdin.take() {
            let mut stdin = stdin
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let _ = writeln!(stdin, "stop");
        }
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        // The worker checks for the stop between frames.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if !matches!(child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        tracing::warn!("the stream worker didn't stop; ending it");
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// How long [`Pace`] measures x264 at a time.
const PACE_WINDOW: Duration = Duration::from_secs(2);
/// The share of each frame's time, in percent, x264 may take and still keep
/// up: the rest goes to capturing, converting and scaling the frame, on the
/// same thread.
const PACE_SHARE: u32 = 90;
/// Windows in a row x264 falls behind in before the dock says so: a burst
/// of motion alone doesn't.
const PACE_WINDOWS: u32 = 2;

/// Whether x264 keeps up with the stream on the cores chosen: the time it
/// takes to encode the frames of each [`PACE_WINDOW`], against the time
/// those frames have at the stream's frame rate. It falls behind where it
/// takes [`PACE_SHARE`] of that or more: the stream then gets fewer frames
/// than it asked for. Time between frames that come slower than the frame
/// rate, as a camera's can, counts neither way.
#[derive(Debug, Default)]
struct Pace {
    /// When the window started; none before its first frame.
    since: Option<Instant>,
    /// The frames encoded in it, and the time encoding them took.
    frames: u32,
    busy: Duration,
    /// Windows in a row x264 fell behind in.
    behind: u32,
}

impl Pace {
    /// Counts a frame x264 took `took` to encode, at `now`, for a stream of
    /// `fps` frames a second. Once it has fallen behind for
    /// [`PACE_WINDOWS`] windows in a row, the share of each frame's time it
    /// took in the last, in percent.
    fn encoded(&mut self, took: Duration, fps: u32, now: Instant) -> Option<u32> {
        let since = *self.since.get_or_insert(now);
        self.frames += 1;
        self.busy += took;
        if now.saturating_duration_since(since) < PACE_WINDOW {
            return None;
        }
        // The time the window's frames have at the stream's frame rate.
        let had = Duration::from_secs(u64::from(self.frames)) / fps.max(1);
        let share = (self.busy.as_micros() * 100 / had.as_micros().max(1)) as u32;
        self.behind = if share >= PACE_SHARE {
            self.behind + 1
        } else {
            0
        };
        (self.since, self.frames, self.busy) = (Some(now), 0, Duration::ZERO);
        (self.behind >= PACE_WINDOWS).then_some(share)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sink writing to `frames`, at 1080p30 with little effort, encoding
    /// where `on` says, without sound.
    fn sink_writing(frames: &Frames, controls: channel::Receiver<Control>, on: EncodeOn) -> Sink {
        let writer = Writer::open(&frames.path().unwrap()).unwrap();
        let bounds = (SHARE_WIDTH, SHARE_HEIGHT);
        Sink::new(writer, controls, bounds, 30, Effort::Light, on, None)
    }

    #[test]
    fn shows_mobile_streams_upright_when_the_phone_turns() {
        // Left half dark, right half light, sent as landscape pixels.
        let y: Vec<_> = (0..32)
            .map(|index| if index % 8 < 4 { 16 } else { 235 })
            .collect();
        let (u, v) = ([128; 8], [128; 8]);
        let picture = I420 {
            width: 8,
            height: 4,
            y: &y,
            u: &u,
            v: &v,
            strides: [8, 4, 4],
        };
        for maximum in [(8, 8), (8, 4)] {
            let frames = Frames::create(maximum.0, maximum.1).unwrap();
            let mut writer = Writer::open(&frames.path().unwrap()).unwrap();
            let (mut scratch, mut strip) = (vec![], vec![]);
            // Landscape, both portrait directions, upside down, then back.
            for turns in [0, 1, 3, 2, 0] {
                let size = show_stream_picture(
                    &mut writer,
                    picture,
                    Turn { turns, flip: false },
                    maximum,
                    &mut scratch,
                    &mut strip,
                );
                let expected = if turns % 2 == 1 {
                    if maximum.1 == 4 { (2, 4) } else { (4, 8) }
                } else {
                    (8, 4)
                };
                assert_eq!(size, expected);
                let shown = frames
                    .take(|width, height, rgba| {
                        ((width, height), rgba[0] > 128, rgba[rgba.len() - 4] > 128)
                    })
                    .unwrap();
                assert_eq!(shown, (expected, turns >= 2, turns < 2));
            }
        }
    }

    /// A worker that dies without a [`Report::Ended`] logs a warning with
    /// how it ended and the job it had, and the UI still gets the popup's
    /// error.
    #[cfg(unix)]
    #[test]
    fn a_worker_that_dies_says_how() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let died = |script: &str| {
            let worker = directory.path().join("worker");
            std::fs::write(&worker, format!("#!/bin/sh\nread job\n{script}\n")).unwrap();
            std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o755)).unwrap();
            let logged = Arc::new(Mutex::new(Vec::new()));
            let writer = logged.clone();
            let logger = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(move || Logged(writer.clone()))
                .finish();
            let _logging = tracing::subscriber::set_default(logger);
            // Only this test starts a worker.
            unsafe { std::env::set_var("YAPFAST_STREAM_WORKER", &worker) };
            let (worker, mut reports) = Worker::start(&Job::Glance {
                screens: vec![],
                width: 320,
                height: 180,
            })
            .unwrap();
            assert_eq!(
                reports.blocking_recv(),
                Some(Report::Ended {
                    error: Some("Screen sharing stopped unexpectedly.".into())
                })
            );
            drop(worker);
            String::from_utf8(logged.lock().unwrap().clone()).unwrap()
        };

        // No core file: the shell's would be in the way.
        let log = died("ulimit -c 0; kill -SEGV $$");
        assert!(log.contains("WARN"), "{log}");
        assert!(
            log.contains("the stream worker ended without saying why"),
            "{log}"
        );
        assert!(log.contains("exit=\"signal=11 (SIGSEGV"), "{log}");
        assert!(log.contains("job=\"glance\""), "{log}");
        assert!(log.contains("size=\"up to 320x180\""), "{log}");
        assert!(log.contains("engine=\"unknown\""), "{log}");

        let log = died("exit 3");
        assert!(log.contains("exit=\"code=3\""), "{log}");
    }

    /// Collects a test's log.
    #[cfg(unix)]
    struct Logged(Arc<Mutex<Vec<u8>>>);

    #[cfg(unix)]
    impl Write for Logged {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[cfg(unix)]
    #[test]
    fn says_which_signal_ended_it() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(how(ExitStatus::from_raw(11)), "signal=11 (SIGSEGV)");
        assert_eq!(
            how(ExitStatus::from_raw(6 | 0x80)),
            "signal=6 (SIGABRT, core dumped)"
        );
        assert_eq!(how(ExitStatus::from_raw(1 << 8)), "code=1");
    }

    #[test]
    fn the_worker_is_the_running_yapfast() {
        // Here that's the test binary, which lists its tests and exits.
        let program = running_yapfast().unwrap();
        let status = std::process::Command::new(program)
            .args(["--list", "--format", "terse"])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn fits_frames_with_even_sizes() {
        assert_eq!(fit(1920, 1080, 1280, 720), (1280, 720));
        assert_eq!(fit(3440, 1440, 1280, 720), (1280, 534));
        assert_eq!(fit(801, 601, 1280, 720), (800, 600), "never larger");
    }

    /// 1440p is 2560 by 1440: a 16:9 screen fills it, an ultrawide fills
    /// its width as at the other sizes, and a smaller screen keeps its size.
    #[test]
    fn shares_1440p_within_2560_by_1440() {
        let (width, height) = bounds(Resolution::P1440);
        assert_eq!((width, height), (2560, 1440));
        assert_eq!(fit(3840, 2160, width, height), (2560, 1440));
        assert_eq!(fit(2560, 1440, width, height), (2560, 1440));
        assert_eq!(fit(3440, 1440, width, height), (2560, 1070));
        assert_eq!(fit(5120, 1440, width, height), (2560, 720));
        assert_eq!(fit(1920, 1080, width, height), (1920, 1080), "never larger");
    }

    /// With the GPU, AV1 and H265 before H264 where they open, and an
    /// encoder for the first; without it, H264 alone, on x264.
    #[test]
    fn offers_the_gpus_codecs_first() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Auto);
        let (offers, encoder) = sink.offers(320, 180).unwrap();
        eprintln!("offering {offers:?}, encoding with {}", encoder.engine());
        assert_eq!(offers.last(), Some(&Codec::H264));
        let mut order = [Codec::Av1, Codec::H265, Codec::H264].iter();
        assert!(
            offers.iter().all(|codec| order.any(|next| next == codec)),
            "in order"
        );
        assert_eq!(encoder.codec(), offers[0]);
        let mut sink = sink_writing(&frames, channel::channel().1, EncodeOn::Software);
        let (offers, encoder) = sink.offers(320, 180).unwrap();
        assert_eq!((offers, encoder.engine()), (vec![Codec::H264], "x264"));
    }

    #[test]
    fn previews_what_it_sends_at_a_smaller_size() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        // A 1080p picture, mid-grey: luma and chroma halfway.
        let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
        let (y, u, v) = picture.data_mut();
        y.fill(126);
        u.fill(128);
        v.fill(128);
        sink.frame(&mut picture).unwrap();
        let (width, height, first) = frames
            .take(|width, height, pixels| (width, height, pixels[..4].to_vec()))
            .unwrap();
        assert_eq!((width, height), (PREVIEW_WIDTH, PREVIEW_HEIGHT));
        assert!(
            first[..3].iter().all(|&level| level.abs_diff(128) < 4),
            "{first:?}"
        );
        assert_eq!(first[3], 255);
    }

    #[test]
    fn switches_to_the_last_source_asked_for() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        let mut report = |_| {};
        assert_eq!(sink.check(&mut report).unwrap(), None);
        controls.send(Control::Switch(Source::Display(1))).unwrap();
        controls.send(Control::Switch(Source::Window(2))).unwrap();
        assert_eq!(sink.check(&mut report).unwrap(), Some(Source::Window(2)));
        assert_eq!(sink.check(&mut report).unwrap(), None);
    }

    /// The sound changes alone, before the stream connects too, and the
    /// test pattern never takes any.
    #[test]
    fn changes_the_sound_without_switching_the_picture() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        sink.sound = Some(StreamAudio::All);
        let mut reported = vec![];
        for audio in [StreamAudio::Off, StreamAudio::Window(42), StreamAudio::All] {
            controls.send(Control::Audio(audio)).unwrap();
            assert_eq!(
                sink.check(&mut |report| reported.push(report)).unwrap(),
                None
            );
            assert_eq!(sink.sound, Some(audio));
            let mut picture = I420Buffer::new(320, 180);
            sink.frame(&mut picture).unwrap();
            assert!(frames.take(|width, height, _| (width, height)).is_some());
        }
        assert!(reported.is_empty(), "{reported:?}");
        sink.sound = None;
        controls.send(Control::Audio(StreamAudio::All)).unwrap();
        sink.check(&mut |_| {}).unwrap();
        assert_eq!(sink.sound, None, "the test pattern stays silent");
    }

    #[test]
    fn previews_a_stream_once_its_picture_shows() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        sink.preview_due = Some(Instant::now());
        let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
        let (y, u, v) = picture.data_mut();
        y.fill(16);
        u.fill(128);
        v.fill(128);
        sink.frame(&mut picture).unwrap();
        assert_eq!(sink.thumbnail, None, "not while the screen is black");
        picture.data_mut().0.fill(126);
        sink.frame(&mut picture).unwrap();
        let thumbnail = sink.thumbnail.take().expect("a preview");
        let jpeg = thumbnail
            .strip_prefix("data:image/jpeg;base64,")
            .map(|data| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .unwrap()
            })
            .unwrap();
        let shown = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((shown.width(), shown.height()), (512, 288));
        sink.frame(&mut picture).unwrap();
        assert_eq!(sink.thumbnail, None, "the next one comes minutes later");
    }

    /// A change of quality while sharing reaches the sink whole, its frame
    /// rate kept within what Discord takes.
    #[test]
    fn takes_a_new_quality() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        let quality = Control::Quality {
            width: 1280,
            height: 720,
            fps: 120,
            effort: Effort::High,
            max_kbps: Some(2_000),
        };
        controls.send(quality).unwrap();
        assert_eq!(sink.check(&mut |_| {}).unwrap(), None);
        assert_eq!(
            (sink.bounds, sink.fps, sink.effort, sink.max_kbps),
            ((1280, 720), 60, Effort::High, Some(2_000))
        );
        // A quality from before there was a limit reads as Auto.
        let older = r#"{"quality":{"width":1280,"height":720,"fps":30,"effort":"light"}}"#;
        assert!(matches!(
            serde_json::from_str::<Control>(older).unwrap(),
            Control::Quality { max_kbps: None, .. }
        ));
    }

    /// The unlock reaches a sink as it shares, as where it encodes does,
    /// and the limit it keeps counts past 9 Mbit/s only while it's on: the
    /// encoder opens holding that cap, and one that holds less opens again.
    #[test]
    fn takes_the_bitrate_unlock() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        sink.max_kbps = Some(20_000);
        assert_eq!(sink.reach(1440), 0, "9 Mbit/s: nothing more to hold");
        controls.send(Control::BitrateUnlocked(true)).unwrap();
        sink.check(&mut |_| {}).unwrap();
        assert!(sink.bitrate_unlocked);
        assert_eq!(sink.reach(1440), 20_000);
        assert_eq!(sink.reach(720), 20_000, "at every size");
        sink.max_kbps = None;
        assert_eq!(sink.reach(1440), 0, "Auto is Discord's cap");
        sink.max_kbps = Some(8_000);
        assert_eq!(sink.reach(1440), 0);
        let encoder = sink
            .open_on(Codec::H264, 64, 36, &Device::Processor)
            .unwrap();
        assert_eq!(encoder.held(), 0, "opened as it always was");
        sink.max_kbps = Some(50_000);
        let encoder = sink
            .open_on(Codec::H264, 64, 36, &Device::Processor)
            .unwrap();
        assert_eq!(
            (encoder.held(), encoder.bitrate()),
            (50_000, 2_500),
            "holding 50, starting where streams start"
        );
        controls.send(Control::BitrateUnlocked(false)).unwrap();
        sink.check(&mut |_| {}).unwrap();
        assert!(!sink.bitrate_unlocked);
        assert_eq!(sink.max_kbps, Some(50_000), "kept for when it's back");
        assert_eq!(sink.reach(1440), 0);
        // A job from before the unlock shares within 9 Mbit/s.
        let job: Job = serde_json::from_str(
            r#"{"share":{"source":"test_pattern","frames":"/x","width":2,"height":2,"fps":30}}"#,
        )
        .unwrap();
        assert!(matches!(
            job,
            Job::Share {
                bitrate_unlocked: false,
                ..
            }
        ));
        let line = serde_json::to_string(&Control::BitrateUnlocked(true)).unwrap();
        assert_eq!(line, r#"{"bitrate_unlocked":true}"#);
    }

    /// The core's own numbers for the limit agree with the connection's.
    #[test]
    fn the_limits_kbps_are_the_connections() {
        use crate::voice::{DOCUMENTED_KBPS, UNLOCKED_KBPS};
        assert_eq!(DOCUMENTED_KBPS * 1000, stream_connection::DOCUMENTED_MAX);
        assert_eq!(UNLOCKED_KBPS * 1000, stream_connection::UNLOCKED_MAX);
    }

    /// A change of where streams encode reaches the sink while it shares:
    /// it closes the encoder, and the next frame opens one there. The
    /// same choice again changes nothing.
    #[test]
    fn takes_another_place_to_encode() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        sink.placed = Placed::Processor;
        sink.candidates = Some(vec![]);
        controls
            .send(Control::EncodeOn(EncodeOn::Software))
            .unwrap();
        sink.check(&mut |_| {}).unwrap();
        assert_eq!(sink.placed, Placed::Processor, "the same choice");
        let chosen = EncodeOn::Adapter("pci:0000:73:00.0".into());
        controls.send(Control::EncodeOn(chosen.clone())).unwrap();
        sink.check(&mut |_| {}).unwrap();
        assert_eq!(
            (&sink.encode_on, &sink.placed, &sink.candidates),
            (&chosen, &Placed::Anywhere, &None)
        );
    }

    /// A worker runs on the CPUs chosen in every mode, Auto's, those ticked
    /// and all of them, whether its stream is to encode on the processor
    /// or a graphics card: Yapfast starts it with them, and each change
    /// while it shares reaches it as it was chosen.
    #[test]
    fn runs_on_the_cpus_chosen_whatever_encodes() {
        static CONFINED: Mutex<Vec<Option<CpuMask>>> = Mutex::new(Vec::new());
        fn confined(cpus: Option<CpuMask>) {
            CONFINED.lock().unwrap().push(cpus);
        }
        let auto = CpuMask::parse("8-15,24-31");
        let ticked = CpuMask::parse("15,31");
        let modes = [auto, ticked, None];
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let share = |encode_on| Job::Share {
            audio: StreamAudio::Off,
            source: Source::TestPattern,
            frames: frames.path().unwrap(),
            width: 320,
            height: 180,
            fps: 30,
            effort: Effort::Light,
            encode_on,
            max_kbps: None,
            bitrate_unlocked: false,
        };
        for on in [EncodeOn::Software, EncodeOn::Auto] {
            // What Yapfast starts the worker with: the CPUs as Linux lists
            // them, or none for all.
            for cpus in modes {
                let job = share(on.clone());
                let opened = opening(&job, cpus).unwrap();
                let lines: Vec<&str> = opened.lines().collect();
                assert_eq!(serde_json::from_str::<Job>(lines[0]).unwrap(), job);
                assert_eq!(
                    serde_json::from_str::<Control>(lines[1]).unwrap(),
                    Control::Cpus(cpus)
                );
            }
            let lines = opening(&share(on.clone()), auto).unwrap();
            assert!(lines.ends_with("\n{\"cpus\":\"8-15,24-31\"}\n"), "{lines}");
            // What the worker runs on: Auto's as it starts, then those
            // ticked, then all.
            let (input, mut feed) = std::io::pipe().unwrap();
            feed.write_all(lines.as_bytes()).unwrap();
            let (sender, reports) = std::sync::mpsc::channel();
            let serving = std::thread::spawn(move || {
                let mut report = |report: Report| sender.send(report).unwrap();
                serve_on(Box::new(BufReader::new(input)), &mut report, confined);
            });
            assert_eq!(reports.recv().unwrap(), Report::Ready);
            for cpus in [ticked, None] {
                let line = serde_json::to_string(&Control::Cpus(cpus)).unwrap();
                writeln!(feed, "{line}").unwrap();
            }
            writeln!(feed, "stop").unwrap();
            serving.join().unwrap();
            assert_eq!(std::mem::take(&mut *CONFINED.lock().unwrap()), modes);
        }
    }

    /// x264 falls behind where it takes nine tenths or more of each frame's
    /// time for two windows of 2 s in a row; not while it takes less, nor
    /// for a burst of one window, nor because frames come slower.
    #[test]
    fn x264_falls_behind_only_when_it_takes_the_frames_time() {
        let start = Instant::now();
        let mut clock = 0;
        // `count` frames at 30 fps, one every `every` ms, each taking
        // `took` ms to encode; the share reported, if any.
        let mut frames = |pace: &mut Pace, count: u32, every: u64, took: u64| {
            let mut said = None;
            for _ in 0..count {
                let now = start + Duration::from_millis(clock);
                said = said.or(pace.encoded(Duration::from_millis(took), 30, now));
                clock += every;
            }
            said
        };
        let mut pace = Pace::default();
        assert_eq!(frames(&mut pace, 300, 33, 15), None, "keeps up");
        assert_eq!(frames(&mut pace, 100, 100, 20), None, "slower frames");
        let mut pace = Pace::default();
        assert_eq!(frames(&mut pace, 60, 40, 40), None, "a burst");
        assert_eq!(frames(&mut pace, 150, 33, 15), None);
        let mut pace = Pace::default();
        assert_eq!(frames(&mut pace, 125, 40, 40), Some(120), "behind");
    }

    /// The dock hears once in a stream that x264 can't keep up on the cores
    /// chosen, however long it lasts, and nothing while it keeps up.
    #[test]
    fn says_once_when_x264_cannot_keep_up() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        let start = Instant::now();
        let at = |frame: u64| start + Duration::from_millis(frame * 33);
        let mut said = vec![];
        // At 30 fps, 15 ms a frame keeps up, for 10 s.
        for frame in 0..300 {
            sink.encoded_in(Duration::from_millis(15), at(frame));
            sink.check(&mut |report| said.push(report)).unwrap();
        }
        assert!(said.is_empty(), "{said:?}");
        // 40 ms a frame doesn't, for 20 s.
        for frame in 300..900 {
            sink.encoded_in(Duration::from_millis(40), at(frame));
            sink.check(&mut |report| said.push(report)).unwrap();
        }
        assert!(
            matches!(&said[..], [Report::Notice(notice)] if notice.contains("cores chosen")),
            "{said:?}"
        );
    }

    /// Settings' `stream_encoder` names Auto, the processor, or an adapter
    /// by its key; anything else is Auto.
    #[test]
    fn reads_where_streams_encode() {
        assert_eq!(EncodeOn::parse("auto"), EncodeOn::Auto);
        assert_eq!(EncodeOn::parse("software"), EncodeOn::Software);
        assert_eq!(
            EncodeOn::parse("pci:0000:01:00.0"),
            EncodeOn::Adapter("pci:0000:01:00.0".into())
        );
        assert_eq!(EncodeOn::parse("gpu"), EncodeOn::Auto);
        // A job from before the choice encodes as Auto.
        let job: Job = serde_json::from_str(
            r#"{"share":{"source":"test_pattern","frames":"/x","width":2,"height":2,"fps":30}}"#,
        )
        .unwrap();
        assert!(matches!(
            job,
            Job::Share {
                encode_on: EncodeOn::Auto,
                max_kbps: None,
                ..
            }
        ));
    }

    /// The dock hears once how the stream's encoding isn't what Settings
    /// asked for: a chosen card that's missing, or one that didn't open.
    /// The processor chosen says nothing.
    #[test]
    fn says_where_the_stream_encodes_when_it_had_to_fall_back() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = sink_writing(&frames, received, EncodeOn::Software);
        let card = |name: &str| Candidate {
            device: Device::Vaapi {
                node: PathBuf::from("/dev/dri/renderD128"),
                kernel: "amdgpu".into(),
            },
            name: Some(name.into()),
        };
        let notices = |sink: &mut Sink| {
            let mut said = vec![];
            sink.check(&mut |report| said.push(report)).unwrap();
            said
        };
        sink.place(Placed::On(card("Radeon")), None);
        assert!(notices(&mut sink).is_empty());
        sink.place(Placed::On(card("RTX 3080")), Some(&card("Radeon")));
        let said = notices(&mut sink);
        assert!(
            matches!(&said[..], [Report::Notice(notice)] if notice.contains("Radeon") && notice.contains("RTX 3080")),
            "{said:?}"
        );
        sink.missing = true;
        sink.place(Placed::On(card("RTX 3080")), None);
        assert!(matches!(&notices(&mut sink)[..], [Report::Notice(_)]));
        assert!(!sink.missing, "said once");
        // Whichever encoder opened where none could be listed has no name
        // to give.
        let anywhere = Candidate {
            device: Device::AnyGpu,
            name: None,
        };
        sink.place(Placed::Processor, Some(&anywhere));
        let said = notices(&mut sink);
        assert!(
            said.iter().all(
                |report| !matches!(report, Report::Notice(notice) if notice.contains("couldn't"))
            ),
            "{said:?}"
        );
    }

    /// A smaller size's encoder aims within that size's cap at once, though
    /// the stream's target still follows the larger size's until its first
    /// frame goes out.
    #[test]
    fn aims_within_the_cap_of_a_new_size() {
        assert_eq!(aim(6_000_000, 720, None, false), 3_500);
        assert_eq!(aim(6_000_000, 1080, None, false), 6_000);
        assert_eq!(aim(2_000_000, 720, None, false), 2_000);
    }

    /// A bitrate limit is a ceiling: it lowers the aim, never raises it, and
    /// never past Discord's cap for the size, unless unlocked, then never
    /// past 50 Mbit/s. A limit of 20 kept while it's locked aims at 9, and
    /// at 20 again once it's unlocked.
    #[test]
    fn aims_within_the_bitrate_limit() {
        assert_eq!(aim(9_000_000, 1440, Some(5_000), false), 5_000);
        assert_eq!(
            aim(3_000_000, 1440, Some(5_000), false),
            3_000,
            "not a floor"
        );
        assert_eq!(aim(30_000_000, 1440, Some(20_000), false), 9_000);
        assert_eq!(aim(9_000_000, 720, Some(8_000), false), 3_500);
        assert_eq!(aim(30_000_000, 1440, Some(20_000), true), 20_000);
        assert_eq!(aim(12_000_000, 1440, Some(20_000), true), 12_000);
        assert_eq!(aim(60_000_000, 720, Some(60_000), true), 50_000);
        assert_eq!(aim(30_000_000, 1440, None, true), 9_000, "Auto");
    }

    /// The test pattern's frames follow a change of quality sent to the
    /// worker as it shares: a smaller size, then a lower frame rate.
    #[test]
    fn shares_at_a_new_quality_without_stopping() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            audio: StreamAudio::Off,
            source: Source::TestPattern,
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 60,
            effort: Effort::Light,
            encode_on: EncodeOn::Software,
            max_kbps: None,
            bitrate_unlocked: false,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        let (sender, reports) = std::sync::mpsc::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                sender.send(report).unwrap();
            });
        });
        assert_eq!(reports.recv().unwrap(), Report::Ready);
        let sharing = |width, height| Report::Sharing { width, height };
        assert_eq!(reports.recv().unwrap(), sharing(640, 360));
        let quality = |fps| Control::Quality {
            width: 320,
            height: 180,
            fps,
            effort: Effort::Light,
            max_kbps: None,
        };
        writeln!(feed, "{}", serde_json::to_string(&quality(60)).unwrap()).unwrap();
        assert_eq!(reports.recv().unwrap(), sharing(320, 180));
        let deadline = Instant::now() + Duration::from_secs(5);
        let size = loop {
            if let Some(size) = frames.take(|width, height, _| (width, height))
                && size == (320, 180)
            {
                break size;
            }
            assert!(Instant::now() < deadline, "no smaller frame arrived");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(size, (320, 180));
        // The frames that arrive within `time`.
        let count = |time| {
            let until = Instant::now() + time;
            let mut counted = 0;
            while Instant::now() < until {
                counted += usize::from(frames.take(|_, _, _| ()).is_some());
                std::thread::sleep(Duration::from_millis(2));
            }
            counted
        };
        let fast = count(Duration::from_millis(500));
        assert!(fast >= 8, "{fast} frames in half a second at 60 fps");
        writeln!(feed, "{}", serde_json::to_string(&quality(4)).unwrap()).unwrap();
        let slow = count(Duration::from_secs(1));
        assert!(slow <= 7, "{slow} frames in a second at 4 fps");
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        assert_eq!(reports.recv().unwrap(), Report::Ended { error: None });
    }

    #[test]
    fn serves_a_test_pattern_until_stopped() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            audio: StreamAudio::All,
            source: Source::TestPattern,
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 60,
            effort: Effort::Light,
            encode_on: EncodeOn::Software,
            max_kbps: None,
            bitrate_unlocked: false,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        let (sender, reports) = std::sync::mpsc::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                sender.send(report).unwrap();
            });
        });
        assert_eq!(reports.recv().unwrap(), Report::Ready);
        assert_eq!(
            reports.recv().unwrap(),
            Report::Sharing {
                width: 640,
                height: 360
            }
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let size = loop {
            if let Some(size) = frames.take(|width, height, pixels| (width, height, pixels.len())) {
                break size;
            }
            assert!(Instant::now() < deadline, "no frame arrived");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(size, (640, 360, 640 * 360 * 4));
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        assert_eq!(reports.recv().unwrap(), Report::Ended { error: None });
    }

    #[test]
    fn shares_a_camera_until_stopped() {
        // SAFETY: tests that read this variable all set it the same way.
        unsafe { std::env::set_var("YAPFAST_CAMERA_TEST_PATTERN", "1") };
        let listed = shareable();
        let camera = listed
            .iter()
            .find(|listed| listed.title == "Test pattern")
            .expect("the test pattern camera is listed");
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            audio: StreamAudio::All,
            source: camera.source.clone(),
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 30,
            effort: Effort::Light,
            encode_on: EncodeOn::Software,
            max_kbps: None,
            bitrate_unlocked: false,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        let (sender, reports) = std::sync::mpsc::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                sender.send(report).unwrap();
            });
        });
        assert_eq!(reports.recv().unwrap(), Report::Ready);
        assert_eq!(
            reports.recv_timeout(Duration::from_secs(5)).unwrap(),
            Report::Sharing {
                width: 640,
                height: 360
            }
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let size = loop {
            if let Some(size) = frames.take(|width, height, pixels| (width, height, pixels.len())) {
                break size;
            }
            assert!(Instant::now() < deadline, "no frame arrived");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(size, (640, 360, 640 * 360 * 4));
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        assert_eq!(reports.recv().unwrap(), Report::Ended { error: None });
    }

    /// The portal picks where libwebrtc captures through it: in a Wayland
    /// session, and not on X11, as on XFCE, nor where no Wayland display is
    /// named, where libwebrtc captures through X11.
    #[test]
    fn the_portal_picks_only_in_a_wayland_session() {
        assert!(wayland_session(Some("wayland"), true));
        assert!(!wayland_session(Some("x11"), false));
        assert!(!wayland_session(Some("x11"), true));
        assert!(!wayland_session(Some("tty"), false));
        assert!(!wayland_session(None, true));
        assert!(!wayland_session(Some("wayland"), false));
    }

    #[test]
    fn a_screen_picture_survives_its_report_line() {
        let report = Report::Screen {
            id: 7,
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        };
        let line = serde_json::to_string(&report).unwrap();
        assert!(line.contains(r#""rgba":"/wAA/wAA//8=""#), "{line}");
        assert_eq!(serde_json::from_str::<Report>(&line).unwrap(), report);
    }

    /// Takes a picture of each listed screen, as the share menu shows them.
    /// On Linux, X11 lists them: a private X server, as from `env -u
    /// WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 xvfb-run`, with a window as the
    /// next test says.
    #[test]
    #[ignore = "needs a desktop where Yapfast lists what to share"]
    fn glances_at_the_listed_screens() {
        let screens: Vec<u64> = shareable()
            .into_iter()
            .filter_map(|listed| match listed.source {
                Source::Display(id) => Some(id),
                _ => None,
            })
            .collect();
        assert!(!screens.is_empty(), "no screen");
        let job = Job::Glance {
            screens: screens.clone(),
            width: 320,
            height: 180,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        let mut reports = vec![];
        serve(Box::new(BufReader::new(input)), &mut |report| {
            reports.push(report)
        });
        let pictured: Vec<u64> = reports
            .iter()
            .filter_map(|report| match report {
                Report::Screen {
                    id,
                    width,
                    height,
                    rgba,
                } => {
                    eprintln!("screen {id}: {width}x{height}");
                    assert!(*width <= 320 && *height <= 180);
                    assert_eq!(rgba.len(), (width * height * 4) as usize);
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        assert_eq!(pictured, screens);
        assert_eq!(reports.last(), Some(&Report::Ended { error: None }));
    }

    /// Lists what there is to share, shares the first screen and the first
    /// window for a moment each, then switches from the screen to the window.
    /// Under Xvfb, as above, it needs a window with a title, and the
    /// `WM_STATE` and `_NET_WM_WINDOW_TYPE` a window manager would give it:
    /// without them libwebrtc's X11 capturer lists no window, or asks for a
    /// property X doesn't know and Xlib ends the process.
    #[test]
    #[ignore = "needs a desktop where Yapfast lists what to share"]
    fn shares_a_listed_screen_and_window() {
        let listed = shareable();
        eprintln!("{listed:#?}");
        let first = |window: bool| {
            listed
                .iter()
                .find(|listed| matches!(listed.source, Source::Window(_)) == window)
                .map(|listed| listed.source.clone())
        };
        let sources = [
            first(false).expect("no screen"),
            first(true).expect("no window"),
        ];
        for source in sources.clone() {
            let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
            let job = Job::Share {
                audio: StreamAudio::All,
                source: source.clone(),
                frames: frames.path().unwrap(),
                width: PREVIEW_WIDTH,
                height: PREVIEW_HEIGHT,
                fps: 30,
                effort: Effort::Light,
                encode_on: EncodeOn::Software,
                max_kbps: None,
                bitrate_unlocked: false,
            };
            let (input, mut feed) = std::io::pipe().unwrap();
            writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
            let (sender, reports) = std::sync::mpsc::channel();
            let serving = std::thread::spawn(move || {
                serve(Box::new(BufReader::new(input)), &mut |report| {
                    sender.send(report).unwrap();
                });
            });
            assert_eq!(reports.recv().unwrap(), Report::Ready);
            let sharing = reports.recv_timeout(Duration::from_secs(10)).unwrap();
            eprintln!("{source:?}: {sharing:?}");
            assert!(matches!(sharing, Report::Sharing { .. }), "{sharing:?}");
            // How much of the last picture isn't black, a second and a half
            // in: what's shared may still be drawing at first.
            let deadline = Instant::now() + Duration::from_millis(1500);
            let mut last = None;
            while Instant::now() < deadline {
                let taken = frames.take(|width, height, pixels| {
                    let lit = pixels.chunks_exact(4).filter(|pixel| pixel[..3] != [0; 3]);
                    (
                        width,
                        height,
                        lit.count() as f32 / (pixels.len() / 4).max(1) as f32,
                    )
                });
                last = taken.or(last);
                std::thread::sleep(Duration::from_millis(5));
            }
            let (width, height, lit) = last.expect("no frame arrived");
            eprintln!(
                "{source:?}: {width}x{height}, {:.0}% of it isn't black",
                lit * 100.0
            );
            writeln!(feed, "stop").unwrap();
            serving.join().unwrap();
            // The size may change as capture settles.
            let ended = reports
                .iter()
                .find(|report| !matches!(report, Report::Sharing { .. }));
            assert_eq!(ended, Some(Report::Ended { error: None }));
        }
        // Switching to the window while the screen is shared.
        let [screen, window] = sources;
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            audio: StreamAudio::All,
            source: screen,
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 30,
            effort: Effort::Light,
            encode_on: EncodeOn::Software,
            max_kbps: None,
            bitrate_unlocked: false,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        let (sender, reports) = std::sync::mpsc::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                sender.send(report).unwrap();
            });
        });
        assert_eq!(reports.recv().unwrap(), Report::Ready);
        let shared = reports.recv_timeout(Duration::from_secs(10)).unwrap();
        let switch = Control::Switch(window.clone());
        writeln!(feed, "{}", serde_json::to_string(&switch).unwrap()).unwrap();
        // The screen's size may still settle first.
        let switched = loop {
            let report = reports.recv_timeout(Duration::from_secs(10)).unwrap();
            if report != shared {
                break report;
            }
        };
        eprintln!("{shared:?}, then {window:?}: {switched:?}");
        assert!(matches!(switched, Report::Sharing { .. }), "{switched:?}");
        assert_ne!(switched, shared);
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        // A window closed since it was listed.
        let frames = Frames::create(16, 16).unwrap();
        let gone = Job::Share {
            audio: StreamAudio::All,
            source: Source::Window(u64::MAX),
            frames: frames.path().unwrap(),
            width: 16,
            height: 16,
            fps: 1,
            effort: Effort::Light,
            encode_on: EncodeOn::Software,
            max_kbps: None,
            bitrate_unlocked: false,
        };
        let mut reports = Vec::new();
        let input = format!("{}\n", serde_json::to_string(&gone).unwrap());
        serve(Box::new(std::io::Cursor::new(input)), &mut |report| {
            reports.push(report);
        });
        assert_eq!(
            reports,
            [
                Report::Ready,
                Report::Ended {
                    error: Some("That screen or window is gone.".into())
                }
            ]
        );
    }
}
