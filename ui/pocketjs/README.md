# P2-J01: actual PocketJS Linux runtime spike

Original PocketJS TypeScript application. No browser, WebView or stub renderer.

Upstream dependency: https://github.com/pocket-nexus/pocketjs version v0.17.0,
pinned commit 83c6cac996237968785125118b543cd4e339254c.
PocketJS core is MIT licensed; third-party modules retain separate licenses.

The original UI uses supported PocketJS Solid mount, View/Text components,
button input, and local reactive state. It is compiled against the REAL
upstream product-linux-kms host contract at 800x600 with baked glyphs.

The acceptance script copies only our original app into a disposable upstream
checkout, runs pocket check and pocket compile, builds the upstream native
Linux runtime, admits the compiled .pkb and boots QuickJS/Rust for four
headless ticks. No PocketJS source is vendored into pokshell.

WARNING: the upstream pocket-linux-kms program owns DRM/fbdev in live mode.
Never run it against a live card while Sway or native pokshell owns the display.
This P2-J01 milestone deliberately runs only --headless and --check.
A future Sway/Smithay host adapter and bounded low-privilege IPC companion are
required before this UI can replace the existing system menu or launch games.
Library, Settings and Quick Menu are currently local UI screens only.

Local reproduction from a graphical terminal: clone the pinned upstream
PocketJS commit into a disposable directory, install its dependencies,
then call tools/verify-pocketjs.sh /path/to/pocketjs-checkout.
