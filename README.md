# Bluetooth Audio Mode Manager

[![GitHub release](https://img.shields.io/github/v/release/Z-M-Huang/win-bt-stereo-vs-handsfree?style=flat-square)](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-blue?style=flat-square)](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/releases)
[![Languages](https://img.shields.io/badge/languages-7-green?style=flat-square)](#supported-languages)

A Windows system tray application that monitors and controls Bluetooth headphone audio modes (stereo/A2DP vs hands-free/HFP).

## Documentation / Wiki

**For detailed user guides and tutorials, visit our [Wiki](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki):**

| Language | Guide |
|----------|-------|
| English | [User Guide](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki/User-Guide) |
| 简体中文 | [用户指南](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki/User-Guide-zh-CN) |
| 繁體中文 | [使用指南](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki/User-Guide-zh-TW) |
| Español | [Guía del Usuario](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki/User-Guide-es) |
| Deutsch | [Benutzerhandbuch](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki/User-Guide-de) |
| Français | [Guide de l'Utilisateur](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki/User-Guide-fr) |
| 日本語 | [ユーザーガイド](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/wiki/User-Guide-ja) |

## The Problem

Bluetooth headphones on Windows automatically switch from **high-quality stereo mode (A2DP)** to **lower-quality hands-free mode (HFP)** when any application activates the headset's microphone. This results in noticeably degraded audio quality - music sounds muffled and low-quality.

## The Solution

This application helps you:

- **Monitor** your Bluetooth audio mode in real-time
- **See** which apps are causing the mode switch
- **Force stereo mode** by disabling the hands-free service
- **Get notified** when the audio mode changes

## Quick Start

### Download

1. Go to [**Releases**](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/releases)
2. Download the latest `.zip` file (e.g., `win-bt-stereo-vs-handsfree-v0.3.0-windows-x64.zip`)
3. Extract to any folder

### Run

1. Double-click `win_bt_stereo_vs_handsfree.exe`
2. The app icon appears in your **system tray** (bottom-right corner)
3. Right-click the icon to see the menu

### Keep the Icon Visible (Recommended)

By default, Windows hides tray icons. To always show this app's icon:

**Windows 11:**
1. Right-click the taskbar → **Taskbar settings**
2. Expand **Other system tray icons**
3. Turn ON **Bluetooth Audio Mode Manager**

**Windows 10:**
1. Right-click the taskbar → **Taskbar settings**
2. Click **Select which icons appear on the taskbar**
3. Turn ON **Bluetooth Audio Mode Manager**

## Features

- **System Tray Integration** - Runs silently with mode-indicating icons
- **Real-time Monitoring** - Continuously monitors audio mode
- **Force Stereo Mode** - Disable HFP to keep high-quality audio
- **HFP App Detection** - See which apps trigger hands-free mode
- **Toast Notifications** - Get notified of mode changes
- **Multi-Language UI** - Available in 7 languages
- **Auto-Start** - Optional startup with Windows
- **Auto-Update** - Checks for new versions

## Supported Languages

| Language | Code |
|----------|------|
| English | en |
| Simplified Chinese (简体中文) | zh-CN |
| Traditional Chinese (繁體中文) | zh-TW |
| Spanish (Español) | es |
| German (Deutsch) | de |
| French (Français) | fr |
| Japanese (日本語) | ja |

The app automatically detects your Windows language. You can also manually select a language in Settings.

## System Requirements

- Windows 10 or Windows 11
- Bluetooth audio device (headphones, earbuds, speakers)
- No administrator privileges required

## Building from Source

<details>
<summary>Click to expand build instructions</summary>

### Prerequisites

- Rust 1.70 or later
- Windows 10/11 SDK
- Visual Studio Build Tools (for MSVC)

### Build

```bash
git clone https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree.git
cd win-bt-stereo-vs-handsfree
cargo build --release
cargo test
```

The executable will be in `target/release/win_bt_stereo_vs_handsfree.exe`.

</details>

## Configuration

Configuration is stored in `%LOCALAPPDATA%\BtAudioModeManager\config.toml`.

| Setting | Description | Default |
|---------|-------------|---------|
| language | UI language (null = system default) | null |
| auto_start | Start with Windows | false |
| notify_mode_change | Notify on mode changes | true |
| notify_mic_usage | Notify when apps use mic | true |
| notify_errors | Show error notifications | true |
| auto_check | Auto-check for updates | true |

## Security

- No admin privileges required
- Update checks verify SHA256 checksums
- All operations are read-only monitoring (except Force Stereo which toggles HFP service)

## License

MIT License with Attribution Requirement - Copyright (c) 2026 Mark.Huang (Z-M-Huang)

See [LICENSE](LICENSE) for full details.

## Contributing

Contributions are welcome! Please fork the repository, create a feature branch, and submit a pull request.

## Credits

Created by Mark.Huang (Z-M-Huang)
