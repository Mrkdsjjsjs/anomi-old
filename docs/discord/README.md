# Discord in Yapfast

| Area | Works on Linux | Open |
| --- | --- | --- |
| [Linking](#account-linking) | QR code on the sign-in screen; unlinking by signing out; Discord's captcha, in the browser | A captcha solved against real Discord |
| [Voice](voice.md) | Server voice channels, [a card of who is in one](voice.md#who-is-in-a-voice-channel), [how long each one's call has gone on](voice.md#how-long-a-channels-call-has-gone-on), mute, deafen, per-person volume, [moving people to another channel](servers.md#someones-menu-in-a-voice-channel) from their menu or [by dragging them](servers.md#dragging-someone-to-another-voice-channel), voice gate with its sensitivity, [a microphone test](../voice.md#microphone-test), [push-to-talk](voice.md#push-to-talk) and [keybinds](../keybinds.md) for mute, deafen and the input mode, [DeepFilterNet noise suppression](voice.md#microphone) and its strength, taking a room's echo out, [camera video](voice.md#camera-video), [the soundboard](voice.md#soundboard) | A check of Windows audio, cameras and keybinds on real Windows, and of keybinds on real Wayland desktops |
| [DM calls](voice.md#dm-calls) | Starting, ringing, answering and declining DM and group DM calls, with cameras | A check by hand through real Discord |
| [Go Live](go-live.md) | Sharing a screen with its sound, watching streams | A check on real Windows |
| [Channels and messages](messages.md) | History, live updates, sending, replies, edits, reactions, files, embeds, custom emoji, stickers, spoilers, unread state and [marking a message unread](messages.md#mark-unread-and-message-links), notifications and their settings, member lists, pinning messages, pins and search, a channel's topic and header, server, category and [channel menus](servers.md#a-channels-menu), [forwarding](messages.md#forwarding), pinning channels to the top, [following channels](servers.md#following-channels), invites, [joining by invite](servers.md#joining-by-invite), [scheduled events from their links](servers.md#event-links), Discord's own channel, message and profile links, making and changing channels, [server settings](server-settings.md)' Server Profile, Engagement, Boost Perks, [Emoji](server-settings.md#emoji), [Stickers](server-settings.md#stickers), [Members](server-settings.md#members), [Roles](server-settings.md#roles), [Invites](server-settings.md#invites), [Safety Setup](server-settings.md#safety-setup), [Audit Log](server-settings.md#audit-log), [Bans](server-settings.md#bans) and [Delete Server](server-settings.md#delete-server-and-transfer-ownership), [channel settings](server-settings.md#channel-settings) and their permissions, [two-factor authentication](server-settings.md#two-factor-authentication), threads, a channel's list of them and starting them, [forums](servers.md#forums), leaving servers, [bots' buttons, dropdown menus and forms](messages.md#bots-buttons-menus-and-forms), their radio groups, checkboxes and file uploads included, [apps' slash commands](messages.md#apps-commands), [context-menu commands](messages.md#apps) and [who used one](messages.md#who-used-an-apps-command) | The rest of a server's settings and its privacy, membership forms when joining |
| [DMs](people.md#dms) | DMs, group DMs, message requests and spam, starting a DM, [a DM's menu](people.md#a-dms-menu): pinning, muting, closing and leaving a group | |
| [Friends](people.md#friends-and-requests) and [profiles](people.md#profiles) | Friends list, friend requests, adding by username, profile cards with their badges, others' server tags and their servers' cards, the DM panel, custom status, [editing one's profile](people.md#editing-ones-main-profile), with its theme, name style and server tag, and [server profiles](people.md#editing-ones-server-profile), notes, friend nicknames, removing friends, ignoring, blocking and unblocking, also [from a profile](people.md#profiles), [the blocked list](people.md#friends-and-requests) | A list of the ignored; changing the username |

Each area has a doc of its own: [voice](voice.md) and calls, [Go
Live](go-live.md), [channels and messages](messages.md), [servers](servers.md)
with their menus, invites, channels, threads and forums, [server
settings](server-settings.md) with channel settings and two-factor
authentication, and [DMs, friends and profiles](people.md) with presence, game
detection and Spotify. The [later list](#later) has what's left to do, and
[adding a feature](adding-a-feature.md) walks a new one through each layer.

Yapfast connects to Discord directly, so a person who links their Discord account
doesn't need the Discord desktop app. Yapfast joins Discord voice as that person,
shows their servers, channels and DMs, and sends and receives messages. Yapfast must still be complete without Discord: nothing
Discord-related appears until someone links an account, and Discord work doesn't
replace beta features of Yapfast itself.

Protocol references: Discord doesn't document what its own client sends. The
community's
[discord-userdoccers](https://github.com/discord-userdoccers/discord-userdoccers)
does, for example in `pages/gateway/gateway-events.mdx`,
`pages/topics/read-state.mdx`, `pages/resources/relationships.mdx`,
`pages/remote-authentication/desktop.mdx` and `pages/topics/cloud-uploads.mdx`.
The settings protobuf is in
[discord-protos](https://github.com/discord-userdoccers/discord-protos).

## Risk to accounts

Discord's terms don't allow third-party clients to use personal accounts.
Bans are uncommon, but they happen, and Discord can change its protocol and
break Yapfast at any time. Linking is opt-in per person. Yapfast shows this warning
before linking, and each person decides for their own account.

Behave like the official client to keep the risk low. Respect rate limits,
never act without a user action behind it, and don't poll. Keep all Discord code
in one crate, so a protocol change breaks only that part.

### Staying within Discord's limits

Yapfast follows the rules Discord documents for every client:

- **HTTP rate limits.** Each request's route maps to the bucket Discord names in
  `X-RateLimit-Bucket`. Discord counts a bucket apart for each major ID, the
  channel, server or webhook, so Yapfast keys its count by both: sending in one
  channel doesn't hold back another. Yapfast waits for an empty bucket to reset
  before it asks again. It also waits after a global 429, marked so in the
  headers or the body. If Discord still answers 429, Yapfast waits the body's
  `retry_after`, or `Retry-After`, or a second when Discord names neither, and
  tries once more. A wait longer than a minute fails at once, with a message to
  try again later.
- **Invalid requests.** Discord bans an IP that sends 10,000 requests in a
  10-minute window answered 401, 403 or 429. Yapfast doesn't ask for what it knows
  will be refused: a channel the account's roles hide shows a notice instead
  of fetching messages.
- **Gateway resume.** When the connection drops, Yapfast resumes the session at
  READY's `resume_gateway_url` with op 6 and the last sequence number, and
  Discord replays what was missed. Yapfast also resumes after op 7, after op 9
  with `d: true`, and when a heartbeat goes unacknowledged. It identifies again
  only when Discord won't resume: op 9 with `d: false`, close codes 4007 and
  4009, or three failed attempts. After op 9 it waits a random one to five
  seconds first, or longer when sessions keep ending. Opening a connection,
  Discord's HELLO and each send give up after 10 seconds, so a dead connection
  fails rather than hangs. Yapfast drops a connection without a close frame,
  because a close with 1000 or 1001 ends the session. Quitting closes it
  with 1000 for that reason, after leaving a call
  ([Quitting during a call](voice.md#quitting-during-a-call)).
- **Gateway send limit.** A connection may send 120 commands a minute and is
  closed with 4008 past that. Yapfast holds commands back at 110, so heartbeats
  always fit. The connection runs in a task of its own, so heartbeats go on
  time while commands wait.
- **Backoff.** Reconnects back off from 2 to 60 seconds, and start over only
  once a session has stayed up for a minute. Each wait is a random 50% to 100%
  of the step, so clients cut off together don't return together. Yapfast sends
  at most 10 IDENTIFYs an hour: an account that keeps identifying gets flagged,
  so past that Yapfast stops reconnecting and says so until it restarts. The first
  heartbeat comes after a random part of the interval, as Discord asks.
- **Typing.** Mention search waits 300 ms after the last key before it asks
  Discord for members, so typing a name sends one search, not one per letter.
- **Cookies.** Yapfast keeps the cookies Discord's servers set and sends them
  back where they apply, as any HTTP client with a cookie jar does. These are
  Discord's device cookies and Cloudflare's rate limiting and bot management
  cookies. One jar serves the whole process. Persistent cookies are saved,
  readable only by the user, in `discord-cookies.json` in the data directory,
  so Yapfast stays the same device across restarts. Yapfast can't pass a Cloudflare
  challenge or captcha, so it never has a `cf_clearance` cookie. Requests to
  other sites, such as Spotify, the GIF providers and Google's storage that
  uploads go to, go through a client without the jar, so they get none of
  Discord's cookies and leave none of theirs.
- **Headers.** Every API request carries what the web client's carry, besides
  the browser's own ([how Yapfast presents itself](#how-yapfast-presents-itself)):
  `X-Super-Properties`, the same description of the client as the gateway's
  identify, `X-Discord-Locale` for English, and `X-Discord-Timezone` with
  this machine's time zone. `X-Debug-Options` stays off, as it does in the
  web client until a debug option is on.
- **Downloads.** Pictures and files from Discord's CDN download eight at a
  time, over one HTTP/2 connection, as a browser's requests to one host go.
  Avatars, server icons, banners, emoji and stickers have eight turns of
  their own, apart from previews and files, so neither waits behind the
  other. Their newest batch goes first, in its order: rows on screen now
  come before rows scrolled past, and opening the emoji picker queues its
  pictures in the picker's order instead of asking for hundreds at once.
  A start fetches the sticker picker's pictures ahead into the disk cache,
  which decodes none of them and holds none.
  One kept on disk shows without waiting for a turn. A file someone asked
  for, such as the image the viewer shows or one they save or copy, skips
  the queue. A
  download that gets nothing from Discord for 20 seconds gives up, so a
  connection lost to a network change or sleep doesn't hold the queue; while
  requests wait, an HTTP/2 ping that goes unanswered closes that connection.

## How Yapfast presents itself

Yapfast is Discord's web client in Firefox, on the system it runs on:
Firefox 151 on Linux (`X11; Ubuntu; Linux x86_64`) or on Windows 10
(`Windows NT 10.0; Win64; x64`). Every HTTPS request and WebSocket it makes,
to Discord and to anyone else, goes through `crates/yapfast-http`: wreq makes
the connection over BoringSSL, and wreq-util's profile of that Firefox gives
its TLS ClientHello (cipher suites, extensions and their order, groups, key
shares, signature algorithms, ALPN, certificate compression, ECH GREASE:
what JA3 and JA4 read), its HTTP/2 SETTINGS, window update, priorities and
pseudo-header order (the Akamai fingerprint), and the headers a typed
address sends. `yapfast_http::PROFILE` and `PLATFORM` choose the profile.
The User-Agent, the version, `Accept-Language` (`en-US,en;q=0.5`) and the
`browser`, `browser_version`, `browser_user_agent`, `os` and `os_version`
of the gateway's identify and `X-Super-Properties` all come from it
(`client.rs`), and `claims_the_browser_the_connections_are` fails if they
part. The version is the profile's, behind the newest Firefox: claiming one
whose TLS and HTTP/2 Yapfast can't show would give it away. A newer profile
is `PROFILE` and wreq-util's version.

Each request carries the headers Firefox gives its kind
(`yapfast_http::Kind`), in Firefox's order (`yapfast_http::ORDER`), whatever
order the code adds them in, as if from the web client's page,
`https://discord.com/channels/@me`:

- The API: a script's request, with `Accept: */*`, `Accept-Encoding: gzip,
  deflate, br, zstd`, the web client's own headers, `Origin` on all but GET,
  the page as `Referer`, `Sec-Fetch-Dest: empty`, `Sec-Fetch-Mode: cors`,
  `Sec-Fetch-Site: same-origin` and `TE: trailers`. Over HTTP/2 each cookie
  goes in a field of its own, as Firefox splits them.
- Pictures and files, from Discord's CDN, its media proxy or the GIF
  providers: a picture's, with Firefox's image `Accept`,
  `Referer: https://discord.com/`, `no-cors`, `cross-site` and
  `Priority: u=5, i`. A video's parts: a video's, with its `Accept`, a
  `Range` and no `Accept-Encoding`.
- WebSockets, the gateway's, voice's, Go Live's, the login service's and
  Spotify's: an HTTP/1.1 upgrade with `Origin: https://discord.com`,
  `Connection: keep-alive, Upgrade`, `Sec-Fetch-Mode: websocket` and
  `Pragma` and `Cache-Control: no-cache`; the frames then go through
  tokio-tungstenite, over the patched `vendor/tungstenite`.
- The web client's page, read for its build number, and the update check:
  an address typed into a tab, the profile's own headers.
- Spotify's API and the uploads to Google's storage: a script's cross-site
  request, with `Origin`, through the client without Discord's cookies.

Where Yapfast still differs from that Firefox:

- No HTTP/3. Firefox moves to QUIC once a server's `Alt-Svc` offers it, as
  Discord's Cloudflare does; Yapfast stays on HTTP/2.
- A WebSocket's connection offers the ALPN `http/1.1` alone, since wreq
  offers the version a request asks for, so JA4 reads `h1` there. Firefox's
  offers `h2` and `http/1.1`, and may carry the WebSocket over HTTP/2.
- No `Sec-WebSocket-Extensions: permessage-deflate`: tungstenite can't read
  the compressed frames a server that took it would send.
- No CORS preflight before Spotify's requests and uploads, and no
  `Priority: u=1`, which Firefox gives a request a click or key starts.
- The `Referer` of the API is always `/channels/@me`, not the page shown.
- Voice selects Discord's `udp` protocol, as its desktop apps do; the web
  client in a browser can only use WebRTC.

`yapfast-fingerprint` shows what a fingerprint echo service sees of the
connection, with no login, super properties or Discord cookie:

```sh
nix develop path:./nix --command cargo run --profile fast -p yapfast-discord \
  --bin yapfast-fingerprint -- https://tls.peet.ws/api/all
```

It prints JA3, JA4, the Akamai fingerprint and the headers in the order the
service saw them. Open the same address in Firefox 151 on the same system
and compare; `script`, `image` and `http1` after the address ask as the web
client's script, as a picture, or over HTTP/1.1 as the WebSockets'
connections do. Against the fake Discord the same clients speak plain
HTTP/1.1 and `ws://`.

## Account linking

Link by QR code, the method mautrix-discord uses. Yapfast shows a code from
Discord's remote login service, and the person scans it with the Discord phone
app. Email/password login isn't offered. Yapfast links on its sign-in
screen, and signing out unlinks. Yapfast first says
that Discord doesn't allow third-party clients, so linking puts the account at
risk, and shows the code only when asked.

HTTPS requests and WebSockets alike trust the certificates the system trusts,
which `rustls-native-certs` reads once into BoringSSL's store, so a reverse
proxy whose CA is in the system's store works; see [how Yapfast presents
itself](#how-yapfast-presents-itself).

The remote login gateway (`wss://remote-auth-gateway.discord.gg/?v=2`, with
Discord's web origin) works as Discord's own login page does,
`yapfast_discord::remote_auth`:

1. Yapfast makes a 2048-bit RSA key and sends its public half, as base64 DER.
2. Discord sends a nonce encrypted to it (RSA-OAEP with SHA-256), and Yapfast
   answers with the SHA-256 of the nonce, base64url, which proves it holds the
   key.
3. Discord sends a fingerprint, and the code carries
   `https://discord.com/ra/{fingerprint}`.
4. Scanning it sends who scanned (`id:discriminator:avatar:username`, encrypted
   to the key), and Yapfast shows "Confirm on your phone".
5. Confirming sends a ticket, which `POST /users/@me/remote-auth/login` trades
   for the login, encrypted to the key again.

The gateway closes the connection about two minutes after the code shows, and
Yapfast connects again for a new code while the code is on screen. Saying no on
the phone stops linking, with "Try again". The key lives only in memory for the
one attempt. The `rsa` crate carries the Marvin timing advisory
(RUSTSEC-2023-0071), which needs an attacker to time many decryptions; Yapfast
decrypts three messages from Discord per attempt. Linking connects Discord at
once, without restarting: voice reaches it through a slot the chat backend
fills. When Discord can't start then, or as Yapfast starts, such as when the
credential store can't be read, Yapfast says why. What waits on Discord, such as
a friend request, the custom status or a profile, fails with that reason
rather than waiting, as it does when the chat backend stops.

Discord may ask for a captcha when trading the ticket, as it does of logins
from devices it doesn't know ([captcha
handling](https://docs.discord.food/topics/captcha-handling)): a 400 with
`captcha_key`, an hCaptcha site key that changes, and often `captcha_rqdata`,
`captcha_rqtoken` and `captcha_session_id`. Yapfast has no web view, so
`captcha.rs` serves a page with the challenge on `127.0.0.1`, at a random path,
and opens it in the browser; the dialog says so and can open it again. The page
loads hCaptcha's `api.js` with `host=discord.com`, as hCaptcha's mobile SDKs
name the host they stand for, sets the rqdata, and posts the solution back.
Yapfast trades the ticket again with `X-Captcha-Key`, `X-Captcha-Rqtoken` and
`X-Captcha-Session-Id`, keeping the login gateway's heartbeat going meanwhile,
and shows a new captcha up to three times if Discord doesn't take one. The
listener lives only while the page waits, so nothing stays loaded afterwards.
Whether Discord takes a solution made this way awaits a real captcha. [Joining
by invite](servers.md#joining-by-invite) shows Discord's captchas the same way.

Store the Discord token in the OS credential store. The token gives full
control of the account, so it never leaves this device. A device that links
appears in Discord as its own session.

Unlinking a device, by signing out, leaves any Discord call, ends the session
with `POST /auth/logout`, and deletes its token.

While the connection is lost, Yapfast waits before connecting again, longer
each time a session ends soon after it began (`gateway::Reconnects`). The
Reconnect button, in the account panel and on Settings' account page, ends
that wait at once (`Discord::reconnect`); connected or connecting, it changes
nothing, and the cap on how often Yapfast identifies holds.

## Later

In rough order. The browser covers these until then.

- Sorting and filtering a forum's posts by tag, and the gallery layout of
  forums and media channels.
- Renaming a server folder and changing its colour in the Slint client
  (Anomi's iced client has them, and dragging servers into and out of
  folders is in both: [servers](servers.md#the-server-list-and-its-folders)).
- A server's membership form and onboarding questions when joining by
  invite, and invite cards in messages.
- A server's privacy settings: direct messages, message requests and
  activity status from its members.
- Discord's voice settings beyond those above, and push-to-talk's sounds,
  key combinations as its key, and more than one key.
- A server's soundboard sounds in its settings.
- Activities, and the rest of a server's settings.
- The blocked on the friends page, beside Online and All.
- Animated custom emoji, which show their first frame.
- Copying custom emoji: copied text has their private code points, not
  `:name:`.
- A custom status's end while Yapfast is closed, or set from a device whose
  clock is off: Yapfast trusts the time Discord gives.
- Opening embed pictures from a card in the viewer with their neighbours.
- A smaller READY. Yapfast identifies with an empty
  `client_state.guild_versions`, so every connect brings every server in full.
  Discord's web client, when it has a local cache, sends each server's
  version from its last READY, with `highest_last_message_id`,
  `read_state_version`, `user_guild_settings_version`,
  `user_settings_version`, `private_channels_version` and `api_code_version`,
  and Discord leaves out what hasn't changed. Yapfast would keep those versions
  beside its saved snapshot, fill in what READY leaves out from it, and the
  fake would answer the same way.

## Memory

The Discord connection and its server state count towards the same targets as
the rest of Yapfast: under 250 MiB settled idle and under 400 MiB in a ten-person
call. Large servers carry big member lists, so request members only for the
channel on screen. Measure settled idle and a Discord call with the method in
[measurements](../measurements/README.md).

## Testing

`testing/fake_discord.ts` is a local Discord: the gateway, the parts of the
HTTP API Yapfast uses, a voice server that answers the handshake without DAVE,
Go Live signalling, and the remote login gateway. Set
`YAPFAST_DISCORD_TEST_SERVER` to it, and Yapfast uses a test token instead of
the linked account, kept in memory. With `YAPFAST_DISCORD_TEST_UNLINKED=1` too,
Yapfast starts unlinked and links against the fake, whose controls scan and
confirm the code. The fake resumes a dropped gateway session, replaying what it
missed, and can send op 7. `crates/yapfast-discord/tests/fake_server.rs` runs
against it, each test on a fake of its own; `scripts/test-fake-discord
FILTER` runs one area, such as `forum`. With `--env PATH`, the fake serves an environment's content in
place of its fixture, for demos and screenshots; see
[building](../building.md#networks) and `testing/fake_discord_env.ts`.
`crates/yapfast-discord/tests/fake_env.rs` runs against
`testing/envs/example.json`. It counts the audio a call sends, which the
ignored `discord_audio` test `a_call_sends_what_the_microphone_hears` checks:
Yapfast joins the Lounge, unmutes and beeps into a microphone that hears its
speakers, as a private PulseAudio of null sinks does, named by `PULSE_SERVER`
with `YAPFAST_TEST_AUDIO=1` set. What the fake can't show,
such as media and DAVE with Discord's servers, needs real Discord. Use a
separate test account, which the owner provides, in a private test server. Run
these tests by hand, never in CI: repeated automated logins are what gets
accounts flagged. Keep the QR-linked test token in the developer's OS
credential store, like any other Discord login.

## Licences

davey is permissive, so closed-source Yapfast can use it. GPL clients
such as [Abaddon](https://github.com/uowuo/abaddon) are reference
material only.
