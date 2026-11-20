# Resource measurements

## Vesktop 1.6.7 and Yapfast

| | Vesktop memory | Yapfast memory | Vesktop CPU | Yapfast CPU |
| --- | --- | --- | --- | --- |
| Idle | 724 MB | 30 MB (−96%) | 0.11% | 0.015% (−86%) |
| Image-heavy channel | 706 MB | 41 MB (−94%) | 0.17% | 0.008% (−95%) |
| In a call | 888 MB | 44 MB (−95%) | 0.46% | 0.33% (−28%) |
| Sharing your screen¹ | 1,013 MB | 113 MB (−89%) | 2.15% | 1.1% (−48%) |

Vesktop is basically Discord's own app: the same Discord web app, in the same
Electron, with Vencord on top. So its numbers stand in for Discord's.

¹ Sharing a 1920×1080 monitor at 30 frames a second, while in the call.
Yapfast encodes on the graphics card when it has an encoder, here NVENC
sending H264. Encoding on the processor with x264 instead, it takes 126 MB and
2.5%, a little more CPU than Vesktop.

<details>
<summary>How these were measured</summary>

Measured on 25 October 2026, and screen sharing on 27 October, on the same
PC: an AMD Ryzen 9 9900X (12 cores, 24 threads) with 32 GB of RAM and an NVIDIA
RTX 3090 (driver 580), under NixOS with Hyprland, and each app's default
renderer (the GPU for Vesktop, the CPU for Yapfast). Yapfast is the 0.2.0 Linux
release binary, except while sharing a screen: that's main after 0.2.0, which
encodes on the GPU, or with x264 in place of OpenH264, built for Discord alone
and optimized as the release is.

Memory is what Windows' Task Manager counts: each app's own memory across all
its processes, without the program files and fonts it maps from disk. On
Linux, that's the private anonymous memory of every process in the app, plus
Yapfast's window frames, which Yapfast draws itself. While NVENC encodes, the
NVIDIA driver also maps about 21 MB of shared memory into the stream's process,
which this leaves out, as it leaves out other shared memory. CPU counts the
whole processor as 100%. The percentages compare Yapfast with Vesktop.

Vesktop's window was maximized at 1920×1044. Yapfast's was 1920×1080 on the
test display, and 941×1020 while sharing on the desktop.

- Idle: a DM open, a minute or two after starting.
- Image-heavy channel: scrolled up through about a hundred pictures, then back
  down to the newest.
- In a call: measured from the call's third minute to its fifth, with the
  microphone on and a text channel open. Yapfast's memory held steady from
  the second minute to the sixteenth.
- Sharing your screen: the call, live with one monitor, measured from the
  stream's second minute to its third. Yapfast captures and encodes in a
  second process that runs only while the stream is live; it takes about
  73 MB of the 113 MB. The stream starts in H265, which the test server
  accepts, and switches to H264 a second later, when a viewer who decodes only
  H264 starts watching, as one of Discord's clients would.

Vesktop ran on a real account, which has more servers than Yapfast's, once
for each scenario; its call had one friend talking now and then. Yapfast ran
against the local test server (`testing/fake_discord.ts`), five times for each
scenario, which stayed within 1 MB of each other, and twice for screen
sharing; its call had three people talking without pause. Yapfast shared a
real screen on the Hyprland desktop through the screen sharing portal.

The scripts behind these numbers, with their raw samples, are in the private
`2026-10-25-release-summary` folder beside this file. `suite.sh` ran the idle,
image-heavy and call scenarios through `scripts/ui-session`,
`desktop-share/run.sh` ran screen sharing on the real desktop, and `sample.py`
sampled each app's whole process tree once a second.

</details>

## The iced interface and the Slint one

Anomi's iced window (`anomi`) beside the Slint one (`yapfast`), on the same
core and the same fake account. CPU is the process's, with the main thread's,
where either interface updates, lays out and draws, in brackets; wakeups a
second are the main thread's and all threads'; memory is the anonymous memory
and the window's frames, as above.

