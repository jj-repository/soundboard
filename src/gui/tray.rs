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

pub fn start_tray() -> Option<TrayHandle> {
    use muda::{Menu, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIconBuilder};

    let (sender, receiver) = mpsc::channel();
    let (stop_sender, stop_receiver) = mpsc::channel::<()>();

    let thread_handle = thread::spawn(move || {
        let menu = Menu::new();
        let play_pause_item = MenuItem::new("Play/Pause", true, None);
        let stop_item = MenuItem::new("Stop", true, None);
        let quit_item = MenuItem::new("Quit", true, None);

        if let Err(e) = menu.append(&play_pause_item) {
            tracing::error!("Failed to append Play/Pause tray item: {}", e);
        }
        if let Err(e) = menu.append(&stop_item) {
            tracing::error!("Failed to append Stop tray item: {}", e);
        }
        if let Err(e) = menu.append(&PredefinedMenuItem::separator()) {
            tracing::error!("Failed to append tray separator: {}", e);
        }
        if let Err(e) = menu.append(&quit_item) {
            tracing::error!("Failed to append Quit tray item: {}", e);
        }

        let play_pause_id = play_pause_item.id().clone();
        let stop_id = stop_item.id().clone();
        let quit_id = quit_item.id().clone();

        let icon = if let Ok(img) = image::load_from_memory(ICON_DATA) {
            let rgba = img.to_rgba8();
            let (width, height) = (rgba.width(), rgba.height());
            Icon::from_rgba(rgba.into_raw(), width, height).ok()
        } else {
            None
        };

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Soundboard");

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

        let menu_channel = muda::MenuEvent::receiver();
        loop {
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
