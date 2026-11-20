# Persistent voice channels

Yapfast can join one encrypted voice channel at a time, wait alone, and keep
the connection while browsing text channels, other servers and DMs. Each join
starts muted and turns the microphone on as the call connects, unless Settings
› Voice & video turns that off. The dock under the rail and sidebar exposes
mute, deafen, audio devices and Leave. A caret beside mute and beside deafen
opens a menu, as Vesktop's: the input or output device chosen, which a click
opens into the list of devices in place, the microphone or speaker volume
(the same as Settings › Voice & video, saved as the slider is let go), and a
way into the voice settings. The input menu adds the microphone's level, and
with Discord linked, its noise suppression. Deafen also mutes the microphone;
undeafening keeps it muted, while unmuting when deafened undeafens too. The
voice view shows the participants and their mute, deafen and speaking state; a
deafened person shows crossed-out
microphone and headphones in the channel list, and only the red crossed-out
headphones on their tile in the call. Clicking a voice channel joins it and leaves
what's on screen as it was, as Discord's client does: the call shows in the
dock, and clicking the channel again, or the channel's line in the dock, opens
it. A voice channel whose chat mentions you opens on its chat as it joins, as
in Discord, and one you can't join opens without joining. Clicking another one
during a call opens a dialog that asks before leaving the current one, and
confirming joins it the same way. Coming back to a server that had a voice
channel selected shows it without joining.

Yapfast captures and plays a call's audio through system devices, with its
own sound devices (`audio_io.rs`). Settings › Voice & video selects the microphone and speakers too.
Yapfast remembers the choice in `devices.json` in its data folder; it never
leaves the machine. A remembered device that is unplugged gives way to the
system default until it returns. Refresh devices in Settings after plugging in
hardware. No microphone track is created until Unmute. Leaving releases the
audio handle, and restarting Yapfast requires joining again. Preview calls are
simulated, but the preview lists and chooses real devices without remembering
them.

## Microphone test

With Discord linked, Settings › Voice & video has a microphone test, as
Discord's Mic Test has: "Test microphone" plays the chosen microphone back
through the chosen speakers, and the bar beside the button shows its level, as
the input sensitivity's does. It plays the microphone as Discord calls process
it, with the noise suppression chosen and its strength, taking out the room's
echo, evening out the voice and the microphone's volume, at the speakers'
volume, and a change to any of them is heard at once. It plays before the voice
gate and push to talk, so no key needs holding, and push-to-talk's key up
doesn't turn DeepFilterNet down to its encoder as it does in a call. Nothing
is sent. Through speakers the microphone hears itself, so the page says to use
headphones.

The test runs on the sensitivity's meter (`GateMeter` in
`crates/yapfast-core/src/discord_audio.rs`): its thread, `yapfast-gate-meter`,
hands each processed 10 ms block to a thread that plays it,
`yapfast-mic-test`, through a queue of four blocks. A full queue drops the
block, so the microphone never waits for the speakers, and speakers whose clock
runs slower than the microphone's can't make the test later and later. The
speakers keep 30 ms queued, give way to the default ones when they won't open,
and open again when they come back, as a call's do; the system lists the stream
as "Microphone test". With the sensitivity's meter running, starting and
stopping the test leave the microphone and its processing as they are, as
choosing other speakers during it does; choosing another microphone opens that
one and goes on playing.

A call leaves the test off, with the button disabled and "Leave the call to
test your microphone.", and one starting stops it, so the test never touches
the call's microphone, mute or deafen. Leaving Voice & video, closing settings
or closing the window stops it too, and coming back doesn't start it again.
Started, it goes on playing with the window behind another.

## Leaving an empty DM call

With Discord linked, Yapfast leaves a DM or group DM call once no one else has
been in it for a while, as Discord's own apps do after three minutes. Settings
› Voice & video › Leave a DM call when you're alone chooses the wait: Never, 3,
10 or 30 minutes, or an hour, three minutes unless changed. `layout.json`
saves it as `alone_timeout`, in minutes, zero for Never; a wait the page
doesn't offer counts as the nearest one it does.

