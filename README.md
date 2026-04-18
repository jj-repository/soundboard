# **🎵 Soundboard**

**Soundboard** is a simple yet powerful **soundboard application** written in **Rust**. It provides a user-friendly graphical interface for **managing and playing audio files, directing their output directly to a virtual microphone.** This makes it an ideal tool for gamers, streamers, and anyone looking to inject sound effects into voice chats on platforms like **Discord, Zoom, or Teamspeak**.

![screenshot.png](assets/screenshot.png)

# **🌟 Key Features**

* **Multi-Format Support**: Play audio files in popular formats, including _**mp3**_, _**wav**_, _**ogg**_, _**flac**_, _**mp4**_, and _**aac**_.
* **Virtual Microphone Output**: The application routes audio through a virtual device (PipeWire on Linux, VB-Audio Virtual Cable on Windows), so other users hear the sounds as if you were speaking into your microphone.
* **Modern and Clean GUI**: The interface is built with the [egui](https://egui.rs) library, ensuring an intuitive and responsive user experience.
* **Sound Collection Management**: Add and remove directories containing your audio files. The application scans these folders and displays all supported files.
* **Quick Search**: Use the built-in search bar to instantly find any sound file.
* **Detailed Playback Controls**: play/pause, volume slider, position slider.
* **Persistent Configuration**: Directory list and audio output selection are saved automatically.
* **Built-in Update System**: Check for updates with progress tracking and download pre-built binaries directly from GitHub Releases.
* **Auto-Launched Daemon**: The GUI spawns the background daemon on startup — one double-click is all you need.

# **⚙️ How It Works**

Soundboard uses a client-server architecture with three components:

* **soundboard-daemon**: Background service. Creates and manages the virtual audio device, links it within the PipeWire / Windows audio graph, handles all playback. Auto-started by the GUI.
* **soundboard-gui**: Graphical client. Communicates with the daemon over a Unix socket (Linux) or localhost TCP (Windows).
* **soundboard-cli**: Command-line client for scripting and quick control without the GUI.

# **🚀 Installation**

Download the pre-built archive for your OS from the [releases page](https://github.com/jj-repository/soundboard/releases):

* **Linux**: `soundboard-Linux.zip` — contains `soundboard-gui`, `soundboard-daemon`, `soundboard-cli`. Unzip anywhere and run `soundboard-gui`.
* **Windows**: `soundboard.zip` — contains the three `.exe` files. Unzip and run `soundboard-gui.exe`.

Each archive ships with a `.sha256` sidecar for integrity verification.

## **Building from source**

Requirements:
* [Rust](https://www.rust-lang.org/tools/install) (rustup recommended)
* Linux only: [PipeWire](https://pipewire.org/) installed and running, plus dev headers (`libpipewire-0.3-dev libclang-dev libasound2-dev`)
* Windows only: [VB-Audio Virtual Cable](https://vb-audio.com/Cable/) installed for mic routing

```bash
git clone https://github.com/jj-repository/soundboard.git
cd soundboard
cargo build --release
```

Binaries land in `./target/release/`:
* **soundboard-gui**
* **soundboard-cli**
* **soundboard-daemon**

# **🎮 Usage**

### **GUI**

Just run `soundboard-gui` — the daemon is launched automatically if it isn't already running.

1. **Add Sounds**: Click **"Add Directory"** and select a folder with your audio files.
2. **Select Microphone**: Pick your physical microphone. Soundboard creates a virtual mic that mixes your voice with the audio files.
3. **Playback**: Click a file to load it, then play/pause as needed.

### **CLI**

```bash
soundboard-cli --help
soundboard-cli action play <file_path>
soundboard-cli get volume
soundboard-cli set position 20
```

The CLI assumes the daemon is running — start the GUI once (or run `soundboard-daemon &` manually) before using it standalone.

# **🔄 Updates**

Soundboard includes a built-in update checker.

1. Open **Settings** in the GUI
2. Click **"Check for Updates"**
3. If an update is available, click **"Download"** — the new archive is saved to the user runtime dir with SHA-256 verification
4. Unzip and replace your current binaries

Enable **"Check for Updates on Startup"** in Settings to check automatically on each launch.

# **🛠️ Logging**

Set the `SOUNDBOARD_LOG` env var to filter output (e.g. `SOUNDBOARD_LOG=debug`). Defaults to `info`.

# **🤝 Contributing**

Contributions welcome. Open an [issue](https://github.com/jj-repository/soundboard/issues) or submit a [pull request](https://github.com/jj-repository/soundboard/pulls).

Originally forked from [arabianq/pipewire-soundpad](https://github.com/arabianq/pipewire-soundpad).

# **📜 License**

MIT — see [LICENSE](LICENSE).
