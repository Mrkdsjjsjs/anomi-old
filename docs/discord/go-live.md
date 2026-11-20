# Go Live

Sharing a screen or window with its sound on Discord, and watching others'
streams. The rest of Discord is in [the Discord docs](README.md).

Go Live streams come through Discord's gateway: op 18 starts sharing, op 20
watches someone, op 22 pauses and op 19 ends either. Discord answers with
STREAM_CREATE, naming the RTC server, then STREAM_SERVER_UPDATE with an
endpoint and token. The stream gets a voice connection of its own to that
server, reusing the call's voice session id; a stream waits for that id too,
if this account's voice state arrives after both answers. `yapfast_discord::stream` names
streams by key (`guild:{server}:{channel}:{user}`) and
`Discord::{go_live, watch, end_stream, pause_stream}` run the signalling. Voice
states carry `self_stream`, which shows as a LIVE badge beside the person.
Sharing starts from the call's controls or from the voice panel's Share
button, which works wherever the window is; the panel says when the stream
is live and stops it. In the call, as in Discord's, a stream is always a
tile of its own, right before its streamer's, who keeps their picture or
camera, their speaking ring and their marks: someone may film while they
share. The stream's tile has the red LIVE badge at its top right, and their
own none; a click on the stream's tile watches it and shows it large, a
click on theirs shows them. In a tile too small for all it has, what has no
room goes rather than covering another part (`anomi-voice/src/tiles/room.rs`):
the words under its middle first (what it waits for, "Watch stream", this
account's status), then its icon or ring shrinks, rises into the band
between the badge and the chip where it isn't clear of both in the middle
(Discord's strip keeps its screen there), and goes, a stream on its way
then turning its ring in the name's chip; who watches and the stop
button go before the middle does; the name and the badge stay, as at the
strip's smallest, 120 by 67.5. A stream being watched starts large, however
watching began, and stays watched in its tile after a click beside it.
While a stream shows large, the row beneath keeps its place with a
stand-in, its screen icon and LIVE badge, as Discord's strip does, which
draws and decodes nothing more; someone shown large leaves the row. While
the call is off screen, the large tile's stream or camera, or else the
stream being watched, or this account's own, shows in a corner
(`StreamPip`), as Discord's picture in picture does. STREAM_CREATE and
STREAM_UPDATE name a stream's viewers in `viewer_ids`; Yapfast keeps them
for the stream it shares and the one it watches, and shows them over the
stream (`ViewerStack`) and as a count in the voice panel; a click on it
returns to the call. This account's own stream, "Your screen", has a tile
of its own right before its own, which opens large as others' do, without
watching it through Discord: it shows the worker's preview. The iced
window's order is in `anomi-app/src/project/voice/focus.rs`.

A stream from a phone says how its picture is turned (the video orientation
RTP extension, on a frame's last packet), and Yapfast shows it upright, as
it does a camera: what the stream said last holds for the frames after,
until it says otherwise, and a stream that never says shows as it comes.
The worker scales the picture as it came and converts it a few rows at a
time, turning each strip into its place in the shared frame
(`native/picture.c`), so a turned stream takes no second picture.

Capture runs in a worker process, `yapfast --stream-worker`, so its buffers take
memory only while sharing; the process exits when sharing stops. Playing a
video file runs there too (`Job::Play`, `video_player`), which also keeps
FFmpeg's reading of files people sent out of the window's process. Its
frames carry their time, which the seek bar shows.
`stream_worker` starts it with one job as a JSON line and reads its reports
back. On Linux it starts `/proc/self/exe`, the running Yapfast, since Yapfast's path
names a deleted file once a rebuild or update replaces it. Frames reach the UI through `frames`, a triple buffer in a memory-mapped
file in the runtime directory. The file is unlinked once the worker opens it.
Windows won't delete a mapped file, so there it's a temporary file that
Windows deletes once both processes close it.
The worker's standard error goes to Yapfast's log file. If its output ends
without an `ended` report, as when it crashes, the popup says "Screen sharing
stopped unexpectedly." and Yapfast logs a warning, "the stream worker ended
without saying why", with how it exited (`exit="signal=11 (SIGSEGV)"` on Unix,
the exit code on Windows), the job (`share`, `watch`, `play` or `glance`), the
encoder's `engine` and `codec` from the last stats a share reported, and the
picture `size` (or "up to" the job's bounds before one was reported).
`a_worker_that_dies_says_how` runs a worker script that kills itself, through
`YAPFAST_STREAM_WORKER`, which names the program to start instead of Yapfast.
In a Wayland session, libwebrtc's capturer asks the screen sharing portal what
to share. The portal answers over D-Bus through GLib, so the worker runs GLib's main
context while it waits for the dialog and for frames. Yapfast tells Discord it's
live only once frames flow, so a cancelled dialog shares nothing. Windows has
no such dialog, and libwebrtc uses the portal only on Wayland, so elsewhere,
as on Windows and in an X11 session such as XFCE's, the share menu lists the
screens and windows libwebrtc's capturers find (`stream_worker::shareable`),
and the worker captures the one picked by its id; see the
[Windows guide](../windows-beta.md#go-live-on-windows). On X11 the capturers
read the X server, and Yapfast's own windows are listed too.
`stream_worker::portal_picks` makes the choice as libwebrtc does:
`XDG_SESSION_TYPE` starting with `wayland` and `WAYLAND_DISPLAY` set. The
portal can be asked for one kind: under Wayland libwebrtc's screen and
window capturers are the portal's, asking it for screens alone
(`CaptureType::kScreen`) or windows alone (`kWindow`), and its generic one
for either. `Source::PortalScreen` and `Source::PortalWindow` take the
first two, `Source::Screen` the generic one; elsewhere the first two share
the first screen or window listed. The
ignored `glances_at_the_listed_screens` and `shares_a_listed_screen_and_window`
list, picture, share and switch between a screen and a window, under Xvfb on
Linux; the latter's comment says what the window there needs.
While live, the voice dock's share button switches what's shared without
ending the stream: the worker starts a second capturer (on Wayland, through
the dialog again) and keeps sending the first one's frames. When the second one
gives a frame, it replaces the first. If the dialog is cancelled, the first
one goes on. The encoder takes the new size on the same connection.
`YAPFAST_STREAM_TEST_PATTERN=1` shares moving colour bars instead of a screen.

A camera can be shared too, such as OBS's virtual camera (v4l2loopback on
Linux, its DirectShow filter on Windows). The share menu lists the cameras
[the call's camera](voice.md#camera-video) lists, under CAMERAS, after the
screens and windows; on Wayland, where the portal picks a screen or window, a
"Screen or window" choice comes first, and switching with no camera listed
goes straight to the dialog, without the menu. In the iced window the
cameras are the Go Live dialog's "Devices" tab, as Discord's (below). The worker films the camera through the same
capture module (`camera::Camera::open_at`), asking for the stream's
resolution and frame rate rather than the call camera's 720p at 30, and
copies each picture into one buffer it keeps for the next before the encoder
takes it. Switching between a camera and a screen works as between two
screens. `YAPFAST_CAMERA_TEST_PATTERN=1` lists a test pattern camera to share.

Once Discord has started the stream, Yapfast unpauses it with op 22, as
Discord-video-stream does, and the worker gets its server and sends the video
there (`yapfast_discord::stream_connection`). The connection is a call's
(`voice_connection.rs`), on voice gateway version 8 as Discord's clients use:
heartbeats acknowledge the last sequence number, and binary DAVE messages start
with one. It identifies with the stream's RTC server and `video: true`,
discovers its address, then uses `aead_aes256_gcm_rtpsize` transport
encryption. Yapfast offers H264, and AV1 then H265 before it where the
graphics card it encodes on encodes them ([Encoding on the
GPU](#encoding-on-the-gpu)), and announces its size with op 12, again
whenever someone connects. H264 frames come from x264 (`vendor/x264-src`,
built from source) through `native/h264.c`, with a keyframe at least every 3 s
and whenever RTCP asks for one; [Choosing the H264
encoder](#choosing-the-h264-encoder) says how it's set up and why. Yapfast also
has VP8 (`native/vp8.c`, still built and tested); every Go Live sender known to
work sends H264. H264 goes as whole NAL units or FU-A fragments, H265 as whole
units or FU fragments (RFC 7798), and AV1 in AOM's [RTP payload
format](https://aomediacodec.github.io/av1-rtp-spec/v1.0.0.html), as
libwebrtc's packetizer sends it: OBU elements without their size fields,
temporal delimiters, tile lists and padding dropped, an element too large
for a packet carried on in the next, OBUs of different layers in packets of
their own, and a keyframe's first packet marked as starting a coded video
sequence (N). A frame with a malformed OBU isn't sent. DAVE leaves an AV1
frame's last OBU without its size, its trailer after it, and receivers keep
it so, since DAVE's decryptor reads the frame as it was sealed. Past a
frame's first 8 packets, the rest are paced at 2.5 times the stream's
target bitrate, as libwebrtc's pacer sends, or faster when that wouldn't
send the frame within two frames' time, so a keyframe doesn't hold back the
frames after it until they're dropped: a 1080p keyframe of 200 packets sent
at once can overflow buffers on the way, and fills the sender's uplink. A
frame's packets wait in a queue with when each is due, and go out from a
branch of their own, so the stream's sound, NACKs and receiver reports are
handled while a frame goes out, and a report is timed when it comes; the
next frame is taken once the queue is empty. The queue has no fixed
length: it holds a frame, a keyframe's whole half second's buffer at the
unlock's 50 Mbit/s too, about 2600 packets, which goes out within two
frames at 750 Mbit/s. Each packet is made in one already sent, which the
packetizer takes back once it's out (`Packetizer::recycle`, up to 1024
kept), with room for its header, payload and the transport's tag and
counter, so sealing it doesn't grow it: once a stream runs, sending a
packet allocates nothing, at the five thousand a second of 50 Mbit/s too.
Yapfast keeps about a second of video packets at the stream's cap, at
least the last 2048 (two seconds at 9 Mbit/s) and up to 8192 at 50
(`rtp::History::fit`, which grows the ring when the cap rises and keeps
what it held), each slot reusing its buffer; and when a viewer reports
some lost with a generic NACK (RTCP 205), resends up to 128 of them as
Discord's client does: on the RTX SSRC with RTX's payload type and numbering,
the original sequence number before the payload (RFC 4588), and the repaired
stream id (extension 12) in place of the stream id. An RTCP sender report goes
out every second.

Before sharing, the share button's menu picks a resolution and a frame rate, as
Discord's client asks for them: 720p, 1080p, 1440p or the screen's own size
(up to 4K), at 15, 30 or 60 fps. Its "Higher quality" checkbox, which Discord's client
doesn't have, has x264 work harder on each frame. Yapfast remembers the choice
beside the chosen audio devices (`devices.json`), and starts at 1080p30 without
it.

The iced window (`anomi`) has Discord's Go Live dialog instead
(`anomi_voice::share`): a card in the middle of the window with two tabs.
"Applications", where the portal picks, says "Share your screen, a window or
an application" over two cards, the whole screen and a window, each going
live through the portal asked for that kind alone, and its "Choose What to
Share" button asks the portal for either; elsewhere (X11, Windows) the tab
lists the screens, with their pictures, and the windows to pick from, and
"Start Streaming" shares the one picked. "Devices" lists the cameras and
capture cards to stream, or says none was found. Along the bottom are the
stream's mode and what it sends ("Custom Quality", "720p · 60fps · System
sound") and the gear, whose menu is the window's context menu: "Stream
Mode" with Discord's three modes as presets over the resolution and the
frame rate (Games, 1440p at 60 frames, as Discord's; Screen Share, the
source's size at 15 frames; Custom, what the person set, which choosing a
preset remembers and
Custom brings back), "Screen Resolution" and "Frame Rate" with their
choices, "Mute Stream Audio", and "Advanced", with "Hide Stream Preview"
and what this client has beyond Discord: the bitrate limit (with the
unlock below, past 9 Mbit/s and typed by hand), "Higher
quality" and where streams encode, the choice of Settings' Advanced
(`layout.json`'s `stream_encoder`). Before going live a mode, resolution,
frame rate, limit or quality chosen only changes the dialog, which carries
it to the stream as it starts; the sound, the preview and the encoder are
kept at once.

While live, the share buttons, the gear on "Your screen" in the dock and the
gear on one's own stream's tile open the stream's own menu, as Discord's
client has it: "Stream Quality" with the frame rates, the resolutions, the
encoding and the bitrate limit in its submenu, "Sound", "Change Windows",
which opens the dialog again to pick what to switch to, and "Stop
Streaming". A right click on that tile opens Discord's menu of it: "Stop
streaming" first, in red; "Stream quality" with the resolutions and frame
rates; "Share Stream Audio"; and "More Settings", with "Change Windows" and
the rows of the gear's "Advanced". Discord's "Pop Out Stream" isn't there:
the window has no window of its own for a stream yet, only the corner of
the conversation. A choice in any of them changes the
stream at once
(`voice::Command::Quality`, which out of a stream only remembers the choice)
and the menu stays open; the Slint menu hides them while live. The stream and its connection go on: the gateway's stream ops carry
no quality, and op 12 is the stream's video state, which [the voice
docs](https://docs.discord.food/topics/voice-connections) describe as the
current `streams` with their `max_resolution`, `max_framerate` and
`max_bitrate`; they name no other message for a change. The worker takes the change between two
frames (`stream_worker::Control::Quality`): the next screen frame fits the
new size, the frame limiter counts at the new rate, and a camera shared
opens again at the new size and rate. A new size or frame rate opens the
encoder again (each encoder sets its rate control by the frame rate it
opened with), as a new effort does x264's; the old one closes first, and
the new one starts with a keyframe, at the stream's target, never above
the new size's cap. A new bitrate limit opens nothing: the stream's target
drops under a lower one at once (`StreamSender::set_ceiling`), and the
encoder follows it with its next frame. Op 12, sent before the first frame
of the new size, rate or limit, announces its `max_resolution`,
`max_framerate` and `max_bitrate` with the same SSRCs, so viewers keep
watching; a change the worker can't
take shows in the dock's notice, as other stream errors do. The menu lists
every screen, window and camera, and is never taller than
the window: where they don't all fit, as with many windows open or the window
short or zoomed, the choices scroll as one above Go Live, which stays in view.
Larger screens, and those of another shape, are scaled down to fit, keeping
their shape and never scaled up: 1440p is at most 2560 by 1440, so a 3440 by
1440 ultrawide goes out as 2560 by 1070, its width filling the bounds as at
the other sizes, and op 12 names that size as its fixed `max_resolution`.
Yapfast caps them at the bitrates Discord's client uses: at most 3.5 Mbit/s
up to 720p and 9 Mbit/s above, 1440p and the source's size too, at every
frame rate, with the frame rate as op 12's `max_framerate`. The voice docs'
example op 12 sends 9 Mbit/s at 60 fps as its `max_bitrate`; they name no
cap of 1440p's own, nor what each Nitro tier may stream (Discord's server features
name only boosts' 720p60 and 1080p60, [`VIDEO_QUALITY_720_60FPS` and
`VIDEO_QUALITY_1080_60FPS`](https://docs.discord.food/resources/guild#guild-features)),
so Yapfast offers 1440p to every account, as it does 1080p and the source's
size, and the limit's steps below are 1080p's. The
menu's "Bitrate limit" (Auto, or 2, 3.5, 5, 6, 8 or 9 Mbit/s) lowers that
cap, never raises it without the unlock below: `stream_connection::cap`
takes the lower of the two, and at least 500 kbit/s. Auto is Discord's cap alone. The limit is kept
with the stream's quality in `devices.json` (`StreamQuality::max_kbps`),
reaches the worker with `Job::Share` and `Control::Quality`, and bounds
everything Discord's cap bounds: the target a stream starts at, the
congestion controller below, the encoder's bitrate and op 12's
`max_bitrate`, which announces the cap in force. It's a ceiling, not a
target: the congestion controller still lowers the bitrate when the uplink
queues or loses packets. Discord's client allows
accounts without Nitro 720p30, but trials 1080p30 at 3.5 or 5 Mbit/s for
them.

Past 9 Mbit/s is Anomi's "Unlocked bitrate" in Settings, Advanced, under
Optimization, off unless turned on (`layout.json`'s
`stream_bitrate_unlocked`, written once on). Its note says what it risks:
Discord documents 9 Mbit/s as the most, and what its servers do with more
isn't known, as the fake can't show whether a stream server clamps the
stream, drops packets or closes the connection; and a higher bitrate needs
an upload that fast, and loads the encoder and the viewers. With it, the
limit's steps go on past 9 (12, 15, 20, 30 and 50 Mbit/s), and "Custom
value…" after them, chosen while the limit is none of the steps and saying
it under its name, opens a small dialog in the share layer, in the middle
of the window, in place of the Go Live dialog or the stream's menu
(`anomi_voice::share::entry`): "Bitrate limit", a field with "Mbit/s" after
it, "Cancel" and "Apply". It takes 1 to 50 Mbit/s, with a comma or a point
before up to three decimals (`entry::parse`); an empty field, one that
isn't a number and one out of that range each say why under the field,
which keeps what was typed. Enter applies it as a step chosen is, at once
while live; Escape (once the field lets it go, as in every dialog with a
field), a press beside it and "Cancel" go back to what it came from, the
limit as it was. From the menu of one's own stream's tile, which is the
window's, the layer opens for it alone and closes with it. With the
unlock the limit chosen also shows in the dialog's summary line ("1440p ·
60fps · 20 Mbit/s · System sound"), the stream menu's "Stream Quality" and
the dock's "Your screen" ("1440p · 60 fps · 20.5 Mbit/s"); without it they
read as before. The limit is the cap at every size, 720p's too, up to
50 Mbit/s (`stream_connection::cap`'s `unlocked`); Auto stays Discord's
cap; op 12 announces the limit chosen as its `max_bitrate`. Off, everything
is as before: a limit kept past 9 Mbit/s isn't lost from `devices.json`,
but counts as 9, its step chosen in the menus (`StreamQuality::limit`),
and the next time the unlock is on it's back. The unlock reaches the
worker as where it encodes does: `Job::Share`'s `bitrate_unlocked` for a
stream starting (`stream_worker::unlock_bitrate` keeps it), and
`Control::BitrateUnlocked` for one going, which takes it with its next
frame: the target drops under 9 Mbit/s at once as the unlock goes, and
climbs as the uplink allows as it comes back. The sending thread reads the
limit and its unlock as one word, so it never announces one with the
other's mark.

The worker converts captured
frames to I420 with libyuv, scales them with libyuv's filtering scaler, and
encodes those; the preview is the same picture scaled down to 720p. While
the window isn't in front the UI tells the worker (`Frames::pause`) and the
preview timer stops: the worker neither scales nor converts the preview,
but for the thumbnail Discord is sent. In the iced window Go Live's "Hide
Stream Preview" (`layout.json`'s `hide_stream_preview`) does the same
whatever shows: this account's tile and the corner show no picture of the
stream, saying "Your stream preview is hidden", and the worker makes no
preview for them.

Under the cap, the bitrate follows what the sender's uplink carries
(`yapfast_discord::congestion`). Sending more than it carries fills its
router's queue, and everything else the sender does online waits in it: a
game's ping climbs to hundreds of milliseconds and stays there while the
stream lasts. Discord's client runs libwebrtc's delay-based control on
transport-wide feedback (RTCP 205/15, about the sequence numbers of
extension 5), which Discord's stream server sends a sender on its UDP
connection too, about 9 a second. Yapfast doesn't read that feedback yet; it
measures the round trip from the receiver reports the server sends about the
video, about two a second, which echo the last sender report (`lsr`) with
the delay since (`dlsr`), as RFC 3550 describes, and reads the share of
packets lost from them. A queue at the router lengthens the round trip as
much as the delay it adds.

A stream starts at 2.5 Mbit/s, or its cap when that's lower: Discord's for
its height, or the bitrate limit below it, which a limit set while live
also takes the target down to at once. Each report
whose round trip is within 25 ms of the lowest of the last minute, with at
most 2% of packets lost, raises the target by 8%, up to the cap: about 17 s
from the start to 1080p's 9 Mbit/s, with a report a second, and about 39 s
to the unlock's 50, never in a jump, as each rise is a share of the target
and a cap raised while live only lets it rise. It doesn't rise
past 1.5 times what went out over the last half second, video and resends,
counted in slices of 50 ms, nor before the stream has sent for that long: a
calm picture sends less than the target, and a target raised over it would
be tried on the uplink only once the picture gets busy, all at once. A
round trip more than 60 ms over the lowest in two reports in a row, not one
alone as after a keyframe, or more than 10% of packets lost in one, lowers
it to 85% of what Discord's server received, or of the target when that's
lower, and it doesn't rise again for 3 s. What the server received comes
from its receiver reports at least 0.4 s apart: how far the highest
sequence number it got went, less the packets it counted lost meanwhile, at
the size of the packets sent lately. While a queue builds that's what the
uplink carries, however much more went out, so when a calm picture turns
busy at once and sends far past it, one back-off takes the target under
it. Without a count, as before the second report, it backs off from what
went out over the last half second. From
80% of the rate it backed off from, each rise adds a hundredth of that rate,
so it comes back to where the queue built slowly and stays there longer
before the next back-off; past 115% of it the uplink carries more than it
did, and the target rises by 8% again. It stays under the server's bandwidth
estimate (REMB) when that's lower than the cap, and never goes below
500 kbit/s. Without a measured round trip, as when reports don't echo a
sender report, the target doesn't rise from where it started. The encoder
changes its bitrate only when it differs from the target by more than a
tenth, so a rise reaches it every second step and a fall at once.

Discord shows a still of each stream, in the call and to people outside it.
Like Discord's client, Yapfast makes one once the stream connects and the
picture isn't black any more (a screen is black until what's shared draws),
then every 5 minutes: the picture scaled down to fit 512 by 288, a JPEG at
quality 92. The worker reports it as a data URI, and Yapfast uploads it with
`POST /streams/{stream key}/preview`. A failed upload is tried again a minute
later, up to three times; a newer still replaces an upload not done yet, and
stopping the share cancels it.

In a call, the tile of a stream this account isn't watching shows that
still, as Discord's client does: filling the tile, dimmed, behind "Watch
stream" (`StreamPreview` in `ui/voice.slint`). Without one, as before the
streamer's client uploaded any, the stream's tile shows the screen icon
over "Watch stream". The Slint window shows a stream in its streamer's tile
while they don't film. `GET /streams/{stream key}/preview` answers with the picture's
address, and any failure means no preview, which nobody is told of.
`Discord::stream_preview_picture` asks for the address and downloads the
picture each time, past the picture cache on disk: a preview changes while
its stream goes on, and the address may not. `DiscordCommand::StreamPreview`
decodes it at 512 by 288 at most and answers with `Event::StreamPreview`.
The application (`application/stream_previews.rs`) asks for each stream
when it starts to show a preview: someone in this account's call goes live,
this account joins a call where they already were, or it stops watching
them. Then it asks again every 5 minutes, as often as the streamer's client
uploads one, while the tile shows: [the docs](https://docs.discord.food/resources/voice#get-stream-preview)
give the endpoints but no interval for either side. The window tells it
which streams' tiles are in sight (`Action::VisibleStreams`): in the grid,
uncovered, the window in front, and none behind another window, minimised,
in the tray or with the call off screen. It waits for the moment the next
one falls due (`Application::stream_previews_due`, one abortable wait in
`anomi-app`'s `intent/voice/previews.rs`, nothing ticking meanwhile) and
says when it came (`Action::StreamPreviewsDue`); a tile that comes back
into sight after its preview fell due asks at once, once. The card beside a
streamer's row in the channel list counts as on screen too, in this
account's call or not ([its section](voice.md#what-someone-in-a-voice-channel-is-doing),
`Action::HoverVoicePerson`): its preview is asked for as it shows, again as
it falls due while it stays, and one outside the call goes with it. The Slint window
doesn't say, so there a preview is as old as when it started to show. An
answer that fails, a 404 while the streamer's client has uploaded none or a
second 429 after the request path waited out the first, keeps the picture
there was until the next interval. The decoded picture goes into
`AppState::stream_previews` in place of the old one, which the window shows
without a copy until the new one is there, and lets go of with it; all go
when the stream ends or is watched, or the call is left.

A shared screen carries the sound every other application plays, as Discord's
client and Vesktop share it; Yapfast's own sound stays out, so viewers never hear
the call echoed back (`stream_audio`, with `audio_io::others` hearing the
applications). On Linux, PulseAudio or PipeWire's
PulseAudio server monitors each application's playback stream on its own
(`pa_stream_set_monitor_stream`, as pavucontrol's level meters do), and
streams from Yapfast's processes are skipped by process id. So are other Discord
clients' (Discord, Vesktop, Equibop, WebCord, Legcord, ArmCord, by application
name or program): with one of them in the call too, its sound echoed the call
back to the people in it. The worker mixes them every 20 ms, waiting until an
application has 40 ms queued and keeping at most 200 ms of each, and encodes
Opus at 64 kbit/s, what Discord's client gives a stream's sound. Frames go out
as plain Opus RTP on the stream's audio SSRC, DAVE-encrypted, with sender
reports, and op 5 marks them as shared sound (speaking flag 2). Nothing is
sent while no application plays. On Windows, WASAPI's process loopback hears
everything but Yapfast's own process tree as one stream, from Windows 10's 2004
update on; it can't leave other Discord clients out. Other systems share
video only, and the test pattern shares its bars alone, whatever sound is
chosen. Two ignored tests check the capture against the
machine's sound server with a quiet tone: another process's is heard, and
Yapfast's own is not.

The share menu's SOUND chooses what the stream carries (`voice::StreamAudio`):
all applications, as above, or no sound, and on Windows one application, picked
by one of its windows in a list that opens under its row. A choice applies at
once, before going live or while live, apart from the picture: while live the
menu opens with no screen or window picked, so its Switch button waits for one,
and the voice dock's row for your screen has a button that turns the sound off
and back to what it was. On Wayland with no camera listed, where switching goes
straight to the dialog, that button is how the sound changes while live.
Whether a stream has sound is kept in `devices.json` beside the stream's
quality; an application chosen by its window isn't, as window ids last only
while Yapfast runs, and the next run starts with all applications. The iced
window offers all applications or none: "Mute Stream Audio" in the Go Live
gear's menu and "Share Stream Audio" in the menu of one's own stream's tile,
each taking effect at once; it has no choice of one application yet.

The worker's sound has a thread of its own (`stream_audio::Capture`), started
with the stream's connection, or when sound is first turned on, so a stream
without sound starts no thread. A change (`stream_worker::Control::Audio`)
goes to that thread, which stops what it hears, drops what it heard, and starts
hearing the new choice, while the picture goes on: the capture thread sending
frames never waits for it. The thread keeps its Opus encoder and the sound's
RTP clock, which counts 48 kHz from when the sound started whether anything is
heard or not, so after a change, or a while without sound, a frame's time is
when it was mixed, and stays in step with the picture's on the same SSRC. With
no sound chosen, the thread sleeps until it's told otherwise, and nothing
else wakes it. A sound that can't be heard, such as a window closed since the
menu listed it, is reported (`Report::AudioError`) and shown as the call's
error, and the stream goes on without sound: Yapfast never hears every
application in place of the one chosen. `keeps_the_sounds_clock_while_what_it_hears_changes` checks this with a
listener of its own.

One application's sound on Windows is process loopback's include mode for the
process that owns the window (`GetWindowThreadProcessId`) and the processes it
started, so a browser's sound comes from all its tabs, and an application's
windows all share one sound. Yapfast refuses its own process, a Discord
client's, whose sound would echo the call, and a Store app's window, which the
system's frame host draws, since that process plays nothing. The ignored
`captures_and_encodes_selected_application_audio_on_windows` plays two quiet
tones in processes of their own and checks that all applications, then each
one alone, are heard and survive an Opus round trip; it needs a Windows desktop
with working speakers:

```sh
cargo test -p yapfast-core captures_and_encodes_selected_application_audio_on_windows -- --ignored --nocapture --test-threads=1
```

Each video packet carries the one-byte extensions a Discord client sends: a
transport-wide sequence number (id 5), playout delay (6), screen content
type (7) and the stream id `100` (11). The stream id is what ties a packet to
the stream op 12 declared. These ids aren't the ones in Discord's WebRTC
SDP, where 5 is playout delay: with only playout delay as id 5, Discord's
server drops every packet, and viewers time out with error 2012.

DAVE follows songbird's handshake (`dave.rs`) and encrypts each frame once the
group is ready. A stream's MLS group isn't the voice channel's: its id is the
RTC server id minus one, which Discord-video-stream found too. If the server's
proposals or commits name another group, Yapfast moves to it. The vendored
davey's `set_channel_id` keeps the key package already sent, so the streamer's
welcome still opens. Yapfast accepts the people op 11 names into the group, and
also those named in video attributes (op 12) or in the stream key.

With `YAPFAST_LOG=debug`, Yapfast logs every 5 s while sharing what it sent and the
RTCP that came back (`stream sent`): receiver reports with the highest
sequence number Discord's server got and its echo of our sender reports
(`lsr`), its bitrate estimate (REMB), and keyframe requests, with the last
round trip measured (`round_trip_ms`), the target (`target_kbps`) and what
went out over the last half second (`sending_kbps`) and what the server
received (`received_kbps`). Each
change of the target is logged as "changing the stream's bitrate", with
`kbps`, the `reason`, the report's `round_trip_ms` and `lost` (out of 256),
and the lowest round trip (`lowest_ms`); the encoder following it logs "the
encoder follows the stream's target". The voice connection logs the voice server's RTCP too, to compare
with a sender Discord accepts. Messages the stream server sends are logged
too, except those carrying keys.

Watching works the same way in reverse. Clicking someone's LIVE badge, or their
tile in the call, sends op 20 and starts a worker with a watch job. The worker
connects as a viewer, offers AV1, H265, H264 and VP8 for decoding only (H265
only where it's built, on Linux and Windows), and asks for video with op 15. It
learns the streamer's SSRCs and codec from op 12 and op 14: Discord names the
RTX SSRC only in op 12's `streams`, in the one with the video's SSRC, and
one it doesn't name is the SSRC after the video's. A packet's payload type
gives its codec. On the video's SSRC, a packet whose payload type names none
goes with the codec the frames are already put together in; the codec op 4
or op 14 named, which can be out of date, counts only before there are any.
It then puts RTP packets back in order (`rtp::Reorder`) and together into frames
(`rtp::Assembler`: VP8 descriptors, H264 single units, STAP-A and FU-A, and
H265 single units, aggregation and fragmentation units, as Annex B; AV1's
OBU elements as OBUs, the last without its size, as DAVE's decryptor needs
it and dav1d takes it). An AV1 frame is a keyframe when N marks its first
packet or one of its OBUs is a sequence header, for senders that leave N
out: DAVE encrypts only OBUs' payloads, so their headers show. As
Discord's client does, it asks for a missing packet again at once with a
generic NACK, again every 80 ms up to three times, and takes the resend from
the streamer's RTX SSRC (RFC 4588); padding there, which probes the path, is
dropped. Packets after a gap wait for it at most 300 ms, and at most 4096
of them, 300 ms of a 50 Mbit/s stream with room to spare; one more than
1000 past the highest means the sender started again. A frame still missing
a packet is dropped, and so is everything after it until the next keyframe. So
are frames while none has come yet, and after one the decoder was too far
behind to take. The connection asks for a keyframe with a picture loss
indication, at most every 500 ms. It sends a receiver report every second
(packets lost, the highest received, and the streamer's last sender report
echoed). With `YAPFAST_LOG=debug` it logs what it received every 5 s (`stream
received`): packets out of order, resent, recovered, given up on, and keyframes
asked for. The worker decodes with libvpx or with the FFmpeg H264 decoder in
libwebrtc's archive (`native/decode.c`), AV1 with the dav1d in the same
archive (`native/av1.c`), a picture out for each frame in, or H265 with the
HEVC decoder of `vendor/ffmpeg-src`'s FFmpeg (`native/hevc.c`), which
libwebrtc's lacks, and writes RGBA at up to 1080p into the shared frames.
dav1d takes 0.07 of a core for 1080p30 AV1 at 3.6 Mbit/s, against 0.1 for
H265 at the same bitrate. That FFmpeg is built with
its x86-64 assembly, without which H265 took over three times the processor
time. Watching is possible only in the call you're in, so clicking someone
sharing in another channel joins it first, once asked, and watches once the
call is connected; Discord drops a watch sent before. The stream's sound, Opus
on the streamer's audio SSRC, is DAVE-decrypted as audio and played by the
worker through `audio_io.rs` on the call's speakers (`stream_audio::Player`),
about 100 ms queued; being deafened silences it, which Yapfast tells the worker
with a `hear` control. Yapfast's workers play as "Yapfast", which sharing
leaves out, so a stream you watch never reaches one you share. Watching a
stream from Discord's own client works: H264 with DAVE. Discord's client shows
Yapfast's stream at 1080p with its sound, without the call's.

### Cleaning a stream's sound

A right click on the tile of a stream this account watches opens its menu
in the iced window. The card on top starts as Discord's menu of a stream
does: under "{name}'s screen", "Stop Watching", which stops watching it and
closes the menu, a line, the box "Mute", and the stream's volume, "Stream
volume". "Mute" silences the stream here: the core has no mute of its own
for a stream's sound, so its volume there goes to nothing, while
`layout.json` keeps the volume chosen in `stream_volumes` beside the
streamer in `muted_streams` (Anomi's own key, written once one is muted);
the slider still moves that volume, and the box unticked plays the stream
at it again. Anomi's own rows follow under the card: "Suppress noise",
whose submenu has the suppressors someone's voice may be cleaned with (Off,
RNNoise, DeepFilterNet), and "Remove my echo", for a streamer whose shared
system sound carries the call back, this account's voice with it. Each that's on adds its slider to the card, as for a voice
([Cleaning someone's voice](voice.md#cleaning-someones-voice)): "Noise
suppression strength" and "Echo removal strength". The suppressors are made
for speech and take music and a game's sound apart, which the row says
under it; nothing is on unless chosen. On a stream not watched, whose sound
doesn't play, the menu is its streamer's.

What's chosen is kept by the streamer apart from their voice
(`volume::set_stream_cleaning`), and in `layout.json` as `cleaned_streams`,
in the shape of `cleaned_voices`: `{"1016732398987264102": {"noise":
"rnnoise", "noise_strength": 40, "echo": true}}`. The watch job carries it
to the worker (`Job::Watch`'s `cleaning`), and a change goes with
`Control::Cleaning`, which the player reads once a frame from one atomic.

The worker cleans as the call's thread cleans a voice
(`yapfast_core::stream_cleaning`), on the thread that plays the stream's
sound, after each frame is decoded and before its volume: 20 ms at a time in
one channel, played in mono while anything is on, 10 ms later with RNNoise
and 50 ms with DeepFilterNet, which runs on a thread of its own in the
worker (`their_noise`), its model loaded there the first time it's on while
the stream sounds, RNNoise standing in until it has: a stream may never
pause, so it changes over at once, jumping by the 40 ms between the two.
Off, a frame plays as it came. Nothing runs while the
stream is silent, since no frame comes; the denoiser goes 10 s after its
last sound, the echo gate after a minute, either as soon as it's turned
off, and all of it with the worker, as the stream ends or isn't watched.
After its buffers are made, a frame allocates nothing and takes no lock.

The echo gate needs what the call sent, which the worker doesn't hear. The
call's thread measures it once a tick for every echo gate, its own and the
stream's (`echo_gate::Measure`), only while one is on, and hands the 36
bytes of its levels by band to a bounded channel that never waits
(`stream_worker::EchoFeed`); a thread of its own writes them to the worker
as a `Control::Echo` line, and the line's reader hands them straight to the
player, past the job's loop. The gate takes each tick as it comes and hears
the stream as it comes (`EchoGate::heard_unpaced`), finding the lag between
the two as it finds a voice's, up to 1.5 s. The feed ends when its box is
unticked, or by itself once the worker is gone.

The fake server (`testing/fake_discord.ts`) checks what a stream sends, without
DAVE, and streams its recordings to viewers; its header lists the controls
that add a viewer or change a stream's codec. A streamer's op 12 in another
shape than the voice docs' Video payload (a field they don't name, other
SSRCs than Ready gave, no fixed or source `max_resolution`, a frame rate
outside 1 to 60) closes its connection with 4020, Bad request.

## Choosing the H264 encoder

OpenH264, in libwebrtc's archive, doesn't suit Go Live: on anything that moves
it drops frames rather than lower their quality, and with frame skipping off
its rate control runs far past the target.

x264 encodes every frame within the bitrate. `native/h264.c` uses its superfast
preset, or faster with "Higher quality", tuned for zero latency (a frame out
for each frame in, no B-frames or lookahead, a slice for each of up to four
threads), in Main profile, which Discord's client sends (profile-level-id
`4d0033`). Superfast is the default because it costs little processor time;
faster takes about twice as much for about NVENC's quality. Adaptive
quantization is off: it was the one setting that helped both screens and games
against x264's defaults. The bitrate peaks at the target over half a second's
buffer; a second's buffer let a second reach twice the cap, and a two-frame
buffer starved keyframes. Capped CRF ran past Discord's cap.

## Encoding on the GPU

"Encode streams on" in Settings, Advanced, chooses where a stream's encoder
runs (`stream_worker::EncodeOn`, kept in `layout.json` as `stream_encoder`:
`"auto"`, `"software"`, or a card's key such as `"pci:0000:73:00.0"`): Auto,
one of the graphics cards `yapfast_core::adapters` lists with an encoder this
build has, or the processor, x264, which encodes H264 alone. The Slint
window's "Encode streams with the GPU" switch sets Auto or the processor. A
graphics card takes almost no processor time and looks better than x264 at
low bitrates. Intel graphics encode with x264 on Windows so far, and the
camera stays on x264. The choice reaches the worker with each `Job::Share`,
and one sharing takes a change at once (`stream_worker::set_encode_on`,
`Control::EncodeOn`): it closes its encoder, and the next frame opens one on
the new choice with a keyframe, as for a new quality. The stream's codecs
stay those it offered as it started; one the new choice can't encode is
withdrawn, as below.

The worker reads the adapters from sysfs, which loads no driver, and tries
them in order: the one chosen, then Auto's order, then the processor. Auto's
order (`adapters::encoder_order`, which `adapters::auto_encoder` heads) has
the integrated cards first, then the discrete ones, those without a monitor
before those with, and never a laptop's sleeping card, which it would wake
on battery. Integrated first keeps the card a game draws on free: every
encoder takes its frames from main memory, read back from the card the
compositor draws on whichever card encodes, so the integrated one spares the
game's card the upload, a CUDA context and its encoder's time. The cost is
H264's picture, which AMD's VCN encodes worse than NVENC at Discord's
9 Mbit/s. All of a stream's codecs come from one card: the first on which
AV1 or H265 opens, or else H264, offers AV1 then H265 where they open there,
then H264. NVENC opens on the chosen NVIDIA card by its CUDA number (FFmpeg's
`gpu`), counted in PCI bus order as `adapters::cuda_index` counts: the worker
starts with `CUDA_DEVICE_ORDER=PCI_BUS_ID`, where CUDA would count the
fastest first. VA-API opens the card's render node by its
`/dev/dri/by-path` name. AMF can't be pointed at a card while FFmpeg's
Windows build has no D3D11VA, so on Windows an AMD card is AMF's own pick.
Where no adapters can be listed, the worker tries every encoder as before:
NVENC, then AMF, then VA-API on each render node.

When the card chosen isn't there, taken out or an integrated one the
firmware turned off, the stream encodes as Auto would; when a card's encoder
doesn't open, as at a size it can't take, the next card is tried. Either way
the dock says so once, as a stream's error (`Report::Notice`): "The graphics
card chosen for streams isn't there, so … encodes this one." or "… couldn't
encode the stream, so … encodes it." The processor chosen says none of
this, only, as below, when it can't keep up. When
a graphics card's H264 encoder fails during a stream, x264 takes over from
that frame with a keyframe, and the dock says that too; AV1 and H265 have no
other place, and are withdrawn. The worker logs which encoder each stream got
("encoding the stream", `engine` and `codec`), and for each codec none opened
for, why each GPU encoder failed ("no GPU encoder opened"). The call's Stream
tab shows the encoder and the codec Discord chose ([Voice](voice.md)).

The worker runs on the processor's cores Settings chose for Yapfast
([cpu.md](../cpu.md)), in every mode and wherever the stream encodes:
`stream_worker::set_cpus` keeps them, a worker starting is told with a line
after its job, all of them included (`stream_worker::opening`), and those
running at once with each change (`Control::Cpus`, which any job takes, in
the thread reading the lines). With a graphics card encoding, the worker
needs well under one core. x264 stays on those cores too: it slices each
frame over a thread for each of their logical CPUs, up to four
(`cpu::threads`), and a change that gives another count opens it again with
a keyframe, so more cores help at once.

The worker times each frame x264 encodes (`stream_worker::Pace`): over 2 s
at a time, against the time those frames have at the stream's frame rate.
Where x264 takes 90% of it or more, two windows in a row, it can't keep up
on those cores, and the stream sends fewer frames than it asked for; the
worker logs "x264 can't keep up with the stream on the cores chosen", with
the share it took, the frame rate and its threads, and the dock says once
in the stream: "The processor can't keep up encoding this stream on the
cores chosen: they're too few for this quality. Lower the stream's quality
or frame rate, or in Settings, Advanced, choose more cores or a graphics
card to encode streams." While x264 keeps up, and while a graphics card
encodes, it says nothing.

FFmpeg is built from `vendor/ffmpeg-src` into the one binary, with only its
hardware encoders and every symbol prefixed so it can't meet the FFmpeg
inside libwebrtc. NVIDIA's driver, AMD's AMF runtime and libva load only
when an encoder opens: the binary starts without any of them, and needs
NVIDIA's driver 520 or newer for NVENC, and for AMF an AMD driver with AMF
1.5.2 or newer, from about May 2026 on; an older one fails to open, and x264
encodes.

VA-API opens the card's render node, or, where no adapters can be listed,
tries each one (`/dev/dri/renderD128` on), with the libva driver its kernel
driver calls for: radeonsi for amdgpu, iHD then i965 for Intel's. Naming
it overrides `LIBVA_DRIVER_NAME`, which NixOS and others set
to `nvidia` for browsers, and which would otherwise load NVIDIA's decoding-only
driver for every GPU, AMD's and Intel's too. NVIDIA's nodes are skipped:
NVENC encodes there. NixOS keeps libva itself off the library path, so the
Nix package's wrapper and the dev shell put it there (`runtimeLibraries` in
`nix/native.nix`); it's no build input, or webrtc-sys would build LiveKit's
own VA-API encoder into libwebrtc. The AppImage takes the system's libva,
and the Arch package lists it as optional.

The settings match x264's where they can: low-latency tuning (NVENC's p4
preset, `ll`; AMF's `lowlatency` usage and balanced preset), no B-frames, a
frame out for each frame in, Main profile, keyframes every 3 seconds and when
asked, and variable bitrate capped at the target over half a second, which
leaves a calm screen below the cap rather than padding it. NVENC, AMF and
VA-API change their bitrate as they run, without the IDR frame FFmpeg would
insert, with a patch to FFmpeg for each (AMF's sets the target, peak and buffer
on the running encoder). "Higher quality" changes nothing on the GPU: NVENC's
slowest preset looked no better than p4. The level follows the size and the
frame rate: x264, NVENC and VA-API choose it themselves (H264's 5.1 for
1440p60, H265's 5), and AMF, which holds H264 to 4.2 unless told, enough for
1080p60 and not for 1440p's 14400 macroblocks, is told the lowest that takes
the stream (`h264_level` in `native/hwenc.c`: 5.1 for 1440p60, 5.2 for
4K60); its H265 level is the card's own.

Each encoder chooses that level as it opens, by the bitrate it opens at
too: x264 by its VBV's rate and buffer, FFmpeg's VA-API encoders by the
bitrate, NVENC's own choice by it as well, and keeps it while the bitrate
changes. Up to 9 Mbit/s every level a stream's size takes holds the rate,
so an encoder opens at the stream's target as it always has. With the
unlock past 9 Mbit/s, it opens at the stream's cap instead, so its level
holds it, and aims for the target from its first frame
(`Encoder::holding`; `Sink::reach` in the worker): x264 at 50 Mbit/s takes
Main's 4.1 up to 1080p30, 4.2 at 1080p60 and 5.1 at 1440p60; H265 needs
Main tier's 5.2 and AV1 its 5.2 for 50 Mbit/s, which FFmpeg's VA-API
encoders choose by the bitrate, where NVENC's and AMF's choice is the
driver's own; AMF's H264, told from 4.2 up, holds 50.
An encoder whose level was chosen for less than a cap raised while live,
past 9 Mbit/s, opens again with a keyframe, as for a new size; a lower cap
opens nothing. x264 taking over from a failing GPU encoder opens at the
same rate. The rate control's numbers fit 50 Mbit/s: x264's in kbit/s,
FFmpeg's peak and its half second's buffer (25 Mbit) in bit/s as 64-bit
and 32-bit numbers, and the frame buffer, twice the frame's pixels, holds
a keyframe at any bitrate.

Where the card's encoder encodes AV1 or H265, the stream offers them too,
first, AV1 before H265, as Discord's client orders them: each looks better
than the one after it at the same bitrate. When the stream starts, the
worker opens each on the card to see that it opens, and keeps the first: an
encoder FFmpeg has isn't enough, the GPU and its driver must encode the
codec. There is no AV1 or H265 encoder on the processor, so with the
processor chosen, or no card opening, the stream offers H264 alone. Discord picks the codec Yapfast offers
first that every viewer's client decodes (op 4's `video_codec`), and when
someone who can't decode it starts watching, switches the stream with op
14; Yapfast then opens the other encoder and starts it with a keyframe,
dropping frames still in the old codec. The old encoder closes before the
new one opens, here as for a new size or quality: with NVIDIA's encoder,
one closing while another ran crashed the worker five or six seconds later,
in the driver's library. When AV1 or H265 fails mid-stream,
or doesn't open again for a new size or after a switch, Yapfast withdraws
it with its own op 14 `codecs`, marking it not to encode, and goes on in the
next codec offered, H264 last. Another Yapfast watching keeps the stream in
AV1 or H265. Which of Discord's own clients decode H265 and AV1 isn't
checked yet.

AV1 encodes on NVIDIA's RTX 40 series and newer (`av1_nvenc`), on AMD's
RX 7000 series and newer and the Radeon 700M graphics, through AMF on
Windows (`av1_amf`) and through Mesa's radeonsi on Linux (`av1_vaapi`), and
on Intel's Arc and Core Ultra graphics through VA-API (iHD) on Linux. GPUs
before those have no AV1 encoder, so the stream offers H265 and H264. On an
RX 7900 XTX with AMD's driver 32.0.31041.1004 on Windows, `native/hwenc.c`,
built on its own with FFmpeg's Windows build, encoded with `av1_amf`,
`h264_amf` and `hevc_amf`, forced keyframes and a bitrate change included,
and dav1d decoded the AV1; a whole Yapfast hasn't run there yet, nor AV1 on
Linux, NVIDIA's or Intel's. The packetizer was checked against libaom's AV1
of the test recording, which dav1d decodes to the same pictures after the
packets are put back together, and against DAVE, which decrypts it.

AMD's RX 7000 series codes AV1 in blocks of 64 by 16 pixels, and the
picture's size rounds up: 320x180 came out as 320x192, and AMD gives
1920x1080 as 1920x1082. FFmpeg hands the crop on beside the stream, which
RTP can't carry, so viewers may see the extra rows at the bottom; sizes
such as 1280x720 fit the blocks. See AMD's [alignment
documentation](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/wiki/AV1-Encoder#av1-specific-api)
and [the issue about it](https://github.com/GPUOpen-LibrariesAndSDKs/AMF/issues/423).
The Stream tab gives the size Yapfast encodes, before the rounding.

Wine has no AMF runtime, so there AMF fails to open and x264 encodes.