The wait starts when the last other person leaves, or when a call you start
connects with nobody in it yet: those it still rings, shown "Calling...",
haven't come. Someone joining stops it, and the next to leave starts it again.
Choosing another wait during one starts it over, so a shorter one never
leaves at once. Leaving, or switching to another call, stops it; closing the
window to the tray doesn't. When it ends, Yapfast leaves the call as the Leave
button does, with its sound, and while Yapfast isn't in front, a notification
says so, unless notifications are off or you're on Do Not Disturb. Server
voice channels stay until you leave.

The wait is one single-shot timer in `src/ui/alone.rs`, running only while an
empty DM call does. Each action and backend event checks the call with a few
comparisons: its phase, its room's id, which tells a DM's call from a
server's, and who is connected in it.

## Cameras

Settings › Voice & video chooses the camera, beside the microphone and
speakers, and remembers it in `devices.json`. "Test video" films with it in a
preview there until stopped, or until settings close. An unplugged camera
gives way to the first one listed until it returns, as audio devices do.
`--audio-devices` lists cameras too.

Yapfast films through libwebrtc's own capture module, which its libwebrtc
archive already holds (`crates/yapfast-core/native/camera.cc`), through V4L2 on
Linux and DirectShow on Windows. The module reads the device on a thread of its
own. Yapfast registers for the device's raw frames, so the module hands over
the driver's buffer without converting it, and libyuv converts each one (YUY2,
MJPEG, NV12, RGB and the rest) into the same I420 buffer, frame after frame. It
asks for 1280×720 at 30 frames a second, as Discord's client films without
Nitro and as LiveKit's default camera does, and scales down a camera that can't
film that small.

A picture on screen, such as the preview, is a `picture::Feed`: whoever makes
the frames converts each straight to RGBA at the size it shows, in one of at
most four Slint pixel buffers the feed reuses, and the window shows that
buffer without a copy. Off screen, a feed converts nothing and lets go of its
buffers.

In a call that carries video, the camera button in the call's controls turns
the camera on and off. Each participant whose camera is on has a feed of
their own, and their tile in the grid shows it in place of their avatar; the
people list beside the channel marks them with a camera. Your own camera shows
mirrored, as a mirror would. Up to four people take the columns that make
their tiles largest within both the grid's width and height, so a call
above a DM's messages shows both people side by side without scrolling;
more take tiles at least 240px wide, and scroll. A click on a tile shows it
large, as Discord's focus does, with the others in rows of small tiles
beneath; a click beside it shows the grid again. A button on the large tile
(Hide Members) hides the small ones, and shows them again; the grid then
reports no columns, so their cameras stop. The large tile goes full
screen from its button or a double click, over the whole window, until
Escape. Rust keeps which tile is large (`tiles::Focus`), by whose it is, so
it stays as people come and go. The grid tells Rust its columns, tile size,
scroll and large tile whenever they change (`call-tiles`), so each feed is
made at its tile's size in physical pixels, and only while the tile is in
view. A tile scrolled away, or a grid hidden behind settings, makes no
pictures and keeps no buffers. While the call is off screen, one camera
shows in the corner (`StreamPip`), made at the corner's size: the large
tile's, or else, with no stream to show there, the first of someone else's.
The others make none. Skia draws each frame from Slint's buffer in place
(`docs/building.md`). Preview and Discord calls carry video.

A camera takes a moment to open, and longer to send its first picture, so it
opens on a blocking thread (`Camera::start`) while the call goes on. Until
the first picture, the call says the camera is starting: its buttons pulse
with "Starting camera..." while animations play, and your own tile says so
too. Clicking the button
then stops it. Only then does Discord's voice state say `self_video`; a
camera that fails to open turns off again with the reason. Choosing another camera while filming opens it the
same way, after the old one closes. Listing the cameras looks at the devices
once, rather than once for each camera listed.

In a Discord call, Yapfast encodes the camera in H264 itself and sends it over
the call's own voice connection, and decodes each remote camera on one
thread, into its tile's feed; [Camera video](discord/voice.md#camera-video)
has the details.

libwebrtc's module was chosen over V4L2 directly, PipeWire's camera portal
and nokhwa: it adds no dependency, converts once into a reused buffer where
nokhwa makes a new one each frame, and films on Windows too.