| | Slint CPU | iced CPU | Slint wakeups | iced wakeups | Slint memory | iced memory |
| --- | --- | --- | --- | --- | --- | --- |
| Start: first frame | 0.12 s | 0.09 s | | | 28 MiB¹ | 20 MiB¹ |
| Idle on a DM | 0.52% (0.50) | 0.25% (0.23) | 10.7 / 24 | 2.3 / 17 | 37.2 MiB | 30.6 MiB |
| Image-heavy channel | 0.35% (0.35) | 0.00% (0.00) | 8.4 / 18 | 0.1 / 5 | 50.9 MiB | 37.1 MiB |
| Scrolling | 18.7% (17.9) | 19.5% (18.8) | 227 / 810 | 159 / 738 | 50.9 MiB | 38.3 MiB |
| In a call, at rest | 4.9% (0.82) | 3.7% (0.20) | 13 / 599 | 2.6 / 581 | 63.5 MiB | 56.6 MiB |
| In a call, three speaking | 6.4% (0.85) | 4.0% (0.23) | 13 / 648 | 2.6 / 633 | 63.7 MiB | 56.7 MiB |
| Watching a stream | 9.5% (4.6) | 6.2% (2.7) | 57 / 672 | 30 / 705 | 63.1 MiB | 56.0 MiB |
| Busy server², member list shown | 2.30% (1.77) | 1.97% (1.45) | 33 / 90 | 24 / 153 | 43.2 MiB | 29.5 MiB |
| Busy server², member list hidden | 1.15% (0.62) | 0.52% (0.00) | 27 / 78 | 0.0 / 81 | 43.2 MiB | 29.4 MiB |
| Busy server³, in front | 17.1% (16.2) | 3.1% (2.4) | 150 / 452 | 36 / 242 | 43.8 MiB | 29.9 MiB |
| Busy server³, behind another window | 1.07% (0.45) | 0.65% (0.13) | 26 / 79 | 1.0 / 115 | 44.0 MiB | 30.0 MiB |
| Busy server³, in the tray | 0.77% (0.15) | 0.60% (0.00) | 29 / 91 | 1.0 / 111 | 32.0 MiB | 23.9 MiB |

¹ Anonymous memory as the start settles, 0.25 s in for Slint and 0.17 s for
iced; RSS then is 118 and 91 MiB. The start's CPU until then: 0.18 s and
0.15 s.
² 300 more members, one going offline or coming back ten times a second.
³ The same, with someone starting to type twice a second and a message a
second.

The iced window holds back what a busy server sends that it doesn't show:
who's typing elsewhere, and the member list while it's hidden or the window
is behind another or in the tray, which then reach no `update` at all
(`docs/iced/README.md`, "The background rule"). Without that, a hidden list
took 2.25% (1.70 on the main thread, 25 wakeups a second), the busy server
behind another window 1.87% (1.27, 15 wakeups) and in the tray 0.75% (0.12,
18 wakeups). Slint's typing
dots move every frame for as long as someone types in front; iced's stop
after 15 s without input, hence its busy server in front.

Where the iced window costs more:

- Scrolling: 0.8 points more. Each frame lays the timeline's rows out again
  and paints the whole timeline, about 600,000 pixels at this size, a third
  of it text; the renderer redraws only what differs from the frame its
  buffer holds and copies pictures at their drawn size
  (`vendor/iced_tiny_skia/PATCHES.md`, "Anomi's own"). Upstream's iced
  window also damped the touchpad's reports to one scroll a frame (their
  72fcd542: 10.5% to 6.6%), which isn't here.
- More wakeups of all threads in a busy server (153 against 90): each batch
  of the core's events wakes iced's executor thread (`anomi-ui`), held or
  not, about 0.1% of a core.

The member list's churn, shown, costs iced less than Slint (1.97% against
2.30%; the shown list costs iced 1.45 points over its hidden one, Slint
1.15): a presence change moves its member and shifts the rows between,
which iced repaints only where they differ, the nameplates' pictures
copied at their size.

Two threads of the core cost the same in either window:

