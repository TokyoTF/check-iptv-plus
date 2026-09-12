<p align="center">
  <img src="static/logo-white.webp" alt="Check IPTV Plus logo" width="300" />
</p>

<h1 align="center">Check IPTV Plus</h1>

<p align="center">
  Desktop app to validate <strong>M3U / M3U8</strong> playlists and check which channels are <strong>live</strong>, <strong>dead</strong> or <strong>pending</strong>.
</p>

Load your list from a local file or a remote URL, verify each stream with a real HTTP request, and export the working channels.

## ✨ Features

- 📂 Load lists from file (`.m3u`, `.m3u8`, `.txt`) or remote URL
- ✅ Real verification of each stream (HTTP status code, no false positives)
- 📊 Live / dead / pending / total counters with sticky bar
- 🖱️ Per-row context menu: copy URL, open in browser, recheck, remove
- ⏱️ Configurable timeout in Settings (2–60 s)
- 📤 Export live channels to `.txt`
- 🔄 Automatic updates via GitHub Releases
- ⬆️ Floating scroll-to-top button

## 🛠️ Stack

| Layer    | Technology                          |
|----------|-------------------------------------|
| Frontend | SvelteKit 2 + Svelte 5 + Tailwind 4 |
| Backend  | Tauri 2 + Rust (`reqwest`)          |
| Packaging| `tauri-action` on GitHub Actions    |

Checks run in Rust with `Range` requests (only the first bytes are downloaded), avoiding CORS issues and detecting real 404s.

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

## 📦 Releases

Every `v*` tag triggers the `.github/workflows/release.yml` workflow, which builds for Windows, Linux and macOS and publishes the signed artifacts to GitHub Releases. The app detects and installs updates from **About → Check for updates**.

To make it work, set in `src-tauri/tauri.conf.json`:

- `plugins.updater.endpoints` → your repo URL (`.../releases/latest/download/latest.json`)
- `plugins.updater.pubkey` → your public key

And generate the keypair with:

```bash
bunx tauri signer generate -w ~/.tauri/check-iptv.key
```

Store the private key in the repo secrets as `TAURI_SIGNING_PRIVATE_KEY` (plus `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if you set a password). Never commit the private key — `*.key` files are gitignored.

## 📁 Structure

```
src/routes/+page.svelte         # main UI
src-tauri/src/lib.rs            # Rust commands (check_url)
src-tauri/tauri.conf.json       # Tauri config + updater
.github/workflows/release.yml  # release CI
```

## 📄 License

[MIT](LICENSE)
