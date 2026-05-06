//! MIDI input listener: connects to all available MIDI input ports and
//! forwards configured note-on events into the shared `HotkeyAction`
//! channel so MIDI keyboards / pads can act as alternative bindings for
//! Play/Pause and Stop.
//!
//! Uses `midir` (cross-platform); on Windows it speaks WinMM. Connections
//! are kept alive for the lifetime of `MidiManager`; dropping it closes
//! the ports.

use crate::gui::hotkeys::HotkeyAction;
use midir::{MidiInput, MidiInputConnection};
use soundboard::types::config::MidiBindings;
use std::sync::mpsc::Sender;

/// Holds the live MIDI port connections so they don't get dropped.
pub struct MidiManager {
    _connections: Vec<MidiInputConnection<()>>,
}

impl MidiManager {
    /// Initialise MIDI input. Connects to every available input port and
    /// dispatches matching note-on events through `sender`. Returns `None`
    /// if MIDI is not available on the platform or there are no ports.
    pub fn new(sender: Sender<HotkeyAction>, bindings: &MidiBindings) -> Option<Self> {
        if !bindings.enabled {
            return None;
        }
        let midi_in = match MidiInput::new("soundboard-midi-in") {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("MIDI: failed to initialise MidiInput: {}", e);
                return None;
            }
        };

        let ports = midi_in.ports();
        if ports.is_empty() {
            tracing::info!("MIDI: no input ports detected");
            return None;
        }

        let mut connections = Vec::new();
        for port in ports {
            // Each port needs its own MidiInput instance because `connect`
            // consumes self.
            let mi = match MidiInput::new("soundboard-midi-in") {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("MIDI: skipping port (init failed): {}", e);
                    continue;
                }
            };
            let port_name = mi
                .port_name(&port)
                .unwrap_or_else(|_| "<unknown>".to_string());
            let bindings_clone = bindings.clone();
            let sender_clone = sender.clone();
            match mi.connect(
                &port,
                "soundboard-midi-in",
                move |_stamp, msg, _ctx| {
                    handle_midi_message(msg, &bindings_clone, &sender_clone);
                },
                (),
            ) {
                Ok(conn) => {
                    tracing::info!("MIDI: connected to port \"{}\"", port_name);
                    connections.push(conn);
                }
                Err(e) => {
                    tracing::warn!("MIDI: connect to \"{}\" failed: {}", port_name, e);
                }
            }
        }

        if connections.is_empty() {
            return None;
        }

        Some(MidiManager {
            _connections: connections,
        })
    }
}

/// Pure dispatcher: parse a MIDI message slice and, if it is a note-on
/// matching one of the configured bindings, send the corresponding action.
fn handle_midi_message(msg: &[u8], bindings: &MidiBindings, sender: &Sender<HotkeyAction>) {
    if let Some((note, velocity)) = parse_note_on(msg) {
        // velocity 0 is implicit note-off in MIDI
        if velocity == 0 {
            return;
        }
        if Some(note) == bindings.play_pause_note {
            sender.send(HotkeyAction::PlayPause).ok();
        } else if Some(note) == bindings.stop_note {
            sender.send(HotkeyAction::Stop).ok();
        }
    }
}

/// Returns `Some((note, velocity))` if `msg` is a 3-byte note-on (status
/// nibble `9`); otherwise `None`. Channel is ignored — any channel matches.
fn parse_note_on(msg: &[u8]) -> Option<(u8, u8)> {
    if msg.len() != 3 {
        return None;
    }
    let status_nibble = msg[0] >> 4;
    if status_nibble != 0x9 {
        return None;
    }
    Some((msg[1], msg[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_note_on_recognises_status_9x() {
        assert_eq!(parse_note_on(&[0x90, 60, 127]), Some((60, 127)));
        assert_eq!(parse_note_on(&[0x9F, 72, 64]), Some((72, 64))); // channel 16
    }

    #[test]
    fn parse_note_on_rejects_non_note_on_status() {
        // note-off
        assert_eq!(parse_note_on(&[0x80, 60, 127]), None);
        // CC
        assert_eq!(parse_note_on(&[0xB0, 7, 100]), None);
        // sysex start
        assert_eq!(parse_note_on(&[0xF0, 0, 0]), None);
    }

    #[test]
    fn parse_note_on_rejects_wrong_length() {
        assert_eq!(parse_note_on(&[]), None);
        assert_eq!(parse_note_on(&[0x90]), None);
        assert_eq!(parse_note_on(&[0x90, 60]), None);
        assert_eq!(parse_note_on(&[0x90, 60, 127, 0]), None);
    }

    #[test]
    fn handle_midi_dispatches_play_pause() {
        let (tx, rx) = std::sync::mpsc::channel();
        let bindings = MidiBindings {
            enabled: true,
            play_pause_note: Some(60),
            stop_note: Some(62),
        };
        handle_midi_message(&[0x90, 60, 100], &bindings, &tx);
        assert!(matches!(rx.try_recv(), Ok(HotkeyAction::PlayPause)));
    }

    #[test]
    fn handle_midi_dispatches_stop() {
        let (tx, rx) = std::sync::mpsc::channel();
        let bindings = MidiBindings {
            enabled: true,
            play_pause_note: Some(60),
            stop_note: Some(62),
        };
        handle_midi_message(&[0x90, 62, 100], &bindings, &tx);
        assert!(matches!(rx.try_recv(), Ok(HotkeyAction::Stop)));
    }

    #[test]
    fn handle_midi_zero_velocity_is_ignored() {
        let (tx, rx) = std::sync::mpsc::channel();
        let bindings = MidiBindings {
            enabled: true,
            play_pause_note: Some(60),
            stop_note: Some(62),
        };
        handle_midi_message(&[0x90, 60, 0], &bindings, &tx);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn handle_midi_unmapped_note_is_ignored() {
        let (tx, rx) = std::sync::mpsc::channel();
        let bindings = MidiBindings {
            enabled: true,
            play_pause_note: Some(60),
            stop_note: Some(62),
        };
        handle_midi_message(&[0x90, 80, 100], &bindings, &tx);
        assert!(rx.try_recv().is_err());
    }
}