- In a call, the core's `yapfast-discord` threads take 3 to 3.5% of a core
  at rest, about 220 wakeups a second, and as much in push to talk with the
  key up (2.95% in front and 3.1% in the tray, iced's window, started with
  `scripts/ui-session start --layout '{"input_mode": "push_to_talk"}'`).
  The microphone's thread reads the microphone every 10 ms and runs
  WebRTC's echo canceller (both ways) and gain control, DeepFilterNet's
  encoder (`noise.rs` skips only its decoders when nothing is sent:
  `min_db_thresh` is infinite for `Sent::Never`) and the leveller, then
  sends nothing (`Processing::block` in `discord_audio.rs`); profiled, half
  its time is DeepFilterNet's network and a quarter the echo canceller and
  gain control. Muted, the capture stops, which is the 0.2% seen in
  the tray. Skipping the suppressor and the echo canceller while the key is
  up, and letting them settle for a block or two as it goes down, would
  take most of that; it is the core's to change.
- The `blocking` crate's pool keeps one idle thread waking every 500 ms for
  good (`blocking` 1.7 ends an idle thread only above its 500-thread
  limit): zbus 5 on async-io connects its Unix sockets through it
  (`Task::spawn_blocking` in `address/transport/mod.rs`), for the tray
  (ksni), notifications and the GlobalShortcuts portal. No setting parks
  it. zbus 5.19 built with `tokio` as well as `async-io` uses Tokio for a
  connection made inside a Tokio runtime (`abstractions::use_tokio`), whose
  blocking threads end after 10 s idle: `anomi-desktop` would open its
  session-bus connections inside the core's runtime (which runs I/O; iced's
  small executor runs only timers), and the tray through ksni's Tokio
  flavour. Both windows share `anomi-desktop`, so both would gain the same
  2 wakeups a second.

<details>
<summary>How these were measured</summary>

Measured on 19 November 2026 on one PC: an AMD Ryzen 9 9950X3D (16 cores, 32
threads) with 30 GB of RAM, under Arch Linux (kernel 7.3), each build
`measure`, by `testing/measure_tray.py --bin BUILD --parity dm busy` and
`--parity images scroll call`, on the private display of
`scripts/ui-session --tray` (a headless Weston 15 at 1400×860, its own
PulseAudio and D-Bus, `testing/tray_host.rs` as the tray), against
`testing/fake_discord.ts --servers 4 --dms 20 --members 300 --member-looks
--nitro --extra 400 --images 4`. Both draw on the processor (Slint's
software renderer, tiny-skia) and run the same core. Each figure is the mean
of two runs, each in a session of its own; runs of one build stayed within
0.3 points of CPU and 0.6 MiB of each other, but for Slint's call at rest
(1.3 points) and both scrolling (Slint 1.9, iced 0.6).

Each sample is 20 s, after 15 s for things to settle, with no input
meanwhile. CPU counts one core as 100%, and wakeups are voluntary context
switches (`scripts/measure-process`, which also gives each thread's CPU and
wakeups by name). Both windows draw their frames into shared memory, 9.2 MiB
at this size, gone in the tray. The first frame is the window's first new
buffer in Weston's log of surface commits (`testing/display/seat.c`), from
the process's start; the start settles when its CPU stays under 5% of a core
for a second.

- Idle on a DM: Bob's DM, which the window opens on.
- Image-heavy channel: #general scrolled up through its hundred pictures to
  its start and back down to the newest.
- Scrolling: #general with 60 touchpad scroll events a second, up and down
  by turns every two seconds, as `testing/measure_tray.py` sends them.
- The call: the Lounge, joined in voice activity (the display's microphone
  is silent); then Bob, Carol and Dave speaking a tone without pause
  (`/_control/speakers-start`); then its stage, Bob's stream watched.
- Busy server: #general of Server. `/_control/churn-start` takes one of 300
  members offline or back ten times a second; with `/_control/typing-start`
  someone starts to type twice a second, in #general and in Bob's DM by
  turns, and `/_control/chat-start?every=1000` posts in #general every
  second. Behind another window: `weston-flower` opened over it takes the
  focus. In the tray: the window closed to it.

