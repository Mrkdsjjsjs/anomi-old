# Development

How to build Yapfast, try it, and test it locally. [Building](building.md)
covers the toolchain, Cargo features and packaging in depth; the
[architecture](architecture.md) shows how the code fits together.

## The development shell

On NixOS, or anywhere with Nix, enter the pinned environment from the project
folder:

```sh
nix develop path:./nix
cargo run --locked
```

`path:./nix` also works before files are tracked and keeps Cargo output out of
the Nix source snapshot. Run the `cargo` commands below inside the shell;
[the shells](building.md#shells) says what each one provides. Local work
builds with `--profile fast`.

```sh
scripts/check --fast
cargo build --locked --release
```

`scripts/check` enters the Nix shell itself when Nix is installed; [building](building.md#checks)
lists what it covers.

## Try the UI without an account

```sh
nix develop path:./nix --command cargo run --locked -- --preview
```

Use the upper-left DM button for direct messages. Server buttons sit below it;
the sidebar shows only the selected server's channels. Filter that list, and
send local preview messages with Enter or the Send button. Preview messages
disappear when you leave the preview. "Back to sign in" opens the sign-in
screen, which links Discord. Normal launch connects with the Discord account
linked on this device. Use `--data-dir PATH` for a separate test session.

Closing the window leaves Yapfast running in the system tray, with any call,
where its icon opens it again or quits; without a tray it quits Yapfast
([the tray](tray.md)). For Yapfast's icon on its window and in the launcher,
run `scripts/install-desktop-entry` once after a release build
([the app icon](packaging.md#the-app-icon)).

Discord runs only against the fake server, `testing/fake_discord.ts`, in
development; `scripts/ui-session start --discord` starts both on a private
display ([seeing the app](building.md#seeing-the-app)).

## Renderer

Yapfast renders with Skia Software, which uses about half the memory of Skia
GPU. To use the GPU:

```sh
nix develop path:./nix --command cargo run --locked -- --preview --renderer gpu
```

Omit `--preview` to sign in or restore your real account. If the GPU cannot
start, Yapfast restarts itself with Skia Software and explains why
([renderers](slint-client.md#renderers-and-native-builds)). Renderer
selection does not change the linked account or the native audio backend.

## Windows

Build the Windows packages from this Linux checkout:

```sh
nix develop path:./nix#windows --command scripts/build-windows
```

The ZIP and an installer appear in `target/dist/discord`. See the
[Windows guide](windows-beta.md) for
two-machine checks, and [building](building.md#windows) for the options and
native Windows prerequisites.

## Measurements

Memory is a product goal. The [resource measurements](measurements/README.md)
record their workloads and limits, and how to repeat them.
