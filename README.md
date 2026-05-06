# **🎵 Soundboard**

**Soundboard** is a Windows soundboard application written in Rust. It plays audio files through a virtual microphone so other people on your call hear them as if you were speaking. Built for gamers, streamers, and anyone routing sound effects into Discord, Zoom, TeamSpeak, etc.

![screenshot.png](assets/screenshot.png)

# **🌟 Key Features**

* **Multi-Format Support**: mp3, wav, ogg, flac, m4a, aac, opus.
* **Virtual Microphone Output**: Audio is routed through VB-Audio Virtual Cable, so the other side hears it through your mic.
* **Modern GUI**: [egui](https://egui.rs).
* **Sound Collection Management**: Add directories of audio files; the app scans them and shows everything supported.
* **Quick Search**: Search bar over the loaded files.
* **Playback Controls**: play/pause, volume, position slider.
* **Persistent Configuration**: Directories, audio output, hotkeys, playlists are all saved automatically.
* **CLI Subcommand**: `soundboard.exe cli ...` for scripting.
* **Built-in Update System**: Check for updates from Settings.

# **⚙️ How It Works**

Single executable. Three modes selected by argv:

* `soundboard.exe` — runs the GUI. The audio engine + IPC listener boot in-process as background tasks; no separate daemon binary.
* `soundboard.exe cli ...` — CLI client. Connects to the running GUI's IPC listener over `localhost:19735` to issue commands.
* `soundboard.exe --headless` — runs only the audio engine + IPC listener, no GUI. Useful for autostart-without-window setups.

Internally the GUI talks to the embedded engine through the same TCP loop the CLI uses, which keeps a clean boundary between UI state and audio state.

# **🚀 Installation**

1. Install [VB-Audio Virtual Cable](https://vb-audio.com/Cable/) (free).
2. Download `soundboard.zip` from the [releases page](https://github.com/jj-repository/soundboard/releases).
3. Unzip and run `soundboard.exe`.

Each archive ships with a `.sha256` sidecar for integrity verification.

## **Building from source**

Requirements:
* [Rust](https://www.rust-lang.org/tools/install) (rustup recommended), latest stable
* [VB-Audio Virtual Cable](https://vb-audio.com/Cable/) installed for mic routing

```bash
git clone https://github.com/jj-repository/soundboard.git
cd soundboard
cargo build --release
```

The binary lands at `./target/release/soundboard.exe`.

# **🎮 Usage**

### **GUI**

Just run `soundboard.exe`. The audio engine starts inside the same process — no second window, no second binary.

1. **Set Sounds Folder**: First-run prompt picks where to store your sounds.
2. **Select Microphone**: Pick your physical mic. Soundboard mixes it with your sound effects and routes both into VB-Cable.
3. **Set Output**: Pick "CABLE Input (VB-Audio Virtual Cable)" as the output. In Discord/Zoom/etc., set your mic to "CABLE Output".
4. **Playback**: Click a file to load, then play/pause.

### **CLI**

```
soundboard cli --help
soundboard cli action play <file_path>
soundboard cli get volume
soundboard cli set position 20
```

The CLI assumes a running GUI (or `soundboard.exe --headless`). If nothing is running, start one first.

# **🔄 Updates**

1. Open **Settings** in the GUI.
2. Click **"Check for Updates"**.
3. If an update is available, click **"Download"** — saved with SHA-256 verification.
4. Unzip and replace `soundboard.exe`.

# **🛠️ Logging**

Set the `SOUNDBOARD_LOG` env var to filter output (e.g. `SOUNDBOARD_LOG=debug`). Defaults to `info`.

# **🤝 Contributing**

Open an [issue](https://github.com/jj-repository/soundboard/issues) or submit a [pull request](https://github.com/jj-repository/soundboard/pulls).

Originally forked from [arabianq/pipewire-soundpad](https://github.com/arabianq/pipewire-soundpad). The Linux/PipeWire path was dropped in this fork; for Linux see the v1.10 tag of the upstream project.

# **📜 License**

MIT — see [LICENSE](LICENSE).