</details>

### Drawing on a graphics adapter

The iced window on the processor (tiny-skia), on the integrated Radeon 610M
(Automatic, through Mesa's RADV) and on the RTX 3080 (chosen by hand,
NVIDIA's driver 620), idle on Bob's DM (`crate::gpu`, `vendor/iced_wgpu`).
Memory is the anonymous memory and, in brackets, RSS, which counts the
driver's libraries; wakeups a second are the main thread's and all
threads'.

| | Processor | Radeon 610M | RTX 3080 |
| --- | --- | --- | --- |
| In front: memory | 22.2 MiB (96 MiB) | 27.0 MiB (118 MiB) | 61.4 MiB (246 MiB) |
| In front: threads | 16 | 19 | 22 |
| In front: wakeups | 2.3 / 17.4 | 3.8 / 20.1 | 4.3 / 65.9 |
| Behind another window: wakeups | 0.0 / 4.4 | 0.0 / 4.3 | 0.0 / 8.9 |
| In the tray: memory | 18.8 MiB | 20.0 MiB | 20.9 MiB |
| In the tray: wakeups | 0.0 / 4.5 | 0.0 / 4.2 | 0.0 / 4.2 |
| GPU memory | none | 74.9 MiB of the firmware's 512 MiB carve-out, 27.3 MiB of system RAM (GTT) | 84 MiB on the card |

- The Radeon adds Mesa's two shader-cache threads (`yapfast:disk$0`),
  asleep at rest. In front the main thread wakes 1.5 times a second more,
  for the caret's frames: each waits on the GPU's fence and the
  swapchain.
- The 3080 adds NVIDIA's three threads; `[vkps] Update` wakes 43 times a
  second in front and 4 times a second behind another window, with nothing
  drawn: a GPU chosen by hand is the one that breaks "nothing behind a
  game".
- In the tray iced drops the compositor, and with the device and the
  Vulkan instance the loader unloads the driver: no RADV or NVIDIA library
  stays mapped, no driver thread stays, `nvidia-smi` lists no process and
  amdgpu's `fdinfo` no memory. The window opening again loads it again.
  So does a switch from a GPU to the processor in Settings: the driver
  is unloaded at once.
- On the processor no Vulkan library loads at all: `VK_LOADER_DRIVERS_SELECT`
  limits the loader to Automatic's driver, which loads only once wgpu
  starts.

<details>
<summary>How these were measured</summary>

On 19 November 2026, on the PC above with Mesa 26.2.7, a `fast` build
of `anomi` (`fast` leaves only the Slint interface unoptimized), in one
`scripts/ui-session --discord --anomi --tray --renderer settings` session
(Weston 15 headless at 1400×860, its GL renderer on NVIDIA's EGL, so the
Radeon's frames cross to another GPU, as on a desktop whose monitor is on
the discrete card), against `testing/fake_discord.ts --nitro
--member-looks --members 30 --dms 6 --servers 3`, each renderer a fresh
start (`relaunch` with `--renderer software`, none, `--adapter
0000:01:00.0`). Each sample is 20 s after 15 s settling (8 s behind,
10 s in the tray), once per renderer, by `scripts/measure-process`;
`weston-flower` over the window for behind; `window close` for the tray.
GPU memory: `drm-memory-vram` and `drm-memory-gtt` of the process's
amdgpu file in `/proc/PID/fdinfo`, and `nvidia-smi`.

</details>

The Radeon column is its frames presented straight into a Vulkan surface.
Weston on NVIDIA's EGL imported them; mutter on the 3080, the owner's
desktop, doesn't, and ends the connection (below). Since then that path is
only the adapter the desktop composites on.

### Drawn on the Radeon, shown through shared memory

The owner's desktop composites on the RTX 3080, so Automatic draws on the
Radeon 610M into a texture, and only the regions that changed are drawn,
read back and shown through shared memory (`vendor/iced_wgpu/PATCHES.md`,
"Drawn on one adapter, shown through another" and "Only what changed, read
back and shown"). Against the processor (tiny-skia) and the 3080 drawing
straight into its Vulkan surface, in #general of the fake Discord, under
GNOME Shell 51 on the 3080, the window maximised. Frame: the time `present`
takes on the window's thread, mean of every frame, and in brackets the
read-back's wait and copy. Read back: the bytes a frame reads back, mean of
every frame (the whole frame is 4.7 MiB at 1400×868, 18.6 MiB at
3440×1408). CPU: one core as 100%, `anomi` and in brackets gnome-shell.
Wakeups a second: the main thread's and all threads', 4 of which are the
test control's and a blocking pool thread's in every column.

| | Processor | Radeon 610M, shared memory | RTX 3080, direct |
| --- | --- | --- | --- |
| Scrolling, 1400×868: frame | 1.3 ms | 3.6 ms (3.0 ms) | 0.5 ms |
| Scrolling, 1400×868: read back | | 2.1 MiB | |
| Scrolling, 1400×868: CPU | 13.8% (+4.8%) | 10.0% (+5.1%) | 9.0% (+2.6%) |
| Scrolling, 3440×1408: frame | 2.7 ms | 8.5 ms (7.8 ms) | 0.6 ms |
| Scrolling, 3440×1408: read back | | 13.5 MiB | |
| Scrolling, 3440×1408: CPU | 25.0% (+9.0%) | 16.7% (+9.9%) | 10.9% (+2.7%) |
| Hover, 1400×868: frame | 1.3 ms | 3.6 ms (2.9 ms) | 0.7 ms |
| Hover, 1400×868: CPU | 3.9% (+2.5%) | 3.3% (+2.6%) | 3.4% (+2.1%) |
| Hover, 3440×1408: CPU | 3.1% (+2.5%) | 2.8% (+2.3%) | 2.7% (+2.2%) |
| Typing, 1400×868: frame | 0.3 ms | 2.1 ms (1.6 ms) | 0.7 ms |
| Typing, 1400×868: read back | | 139 KiB | |
| Typing, 1400×868: CPU | 1.9% (+0.9%) | 2.6% (+0.9%) | 1.3% (+0.6%) |
| Typing, 3440×1408: frame | 0.3 ms | 3.5 ms (2.9 ms) | 0.7 ms |
| Typing, 3440×1408: read back | | 573 KiB | |
| Typing, 3440×1408: CPU | 2.4% (+0.9%) | 3.0% (+1.0%) | 3.1% (+1.0%) |
| The caret blinking: read back, 1400×868 / 3440×1408 | | 74 KiB / 330 KiB | |
| At rest in front: wakeups | 0.2 / 4.9 | 0.2 / 5.1 | 0.2 / 9.7 |
| Behind another window: wakeups | 0.0 / 4.3 | 0.0 / 4.1 | 0.0 / 8.9 |
| In the tray: wakeups | 0.0 / 4.1 | 0.0 / 4.3 | 0.0 / 4.3 |
| At rest in front: anonymous memory, 1400×868 | 28 MiB | 30 MiB | 69 MiB |
| At rest in front: anonymous memory, 3440×1408 | 32 MiB | 35 MiB | 72 MiB |
| In the tray: anonymous memory, 1400×868 | 24 MiB | 25 MiB | 28 MiB |

- Scrolling reads back the message list alone, not the sidebars, and
  gnome-shell uploads the same regions as from the processor: together
  `anomi` and gnome-shell take less than with the processor, 15% of a core
  against 19% at 1400×868 and 27% against 34% maximised on the owner's
  monitor. The window's thread still waits on the Radeon 3 to 8 ms a frame,
  where the processor draws in 1.3 to 2.7 ms.
- A small change, the pointer over the list or a key typed, costs about
  what it costs the processor, within a point of a core: the read-back's
  fixed wait, 1.5 to 3 ms, outweighs its few bytes. The caret blinking
  costs no measurable CPU on either.
- Hover reads back as much as the rows under the pointer change: a row's
  background lies under its pictures, so a tall message is read back whole.
  The composer's text is read back whenever the window lays out, as for the
  processor (iced makes its editor anew at each layout).
- Between frames nothing runs: at rest, behind a window and in the tray the
  Radeon's numbers are the processor's.
- Drawing every frame whole, as before damage, the Radeon took 4.7 ms a
  frame scrolling at 1400×868 and 10.8 ms at 3440×1408, reading back 4.7
  and 18.6 MiB, with gnome-shell at 6.4% and 16.7% of a core.

<details>
<summary>How these were measured</summary>

On 20 November 2026, a `fast` build of `anomi` with each frame's
`present` and read-back timed (left out of the tree), on `gnome-shell
--headless --wayland --virtual-monitor` (mutter 51.1, "Boot VGA GPU
renderD129 selected as primary": the 3080), with private session and
system buses, input through mutter's RemoteDesktop and screenshots through
`org.gnome.Shell.Screenshot`, against `testing/fake_discord.ts --extra 400
--images 6 --members 30 --member-looks --dms 6 --servers 3`. Each renderer
a fresh start (`--renderer software`, none, `--adapter 0000:01:00.0`), the
window maximised; 10 s settling, 20 s at rest, then by
`scripts/measure-process`, with gnome-shell's CPU from `/proc/PID/stat`:
10 s of a touchpad's scrolling, 60 events a second, up and down in turns
of 2 s; 10 s of the pointer moving up and down over the list, 480 px a
second; 10 s of typing into the composer, six keys a second; 20 s of the
caret blinking. Behind: `weston-terminal` maximised over it. Tray: the
window control's `destroy`. `ANOMI_SHARED_VERIFY=1` held every frame
shown against the frame drawn whole through scrolling, hover, typing, new
messages, menus, the emoji and GIF pickers, Settings, a theme changed,
resizing, maximising, the tray and back and the renderer switched: no
pixel differed.

</details>

## Targets

| Scenario | Ceiling |
| --- | --- |
| Settled idle after the first sync | Under 250 MiB and under 1% of one core |
| Ten-person audio call | Under 400 MiB |
| Inactive window | No continuous rendering |

These are ceilings; aim lower where the numbers allow. Check them on a
machine of about four cores with 8 GB of RAM and integrated graphics, and on
real Windows.

## How to measure

Measure a `measure` build of the release features, not `fast`, which leaves
the Slint interface unoptimized. Use `release` only when the question is the
shipped build's absolute number.

```sh
nix develop path:./nix --command cargo build --locked --profile measure --no-default-features --features desktop,discord --bin yapfast
```

`scripts/ui-session start --discord --profile measure` builds the same and
runs it against `testing/fake_discord.ts` on a private display; `--bin PATH`
runs a build saved outside `target` instead, so two builds can alternate.

Report memory as the table above does: what Windows' Task Manager shows, the
private working set, and on Linux the app's `RssAnon`, plus `VmSwap` when the
host swaps. RSS and PSS also count the mapped code and fonts, and they fall
whenever the kernel reclaims those pages, so they mislead on a busy host.

To compare two builds, use glibc's heap in use, which reclaim doesn't move:
append `stats` to `.local/ui-session/window-control`, and Yapfast writes
glibc's `malloc_stats` to `.local/ui-session/yapfast.log`.

`scripts/measure-process PID --seconds N` samples one process once a second
and prints the mean and peak RSS, mean PSS and CPU, with one core as 100%,
the main thread's CPU, and wakeups a second of the main thread and of all
threads, with each thread's CPU and wakeups by its name; it doesn't read
`RssAnon`. For a process's lifetime peak, read `VmHWM` from
`/proc/PID/status`. `testing/measure_tray.py` measures anonymous memory plus
swap and heap in use with #general open, closed to the tray and in a call;
it takes `--bin` for a saved build, and with `--parity` measures instead what
the table of the two interfaces holds, for either build.
`testing/measure_discord_cameras.py` covers a Discord call's cameras.

Alternate the builds over several runs and give the spread between runs of
one build. Keep the hardware, workload, warm-up, window state and memory
metric with every result. Dated records of past measurements are in the
private `records.md` beside this file.