The portal is in the same module: `VideoCaptureOptions` with PipeWire allowed
asks it over D-Bus, as screen sharing asks its own portal. Yapfast will turn it
on when it ships in a sandbox; until then V4L2 needs no dialog.

On Windows the same module films through DirectShow (`VideoCaptureDS`), from
the Windows libwebrtc archive, and libyuv flips DirectShow's bottom-up RGB in
the same conversion. The module's device list joins COM's multithreaded
apartment on the thread that opens a camera, and leaves it on the thread that
closes it. A camera there is opened, kept and closed by a thread of its own,
`yapfast-camera`, so no other thread's apartment is joined or left by mistake;
it's one thread more than on Linux, parked while the camera films. A camera's
id is its device path (`\\?\usb#vid_…&pid_…#…`), which stays the same across
restarts; a camera without a serial number may get a new one in another USB
port, and the first camera stands in until it's chosen again. A camera
without a device path, such as a virtual one, goes by its name. Cameras that
only Media Foundation lists, such as some laptops' built-in MIPI cameras,
aren't listed. [Windows beta](windows-beta.md#cameras-on-windows) has what
was checked under Wine.

`YAPFAST_CAMERA_TEST_PATTERN=1` adds a "Test pattern" camera that films moving
bars, for tests and machines without a camera. `cargo test -p yapfast-core
films_a_real_camera -- --ignored` films the first real camera for a second,
or the first whose name holds `YAPFAST_TEST_CAMERA`; a V4L2 loopback device fed
by `ffmpeg -re -f lavfi -i testsrc2=size=1280x720:rate=30 -pix_fmt yuyv422 -f
v4l2 /dev/videoN` works as one.

## Sounds

Yapfast plays its own short sounds when this account joins or leaves a call, and
when others connect or go while it's there. It also plays them on mute, unmute,
deafen and undeafen, and for messages that notify, with a brighter one for
mentions. People arriving together make one sound. Reconnecting makes none, and
someone a DM call is ringing makes one only once they answer. Muting sounds as
it's asked for, rather than once the call confirms it. Turning the microphone
on as a call connects, for the unmute-on-join preference, makes no sound, since
joining already did. While deafened, only deafening's own sounds play. A message
in the conversation in front of you makes no sound, and nor does any on Do Not
Disturb. Someone else starting or stopping watching this account's stream, or
the one it watches, plays the call's join or leave an octave higher, quicker
and quieter, as Discord's client sounds a stream's viewers; a call sound at the
same moment wins, and a stream starting or ending makes no viewer sound.

`sounds.rs` makes each sound as it plays. The six sets are New Kalimba, the
default, Felt piano, Soft marimba, Mellow Rhodes, Glow and Flute, all in F
major pentatonic so any two sound well together. Felt piano, Soft marimba,
Mellow Rhodes and Glow are synthesized; New Kalimba and Flute play the same
notes on one recorded note each, made with ElevenLabs' sound generator and kept
as 24 kHz samples in `crates/yapfast-core/sounds/`, played faster or slower for
each note. They play on the call's speakers through `audio_io.rs`, on Linux and
Windows, like ringing. Settings › Sounds chooses the set and the volume, from
0% to 200%. A slider's percentage becomes a gain by its square, which spreads
the quiet settings along more of it. At 100%, the default, the loudest sounds
peak 12 dB under full scale, and at 200% they reach it. Each sound can also
have its own volume up to 100%, as a share of that one. `layout.json` remembers
them. Desktop notifications ask the desktop to stay quiet, since Yapfast sounds
itself; someone who had turned the desktop's sound off starts with messages and
mentions at 0%.

WebRTC's PulseAudio devices report no GUID, and LiveKit selects devices only by
GUID, which would always choose the first device. Yapfast names such devices
`name:` and their reported name, and selects them by their current index
instead.

