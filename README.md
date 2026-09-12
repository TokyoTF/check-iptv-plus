<p align="center">
  <img src="static/logo-white.webp" alt="Check IPTV Plus logo" width="300" />
</p>

<h1 align="center">Check IPTV Plus</h1>

<p align="center">
  Desktop app to validate <strong>M3U / M3U8</strong> playlists and check which channels are <strong>live</strong>, <strong>dead</strong>.
</p>

Load your list from a local file or a remote URL, verify each stream and export the working channels.

## ✨ Features

- 📂 Load lists from file (`.m3u`, `.m3u8`, `.txt`) or remote URL
- ✅ Real verification of each stream
- 📊 Live / dead / pending / total counters
- 🖱️ Per-row context menu: copy URL, open in browser, recheck, remove
- ⏱️ Configurable timeout in Settings (8s default)
- 📤 Export live channels to `.txt`

## 🚀 Development

Requirements: [Bun](https://bun.sh/), [Rust](https://www.rust-lang.org/) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
bun install
bun run tauri dev
```

Other commands:

```bash
bun run check    # frontend type-check
bun run build    # static web build
```

## 📄 License

[MIT](LICENSE)
