use ksni::{Icon, MenuItem, Tray};
use ksni::blocking::TrayMethods;
use std::sync::mpsc;
use std::thread;

const ICON_DATA: &[u8] = include_bytes!("../../assets/icon.png");

pub enum TrayMessage {
    PlayPause,
    Stop,
    Quit,
}

struct PwspTray {
    sender: mpsc::Sender<TrayMessage>,
}

impl Tray for PwspTray {
    fn id(&self) -> String {
        "pwsp-soundpad".to_string()
    }

    fn title(&self) -> String {
        "Pipewire Soundpad".to_string()
    }

    fn icon_name(&self) -> String {
        "audio-card".to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        if let Ok(img) = image::load_from_memory(ICON_DATA) {
            let rgba = img.to_rgba8();
            let (width, height) = (rgba.width() as i32, rgba.height() as i32);
            let mut data = rgba.into_raw();
            // KSNI expects ARGB format in network byte order
            for chunk in data.chunks_exact_mut(4) {
                let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
                chunk[0] = a;
                chunk[1] = r;
                chunk[2] = g;
                chunk[3] = b;
            }
            vec![Icon {
                width,
                height,
                data,
            }]
        } else {
            vec![]
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            MenuItem::Standard(ksni::menu::StandardItem {
                label: "Play/Pause".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    tray.sender.send(TrayMessage::PlayPause).ok();
                }),
                ..Default::default()
            }),
            MenuItem::Standard(ksni::menu::StandardItem {
                label: "Stop".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    tray.sender.send(TrayMessage::Stop).ok();
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(ksni::menu::StandardItem {
                label: "Quit".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    tray.sender.send(TrayMessage::Quit).ok();
                }),
                ..Default::default()
            }),
        ]
    }
}

pub struct TrayHandle {
    pub receiver: mpsc::Receiver<TrayMessage>,
    _thread: thread::JoinHandle<()>,
    // When dropped, signals the tray thread to exit cleanly
    _stop_sender: mpsc::Sender<()>,
}

pub fn start_tray() -> Option<TrayHandle> {
    let (sender, receiver) = mpsc::channel();

    // Create a channel to signal when the tray should stop
    let (stop_sender, stop_receiver) = mpsc::channel::<()>();

    // Spawn the tray in a separate thread with its own tokio runtime
    // to avoid conflicts with eframe's async context
    let thread_handle = thread::spawn(move || {
        let tray = PwspTray { sender };
        match tray.spawn() {
            Ok(handle) => {
                // Store the handle properly instead of using mem::forget
                // Wait for stop signal or indefinitely if app keeps running
                // The handle must stay alive for the tray to work
                let _tray_handle = handle;
                // Block on stop signal - this keeps the thread alive while holding the handle
                let _ = stop_receiver.recv();
                // When we receive the stop signal (or channel closes), thread exits
                // and _tray_handle is dropped, cleaning up the tray
            }
            Err(e) => {
                eprintln!("Failed to create system tray: {}", e);
            }
        }
    });

    // Store stop_sender so it's dropped when TrayHandle is dropped,
    // which signals the tray thread to exit
    Some(TrayHandle {
        receiver,
        _thread: thread_handle,
        _stop_sender: stop_sender,
    })
}
