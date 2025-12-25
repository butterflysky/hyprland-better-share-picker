# hyprland-better-share-picker
![Rust](https://img.shields.io/badge/Rust-1.76%2B%20(Edition%202024)-orange)

A lightweight, memory-safe, pure‑Rust window picker for Hyprland. It is a drop‑in replacement for the default `hyprland-share-picker` used by `xdg-desktop-portal-hyprland`.

## Motivation
The stock picker is a C++/Qt application. This project exists to provide:
- A **pure‑Rust** toolchain with minimal runtime dependencies.
- **Memory safety** by default.
- **Lower overhead** and a smaller footprint than a full Qt stack.
- A hackable codebase that fits naturally into the Hyprland + Wayland ecosystem.

## Architecture & Design
This tool is built around two core systems that must cooperate without blocking each other:

1) **Wayland discovery and capture (synchronous)**
- We connect to the compositor via `wayland-client` and `smithay-client-toolkit` (SCTK).
- The `zwlr_foreign_toplevel_manager_v1` protocol provides **toplevel discovery** (window list, titles, app IDs).
- The custom `hyprland-toplevel-export-v1` protocol provides **pixel buffers** for thumbnails.
- The Wayland event queue uses a **blocking dispatch loop**. This is the most reliable way to integrate with the compositor and avoids re-implementing a custom Wayland poller.

2) **UI and state management (asynchronous)**
- The GUI uses **Iced**, selected for its pure Rust stack and Elm‑style state model.
- Iced’s `Subscription::run` bridges the synchronous Wayland dispatch into the async UI world.
- A background thread runs the Wayland loop and pushes `WaylandEvent`s through a channel into Iced.
- The UI reacts to these events and renders a grid of buttons with titles and thumbnails.

### Event Loop Bridging (Why this design?)
Wayland’s event queue is fundamentally synchronous, while Iced expects an async stream of messages. The current implementation solves this by:
- Running the Wayland queue on a dedicated thread.
- Forwarding compositor events into Iced via an `mpsc` channel.
- Ensuring UI state never touches Wayland objects (Send/Sync safety).

This keeps the UI responsive and avoids complicated polling or unsafe cross‑thread Wayland usage.

## The “Magic” (Implementation Details)
### Protocol Generation
The custom `hyprland-toplevel-export-v1.xml` protocol is compiled at build time. The build script writes a small Rust module into `OUT_DIR` and uses `wayland-scanner` to generate bindings.

### Thumbnails
- `zwlr_foreign_toplevel_manager_v1` is used for **enumeration** only.
- `hyprland-toplevel-export-v1` is used to **capture** a single frame for each toplevel.
- We currently accept **`wl_shm` buffers** (`ARGB8888` / `XRGB8888`). DMA‑BUF support can be added later if your compositor only exposes GPU buffers.

### Lazy Loading
To avoid flooding the compositor, each toplevel is captured **once** on discovery. This provides a responsive UI without the load of continuous screencopy or live previews. This is intentionally conservative and can be extended with a “refresh” action if needed.

## Integration with xdg-desktop-portal-hyprland
The portal consumes the picker’s result by reading **STDOUT** and the exit code.

When a user clicks a window:
- The picker prints a handle like `wayland:0x...` to STDOUT.
- The process exits with code `0`.

When the user cancels:
- The process exits with code `1`.

This mirrors the behavior expected by `xdg-desktop-portal-hyprland`.

## Installation
### Build
```bash
cargo build --release
```

### Install
Place the binary somewhere on your `PATH` so the portal can discover it. For example:
```bash
install -Dm755 target/release/hyprland-better-share-picker ~/.local/bin/hyprland-better-share-picker
```

You can then configure `xdg-desktop-portal-hyprland` to use this binary instead of the default picker. The portal reads a Hyprland config file and expects the `screencopy:custom_picker_binary` key.

### Portal configuration (xdg-desktop-portal-hyprland)
Add the following to the **xdg-desktop-portal-hyprland** config at `~/.config/hypr/xdph.conf` (default path):
```
screencopy {
    allow_token_by_default = true
    custom_picker_binary = /home/butterfly/.local/bin/hyprland-better-share-picker
}
```

This matches the portal’s invocation path (see the `ScreencopyShared.cpp` picker launch logic you referenced).

## Configuration
No configuration or environment variables are required for the prototype.

## Troubleshooting / Gotchas
- **Portal does not call the picker**: Ensure `xdg-desktop-portal-hyprland` is running and you set `screencopy:custom_picker_binary` in `~/.config/hypr/xdph.conf` exactly (key name and spacing matter).
- **No thumbnails / blank previews**: This prototype only consumes `wl_shm` buffers. If Hyprland exports DMA‑BUF only, you’ll need to add a DMA‑BUF import path.
- **Protocol file missing**: The build requires `hyprland-toplevel-export-v1.xml` in the project root. If it’s absent, protocol bindings won’t generate.
- **Wrong binary path**: The portal uses the literal string path from `xdph.conf`. Absolute paths are safest.
- **No output on selection**: The picker prints a `wayland:0x...` handle to STDOUT and exits immediately. If you wrap the binary, ensure stdout is not redirected or swallowed.

## Project Layout
- `build.rs` — Generates protocol bindings for `hyprland-toplevel-export-v1.xml`.
- `src/main.rs` — Iced UI, selection handling, cancellation behavior.
- `src/wayland.rs` — Wayland connection, toplevel discovery, thumbnail capture.

## Rationale for Key Technical Choices (ADR)
### Rust 2024
Rust 2024 is chosen to keep the project future‑proof and aligned with the latest language ergonomics. The current code uses an event-driven structure that benefits from modern Rust features and clean async interop.

### Iced over GTK/Qt
GTK/Qt bindings would add large native dependency chains. Iced keeps the stack **pure Rust** and provides a consistent, testable Elm-style state model. For a picker, a stable state machine is more important than complex UI widgets.

### SCTK + wayland-client
SCTK provides robust helpers (e.g., SHM pools) while staying close to the metal. `wayland-client` offers precise control over protocol objects, which is essential for custom Hyprland protocols.

## Contributing
PRs are welcome. The codebase is intentionally small and focused. If you plan to add DMA‑BUF support or live preview refresh, please keep the event loop model intact and avoid pushing Wayland objects across threads.

## License
Licensed under either of:
- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)
