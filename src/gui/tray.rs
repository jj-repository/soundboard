use std::sync::mpsc;
use std::thread;

const ICON_DATA: &[u8] = include_bytes!("../../assets/icon.png");

pub enum TrayMessage {
    PlayPause,
    Stop,
    Quit,
}

pub struct TrayHandle {
    pub receiver: mpsc::Receiver<TrayMessage>,
    _thread: thread::JoinHandle<()>,
    _stop_sender: mpsc::Sender<()>,
}

// ============= Linux Implementation (ksni) =============

#[cfg(target_os = "linux")]
pub fn start_tray() -> Option<TrayHandle> {
    use ksni::{Icon, MenuItem, Tray};
    use ksni::blocking::TrayMethods;

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

    let (sender, receiver) = mpsc::channel();
    let (stop_sender, stop_receiver) = mpsc::channel::<()>();

    let thread_handle = thread::spawn(move || {
        let tray = PwspTray { sender };
        match tray.spawn() {
            Ok(handle) => {
                let _tray_handle = handle;
                let _ = stop_receiver.recv();
            }
            Err(e) => {
                tracing::error!("Failed to create system tray: {}", e);
            }
        }
    });

    Some(TrayHandle {
        receiver,
        _thread: thread_handle,
        _stop_sender: stop_sender,
    })
}

// ============= Windows Implementation (tray-icon + muda) =============

#[cfg(target_os = "windows")]
pub fn start_tray() -> Option<TrayHandle> {
    use tray_icon::{TrayIconBuilder, Icon};
    use muda::{Menu, MenuItem, PredefinedMenuItem};

    let (sender, receiver) = mpsc::channel();
    let (stop_sender, stop_receiver) = mpsc::channel::<()>();

    let thread_handle = thread::spawn(move || {
        // Build menu
        let menu = Menu::new();
        let play_pause_item = MenuItem::new("Play/Pause", true, None);
        let stop_item = MenuItem::new("Stop", true, None);
        let quit_item = MenuItem::new("Quit", true, None);

        menu.append(&play_pause_item).ok();
        menu.append(&stop_item).ok();
        menu.append(&PredefinedMenuItem::separator()).ok();
        menu.append(&quit_item).ok();

        let play_pause_id = play_pause_item.id().clone();
        let stop_id = stop_item.id().clone();
        let quit_id = quit_item.id().clone();

        // Load icon
        let icon = if let Ok(img) = image::load_from_memory(ICON_DATA) {
            let rgba = img.to_rgba8();
            let (width, height) = (rgba.width(), rgba.height());
            Icon::from_rgba(rgba.into_raw(), width, height).ok()
        } else {
            None
        };

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("PWSP Soundpad");

        if let Some(icon) = icon {
            builder = builder.with_icon(icon);
        }

        let _tray_icon = match builder.build() {
            Ok(tray) => tray,
            Err(e) => {
                tracing::error!("Failed to create system tray: {}", e);
                return;
            }
        };

        // Listen for menu events in a loop
        let menu_channel = muda::MenuEvent::receiver();
        loop {
            // Check for stop signal
            if stop_receiver.try_recv().is_ok() {
                break;
            }

            if let Ok(event) = menu_channel.try_recv() {
                if event.id() == &play_pause_id {
                    sender.send(TrayMessage::PlayPause).ok();
                } else if event.id() == &stop_id {
                    sender.send(TrayMessage::Stop).ok();
                } else if event.id() == &quit_id {
                    sender.send(TrayMessage::Quit).ok();
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    Some(TrayHandle {
        receiver,
        _thread: thread_handle,
        _stop_sender: stop_sender,
    })
}
