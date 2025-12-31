# Soundboard

[![Build](https://github.com/jj-repository/soundboard/actions/workflows/build.yml/badge.svg)](https://github.com/jj-repository/soundboard/actions/workflows/build.yml)
[![Release](https://github.com/jj-repository/soundboard/actions/workflows/release.yml/badge.svg)](https://github.com/jj-repository/soundboard/actions/workflows/release.yml)
[![Downloads](https://img.shields.io/github/downloads/jj-repository/soundboard/total)](https://github.com/jj-repository/soundboard/releases)

A powerful Linux soundboard application with virtual microphone support for Discord, games, and VoIP applications. Built with Tauri, Rust, and React.

## Features

- **Audio Playback**: Load and play audio files from folders with customizable volume
- **Virtual Microphone**: Route soundboard audio through a virtual microphone for Discord/VoIP
- **System Audio Routing**: Route system audio (YouTube, Spotify, browser) through your microphone
- **Global Hotkeys**: Assign keyboard shortcuts to play sounds instantly
- **Master Volume Control**: Adjust overall volume with support for values up to 200%
- **Clean UI**: Modern, responsive interface built with React and TypeScript
- **PipeWire Integration**: Advanced audio routing using PipeWire/PulseAudio

## Requirements

- **OS**: Linux (tested on systems with PipeWire/PulseAudio)
- **Dependencies**:
  - PipeWire or PulseAudio
  - `pactl` command-line tool
  - Node.js 18+ (for building)
  - Rust 1.70+ (for building)

## Installation

### From Source

1. Clone the repository:
```bash
git clone https://github.com/jj-repository/soundboard.git
cd soundboard
```

2. Install dependencies:
```bash
npm install
```

3. Build the application:
```bash
# Development build
npm run tauri build -- --debug

# Production build
npm run tauri build
```

4. Run the application:
```bash
# Development mode
npm run tauri dev

# Or run the built binary
./src-tauri/target/debug/tauri-app
```

### Binary Installation

Pre-built binaries are available in the [Releases](https://github.com/jj-repository/soundboard/releases) section:

- `.deb` package for Debian/Ubuntu-based systems
- `.rpm` package for Fedora/RHEL-based systems
- Standalone binary for other Linux distributions

## Usage

### Basic Usage

1. **Load Sounds**: Click "Load Folder" to import audio files from a directory
2. **Play Sounds**: Click the play button on any sound card
3. **Adjust Volume**: Use the master volume slider in settings, or adjust individual sound volumes
4. **Stop All**: Click "Stop All" to immediately stop all playing sounds

### Virtual Microphone Setup

1. Open Settings
2. Click "Setup Virtual Microphone"
3. In Discord (or other VoIP apps):
   - Go to Settings → Voice & Video
   - Under "Input Device", select "Soundboard Virtual Microphone"
4. Test by playing sounds - you should see input activity in Discord

**Important**: When the virtual mic is enabled:
- Soundboard audio is routed to the virtual microphone
- Your default audio output (headphones) remains unchanged
- You can still hear Discord, games, and system sounds normally
- Your real microphone is mixed with soundboard audio

### System Audio Routing

To route system audio (YouTube, Spotify, browser) through your microphone:

1. Enable the virtual microphone first (see above)
2. In Settings, enable "Route System Audio to Mic"
3. All system audio will now be mixed into your microphone
4. Disable when done to prevent audio feedback

### Hotkeys

1. Click the edit button on any sound
2. Enter a hotkey combination (e.g., `Ctrl+Alt+A`)
3. Press the hotkey globally to play the sound

## Architecture

### Backend (Rust)

- **audio.rs**: Audio playback using rodio and cpal
- **pipewire.rs**: PipeWire/PulseAudio virtual device management
- **sound_manager.rs**: Sound state management and persistence
- **hotkeys.rs**: Global hotkey registration and handling
- **lib.rs**: Tauri command handlers and application setup

### Frontend (React + TypeScript)

- **App.tsx**: Main application UI and state management
- **App.css**: Styling and responsive design
- Modern React 19 with hooks

### Audio Routing Flow

```
┌─────────────────────────────────────────────────────────────┐
│                      Virtual Mic Setup                       │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Real Mic ──────┐                                            │
│                 │                                            │
│                 ├──→ Soundboard_Mix ──→ Virtual Mic ──→ Discord │
│                 │         (Null Sink)      (Monitor)         │
│  Soundboard ────┘                                            │
│  (auto-routed)                                               │
│                                                               │
│  Discord/Games ──→ Your Headphones (unchanged)               │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Troubleshooting

### Virtual Mic Not Appearing in Discord

1. Check if PipeWire/PulseAudio is running:
```bash
pactl info
```

2. List audio sources:
```bash
pactl list sources short
```

3. Look for "SoundboardMic" in the output

### No Audio Output

1. Verify soundboard audio is being routed:
```bash
pactl list sink-inputs
```

2. Check for "tauri-app" entries and verify they're routed to "Soundboard_Mix"

### ALSA/JACK Errors

These warnings are normal and can be ignored:
```
ALSA lib pcm_dmix.c:1000:(snd_pcm_dmix_open) unable to open slave
Cannot connect to server socket err = No such file or directory
```

The application automatically falls back to PipeWire/PulseAudio.

## Development

### Project Structure

```
soundboard/
├── src/                  # React frontend
│   ├── App.tsx
│   ├── App.css
│   └── main.tsx
├── src-tauri/            # Rust backend
│   ├── src/
│   │   ├── audio.rs
│   │   ├── pipewire.rs
│   │   ├── sound_manager.rs
│   │   ├── hotkeys.rs
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
└── README.md
```

### Building

```bash
# Development build (faster, includes debug symbols)
npm run tauri build -- --debug

# Production build (optimized)
npm run tauri build
```

### Running in Development

```bash
npm run tauri dev
```

This enables hot-reloading for the frontend and automatic Rust recompilation.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [Tauri](https://tauri.app/) - Rust-based desktop application framework
- Audio playback powered by [rodio](https://github.com/RustAudio/rodio)
- Icons from [Lucide React](https://lucide.dev/)
- PipeWire integration for advanced Linux audio routing

## Support

If you encounter any issues or have questions:

1. Check the [Troubleshooting](#troubleshooting) section
2. Search existing [Issues](https://github.com/jj-repository/soundboard/issues)
3. Open a new issue with:
   - Your Linux distribution and version
   - Audio system (PipeWire/PulseAudio version)
   - Steps to reproduce the problem
   - Relevant error messages or logs
