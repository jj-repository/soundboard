# Contributing to Soundboard

Thank you for your interest in contributing to Soundboard! This document provides guidelines and instructions for contributing.

## Code of Conduct

Be respectful, constructive, and collaborative. We want to maintain a welcoming environment for all contributors.

## How to Contribute

### Reporting Bugs

If you find a bug, please create an issue with:

- A clear, descriptive title
- Your Linux distribution and version
- Audio system (PipeWire/PulseAudio version)
- Steps to reproduce the bug
- Expected vs actual behavior
- Any error messages or logs
- Screenshots if applicable

### Suggesting Features

Feature suggestions are welcome! Please create an issue with:

- A clear description of the feature
- Use cases and benefits
- Any relevant examples or mockups

### Pull Requests

1. **Fork the repository** and create a new branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes**:
   - Follow the existing code style
   - Add comments for complex logic
   - Update documentation if needed

3. **Test your changes**:
   - Ensure the app builds: `npm run tauri build -- --debug`
   - Test functionality manually
   - Verify audio routing works correctly

4. **Commit your changes**:
   ```bash
   git add .
   git commit -m "Add feature: brief description"
   ```

5. **Push and create a Pull Request**:
   ```bash
   git push origin feature/your-feature-name
   ```

### Development Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/soundboard.git
   cd soundboard
   ```

2. Install dependencies:
   ```bash
   npm install
   ```

3. Run in development mode:
   ```bash
   npm run tauri dev
   ```

## Project Structure

```
soundboard/
├── src/                    # React frontend
│   ├── App.tsx            # Main UI component
│   ├── App.css            # Styling
│   └── main.tsx           # React entry point
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── audio.rs       # Audio playback
│   │   ├── pipewire.rs    # Virtual mic management
│   │   ├── sound_manager.rs # Sound state
│   │   ├── hotkeys.rs     # Global hotkeys
│   │   └── lib.rs         # Tauri commands
│   └── Cargo.toml         # Rust dependencies
└── package.json           # Node dependencies
```

## Coding Style

### Rust

- Follow standard Rust formatting (use `rustfmt`)
- Use meaningful variable names
- Add error handling with proper context
- Document public functions

### TypeScript/React

- Use TypeScript for type safety
- Follow React hooks best practices
- Use functional components
- Keep components focused and modular

## Testing

Currently, testing is manual. When adding features:

1. Test basic functionality (load sounds, play, stop)
2. Test virtual microphone setup/cleanup
3. Test system audio routing
4. Test hotkeys
5. Verify no audio routing conflicts

## Areas for Contribution

We welcome contributions in these areas:

- **Bug fixes**: Any bugs you encounter
- **Documentation**: Improve README, add guides, fix typos
- **Features**:
  - Hotkey improvements
  - UI enhancements
  - Additional audio format support
  - Cross-platform support (macOS, Windows)
  - Sound organization (folders, favorites)
  - Sound preview before playing
  - Volume normalization
- **Testing**: Add automated tests
- **Performance**: Optimize audio routing, reduce CPU usage
- **Accessibility**: Keyboard navigation, screen reader support

## Questions?

If you have questions about contributing:

- Open a discussion issue
- Check existing issues and PRs
- Review the README for project details

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
