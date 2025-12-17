<div align="center">

# Rweb Music Player

[![English](https://img.shields.io/badge/Language-English-blue)](README_EN.md) [![Chinese](https://img.shields.io/badge/Language-中文-red)](README.md)

</div>

---

# Rweb Music Player

Rweb Music Player is a lightweight Rust-based music player designed for developers, offering a powerful HTTP API for remote control via external scripts (e.g., Python).

## Features

- **Playback Control**: Play music by file path or playlist index.
- **Playlist Management**: Retrieve, rename, delete playlists, and remove songs.
- **API Interface**: Comprehensive HTTP API for programmatic control.
- **Multi-language**: Supports Chinese and English UI.
- **Cross-platform**: Built with Rust (currently optimized for Windows).

## Quick Start

1. Download the latest release.
2. Run `Rweb_music_player.exe`.
3. Use Python scripts or other HTTP clients to control the player via API.

## API Documentation

For detailed API documentation, please refer to [API Documentation](README_API.md).

## License & Copyright

This project is licensed under the Apache License 2.0.

### Font Usage Notice

This project uses the **Microsoft YaHei** font by default on Windows platforms.
- The font copyright belongs to Microsoft Corporation.
- This project only invokes the system-installed font and does not distribute any font files.
- If you are running on a non-Windows platform or wish to use a different font, please modify the font configuration in the source code.

### Third-party Libraries

This project uses the following open-source libraries:
- [egui](https://github.com/emilk/egui) (MIT/Apache-2.0)
- [axum](https://github.com/tokio-rs/axum) (MIT)
- [rodio](https://github.com/RustAudio/rodio) (MIT/Apache-2.0)
- [lofty](https://github.com/Serial-ATA/lofty-rs) (MIT/Apache-2.0)
- [tokio](https://github.com/tokio-rs/tokio) (MIT)

See `Cargo.toml` for the full list of dependencies.

## Contributing

Issues and Pull Requests are welcome!
