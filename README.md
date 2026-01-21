# Bluetooth Audio Mode Manager

A Windows system tray application that monitors Bluetooth headphone audio modes (stereo/A2DP vs hands-free/HFP).

## Problem

Bluetooth headphones on Windows automatically switch from high-quality stereo mode (A2DP) to lower-quality hands-free mode (HFP) when any application activates the headset's microphone or initiates a call. This results in noticeably degraded audio quality.

## Solution

This application monitors your Bluetooth audio devices and shows you:

- The current audio mode (Stereo or Hands-Free)
- Which applications are causing HFP mode (when in hands-free mode)
- Real-time mode change notifications

## Features

- **System Tray Integration**: Runs silently in the system tray with mode-indicating icons
- **Real-time Monitoring**: Continuously monitors audio mode via Windows Audio API
- **HFP App Detection**: Shows which apps are outputting to your Bluetooth headset when in HFP mode
- **Notifications**: Toast notifications for mode changes (appears in Windows notification center)
- **Auto-Start**: Optional Windows startup integration
- **Auto-Update**: Checks for updates from GitHub releases

## Download

Download `win_bt_stereo_vs_handsfree.exe` and the `resources` folder from [Releases](https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree/releases).

### File Structure

```
win_bt_stereo_vs_handsfree.exe
resources/
  ├── app.ico
  ├── tray_stereo.ico
  ├── tray_handsfree.ico
  └── tray_unknown.ico
```

## Usage

1. Launch `win_bt_stereo_vs_handsfree.exe` - it will appear in the system tray
2. Right-click the tray icon to see the context menu
3. The icon indicates the current mode:
   - **Stereo icon**: High-quality stereo mode (A2DP)
   - **Hands-free icon**: Lower-quality hands-free mode (HFP)
   - **Unknown icon**: No Bluetooth audio device detected

### Context Menu

- **Mode: [current mode]** - Shows current audio mode
- **Bluetooth Devices** - Lists detected Bluetooth audio devices
- **Apps Using HFP** - Shows apps outputting to BT headset (only visible in HFP mode)
- **Settings** - Open settings window
- **Check for Updates** - Check for new versions
- **About** - Show version and credits
- **Exit** - Close the application

## How It Works

The application uses Windows Audio APIs to detect the current Bluetooth audio profile:

1. **Peak Meter Detection**: Queries `IAudioMeterInformation` to check channel count
   - 1 channel (mono) = HFP mode
   - 2 channels (stereo) = A2DP mode

2. **Session Enumeration**: Uses WASAPI to enumerate audio sessions and identify which applications are using the Bluetooth audio device

## Building from Source

### Prerequisites

- Rust 1.70 or later
- Windows 10/11 SDK
- Visual Studio Build Tools (for MSVC)

### Build

```bash
# Clone the repository
git clone https://github.com/Z-M-Huang/win-bt-stereo-vs-handsfree.git
cd win-bt-stereo-vs-handsfree

# Build in release mode
cargo build --release

# Run tests
cargo test
```

The executable will be in `target/release/win_bt_stereo_vs_handsfree.exe`.

## Configuration

Configuration is stored in `%LOCALAPPDATA%\BtAudioModeManager\config.toml`.

### Settings

| Setting | Description | Default |
|---------|-------------|---------|
| auto_start | Start with Windows | false |
| notify_mode_change | Notify on mode changes | true |
| notify_mic_usage | Notify when apps use mic | true |
| notify_errors | Show error notifications | true |
| auto_check | Auto-check for updates | true |
| log_level | Logging verbosity | info |

## Known Limitations

1. **Detection Only**: This application monitors and displays the current mode but cannot directly switch Bluetooth profiles. Windows does not expose a public API for controlling A2DP/HFP profile selection.

2. **Windows 11 Unified Endpoints**: On Windows 11, Bluetooth audio endpoints are unified, making traditional sample-rate detection unreliable. This app uses peak meter channel count for accurate detection.

## Security

- No admin privileges required for normal operation
- Update checks verify SHA256 checksums
- All operations are read-only monitoring

## License

MIT License with Attribution Requirement

Copyright (c) 2026 Mark.Huang (Z-M-Huang)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

1. The above copyright notice and this permission notice shall be included in all
   copies or substantial portions of the Software.

2. **Attribution Requirement**: Any distribution of the Software or derivative works
   must include visible attribution to the original author (Mark.Huang / Z-M-Huang) in
   the application's About dialog, README, or equivalent documentation.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test`
5. Submit a pull request

## Credits

Created by Mark.Huang (Z-M-Huang)
