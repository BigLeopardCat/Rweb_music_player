<div align="center">

# Rweb Music Player

[![English](https://img.shields.io/badge/Language-English-blue)](README_EN.md) [![Chinese](https://img.shields.io/badge/Language-中文-red)](README.md)

</div>

---

# Rweb Music Player

Rweb Music Player 是一个基于 Rust 的轻量级音乐播放器，专为开发者设计，提供了强大的 HTTP API 接口，支持通过外部脚本（如 Python）进行远程控制。

## 功能特性

- **播放控制**：支持通过文件路径或歌单序号播放音乐。
- **歌单管理**：支持获取、重命名、删除歌单，以及从歌单中删除歌曲。
- **API 接口**：提供详细的 HTTP API，便于程序化控制。
- **多语言支持**：支持中文和英文界面切换。
- **跨平台**：基于 Rust 开发，理论上支持多平台（目前主要针对 Windows 优化）。

## 快速开始

1. 下载最新 Release 版本。
2. 运行 `Rweb_music_player.exe`。
3. 使用 Python 脚本或其他 HTTP 客户端调用 API 控制播放器。

## API 文档

详细的 API 文档请参考 [API 文档](README_API.md)。

## 许可证与版权说明

本项目采用 Apache License 2.0 许可证开源。

### 字体使用说明

本项目在 Windows 平台上运行时，默认使用系统自带的 **微软雅黑 (Microsoft YaHei)** 字体。
- 该字体版权归 Microsoft Corporation 所有。
- 本项目仅调用系统已安装的字体文件，不分发任何字体文件。
- 如果您在非 Windows 平台运行，或希望使用其他字体，请修改源码中的字体配置。

### 第三方库

本项目使用了以下开源库：
- [egui](https://github.com/emilk/egui) (MIT/Apache-2.0)
- [axum](https://github.com/tokio-rs/axum) (MIT)
- [rodio](https://github.com/RustAudio/rodio) (MIT/Apache-2.0)
- [lofty](https://github.com/Serial-ATA/lofty-rs) (MIT/Apache-2.0)
- [tokio](https://github.com/tokio-rs/tokio) (MIT)

完整依赖列表请查看 `Cargo.toml`。

## 贡献

欢迎提交 Issue 和 Pull Request！