A call's own speakers and microphone, and a watched stream's sound, outlast
their device (`audio_io::LastingOutput` and `LastingInput`). Bluetooth
headphones that drop out end the stream on Windows, and on PulseAudio the
server moves it to another device and leaves it there when they return.
So a stream that fails opens again, on its device or on the default while
that's gone, and every 2 s it checks which endpoint it would open now and
moves there if that's another one. On PulseAudio that endpoint is the
sink's or source's name with its index, which a device gets anew each time
it comes back, so even a short gap brings the stream back. On Windows it's
the endpoint id, the default's while the chosen device is gone, so the
default also follows Windows when it changes. While nothing opens, the
stream keeps time with silence and tries again every half second. Settings
show a chosen device that isn't connected last in its list, as "(not
connected)", by the name it was last listed with; the call plays on the
default meanwhile. Each picker's selection is bound both ways, since a
ComboBox sets its own index as it's used, which undoes a one-way binding
and left it showing another device once the list changed.

Right-clicking someone in a call, in the sidebar or on their tile, sets how
loud they sound from 0% to 200%, and their stream's sound too while they're
live. A drag snaps to 100% near it. The sliders follow Discord's, read from its
web client: below 100% the gain is `(slider / 100)^2.8`, so 50% is about −17 dB
and 40% a gain of 0.077; above it each 100% adds 6 dB (`volume::from_slider`).
A linear slider had a music bot at 4% to sound as Discord's 40% did. A
stream's sound plays at Discord's default gain of 0.18, about 54% on the
slider, and voices at 1.0. Yapfast remembers the gains by Discord user id in
`layout.json`, so they hold in every call and after a restart. Discord
voice scales each person as it mixes the call, from a copy of the volumes its
thread takes again only when one changes, and a watched stream's worker
scales its sound. In a Discord call the same menu can also clean their voice
before it's mixed: a submenu chooses what suppresses their noise (Off,
RNNoise or DeepFilterNet, which costs more of the processor), a box turns
down our own echo from their speakers, and the card over the menu then has
a slider for how strongly each works. `layout.json` keeps them by user id
too (`cleaned_voices`); see [Cleaning someone's
voice](discord/voice.md#cleaning-someones-voice). In the iced window, a
right click on the tile of a stream being watched opens the same card for
its sound alone, its volume and how it's cleaned, kept apart
(`cleaned_streams`); see [Cleaning a stream's
sound](discord/go-live.md#cleaning-a-streams-sound).

Settings › Voice & video has a volume under each device, as Discord's
does: "Microphone volume" sets how loud every call sends you, and "Speaker
volume" how loud every call plays, both from 0% to 200% on the same curve
as a person's volume, with 100% marked. Dragging plays each step at once,
and letting go saves the value in `layout.json` (`microphone_volume`,
`speaker_volume`). Everyone in a call, and their stream's sound, plays at
their own volume times the speakers' (`volume::heard`), which Discord voice
mixes with. The
microphone's volume applies with leveling, as one gain ramped across each
10 ms block, and like leveling it never pushes a peak past 90% of full
scale, so a boosted microphone doesn't clip on a shout; leveling hears the
voice before the volume, so changing it doesn't move your usual level.
Yapfast's own sounds keep their volume in Settings › Sounds.

Settings' "Even out your voice", on unless turned off, levels the microphone
after WebRTC's processing. Yapfast follows your usual speaking level over the last
half minute or so, and undoes up to 8 dB of drift from it as you lean towards
the microphone or away from it. Drifts of more than 12 dB, such as whispers and
shouts, pass through as they are, and a shout lets go of any correction at once.
Pauses hold the gain, and raising never pushes a peak past 90% of full scale.
Discord voice levels in its own microphone thread; its voice
activation gate still opens on the level before leveling, so held gain can't
lift noise past it.

Discord calls suppress noise with DeepFilterNet, which also takes out typing;
see the Microphone section of [Discord](discord/voice.md#microphone). There,
Settings can also turn echo cancellation off, for headphones
(`echo_cancellation`).

The input menu's level meter reads the chosen microphone locally through
`audio_io.rs`, raw and before any processing, only while the menu is open. It
works while muted, outside calls and in the preview, and it never touches the
call: muted, nothing is published or sent. It shows as "Input level" in the
system's recording streams on Linux. On Windows it reads the microphone
through WASAPI.

## Listing devices

List native devices without publishing a microphone:

```sh
cargo run --locked -- --audio-devices
```

Push-to-talk and the other [keybinds](keybinds.md) work in calls; see
[Discord's microphone](discord/voice.md#push-to-talk). A call goes on with the
window closed to the [tray](tray.md), whose menu can disconnect it.
