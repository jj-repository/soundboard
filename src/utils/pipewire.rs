use crate::VIRTUAL_MIC_NAME;
use crate::types::pipewire::{AudioDevice, DeviceType, Port, Terminate};
use pipewire::{
    context::ContextRc, link::Link, main_loop::MainLoopRc, properties::properties,
    registry::GlobalObject, spa::utils::dict::DictRef,
};
use std::{collections::HashMap, error::Error, sync::OnceLock, thread, time::Instant};
use tokio::{
    sync::{Mutex, mpsc},
    time::{Duration, timeout},
};

type DeviceSnapshot = (Vec<AudioDevice>, Vec<AudioDevice>);
const DEVICE_CACHE_TTL: Duration = Duration::from_secs(2);

fn device_cache() -> &'static Mutex<Option<(Instant, DeviceSnapshot)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, DeviceSnapshot)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Drop the cached device snapshot so the next `get_all_devices()` call re-enumerates.
/// Call after operations that change the device graph (virtual mic creation, link changes).
pub async fn invalidate_device_cache() {
    *device_cache().lock().await = None;
}

/// Assign a PipeWire port to the appropriate field on an AudioDevice
fn assign_port_to_device(device: &mut AudioDevice, port: Port) {
    match port.name.as_str() {
        "input_FL" => device.input_fl = Some(port),
        "input_FR" => device.input_fr = Some(port),
        "output_FL" => device.output_fl = Some(port),
        "output_FR" => device.output_fr = Some(port),
        "capture_FL" => device.output_fl = Some(port),
        "capture_FR" => device.output_fr = Some(port),
        "input_MONO" => {
            device.input_fl = Some(port.clone());
            device.input_fr = Some(port);
        }
        "output_MONO" | "capture_MONO" => {
            device.output_fl = Some(port.clone());
            device.output_fr = Some(port);
        }
        _ => {}
    }
}

fn parse_global_object(
    global_object: &GlobalObject<&DictRef>,
) -> (Option<AudioDevice>, Option<Port>) {
    if let Some(props) = global_object.props {
        if let Some(media_class) = props.get("media.class") {
            let node_id = global_object.id;
            let node_nick = props.get("node.nick");
            let node_name = props.get("node.name");
            let node_description = props.get("node.description");

            return if media_class.starts_with("Audio/Source") {
                let input_device = AudioDevice {
                    id: node_id,
                    nick: node_nick
                        .unwrap_or(node_description.unwrap_or(node_name.unwrap_or_default()))
                        .to_string(),
                    name: node_name.unwrap_or_default().to_string(),
                    device_type: DeviceType::Input,

                    input_fl: None,
                    input_fr: None,
                    output_fl: None,
                    output_fr: None,
                };
                (Some(input_device), None)
            } else if media_class.starts_with("Stream/Output/Audio") {
                let output_device = AudioDevice {
                    id: node_id,
                    nick: node_nick
                        .unwrap_or(node_description.unwrap_or(node_name.unwrap_or_default()))
                        .to_string(),
                    name: node_name.unwrap_or_default().to_string(),
                    device_type: DeviceType::Output,

                    input_fl: None,
                    input_fr: None,
                    output_fl: None,
                    output_fr: None,
                };
                (Some(output_device), None)
            } else {
                (None, None)
            };
        } else if props.get("port.direction").is_some() {
            let node_id = match props.get("node.id").and_then(|s| s.parse::<u32>().ok()) {
                Some(id) => id,
                None => return (None, None),
            };
            let port_id = match props.get("port.id").and_then(|s| s.parse::<u32>().ok()) {
                Some(id) => id,
                None => return (None, None),
            };
            let port_name = match props.get("port.name") {
                Some(name) => name,
                None => return (None, None),
            };

            let port = Port {
                node_id,
                port_id,
                name: port_name.to_string(),
            };

            return (None, Some(port));
        }
    }
    (None, None)
}

async fn pw_get_global_objects_thread(
    main_sender: mpsc::Sender<(Option<AudioDevice>, Option<Port>)>,
    pw_receiver: pipewire::channel::Receiver<Terminate>,
) {
    pipewire::init();

    let main_loop = match MainLoopRc::new(None) {
        Ok(ml) => ml,
        Err(e) => {
            tracing::error!("Failed to initialize pipewire main loop: {}", e);
            return;
        }
    };

    let _receiver = pw_receiver.attach(main_loop.loop_(), {
        let _main_loop = main_loop.clone();
        move |_| _main_loop.quit()
    });

    let context = match ContextRc::new(&main_loop, None) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!("Failed to create pipewire context: {}", e);
            return;
        }
    };
    let core = match context.connect(None) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to connect to pipewire context: {}", e);
            return;
        }
    };
    let registry = match core.get_registry() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to get registry from pipewire context: {}", e);
            return;
        }
    };

    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            let (device, port) = parse_global_object(global);
            let sender_clone = main_sender.clone();
            tokio::task::spawn(async move {
                sender_clone.send((device, port)).await.ok();
            });
        })
        .register();

    main_loop.run();
}

pub async fn get_all_devices() -> Result<DeviceSnapshot, Box<dyn Error>> {
    {
        let guard = device_cache().lock().await;
        if let Some((fetched_at, snapshot)) = guard.as_ref() {
            if fetched_at.elapsed() < DEVICE_CACHE_TTL {
                return Ok(snapshot.clone());
            }
        }
    }

    let snapshot = enumerate_devices().await?;
    *device_cache().lock().await = Some((Instant::now(), snapshot.clone()));
    Ok(snapshot)
}

async fn enumerate_devices() -> Result<DeviceSnapshot, Box<dyn Error>> {
    let (main_sender, mut main_receiver) = mpsc::channel(10);
    let (pw_sender, pw_receiver) = pipewire::channel::channel();

    let _pw_thread =
        tokio::spawn(async move { pw_get_global_objects_thread(main_sender, pw_receiver).await });

    let mut input_devices: HashMap<u32, AudioDevice> = HashMap::new();
    let mut output_devices: HashMap<u32, AudioDevice> = HashMap::new();
    let mut ports: Vec<Port> = vec![];

    loop {
        // If no message arrives within 100ms, assume the pipewire thread finished enumerating
        match timeout(Duration::from_millis(100), main_receiver.recv()).await {
            Ok(Some((device, port))) => {
                if let Some(device) = device {
                    match device.device_type {
                        DeviceType::Input => {
                            input_devices.insert(device.id, device);
                        }
                        DeviceType::Output => {
                            output_devices.insert(device.id, device);
                        }
                    }
                } else if let Some(port) = port {
                    ports.push(port);
                }
            }
            Ok(None) | Err(_) => {
                let _ = pw_sender.send(Terminate {});

                for port in ports {
                    let node_id = port.node_id;

                    let device = input_devices
                        .get_mut(&node_id)
                        .or_else(|| output_devices.get_mut(&node_id));

                    if let Some(device) = device {
                        assign_port_to_device(device, port);
                    }
                }

                let mut input_devices: Vec<AudioDevice> = input_devices.values().cloned().collect();
                let mut output_devices: Vec<AudioDevice> =
                    output_devices.values().cloned().collect();

                input_devices.sort_by_key(|a| a.id);
                output_devices.sort_by_key(|a| a.id);

                return Ok((input_devices, output_devices));
            }
        }
    }
}

pub async fn get_device(device_name: &str) -> Result<AudioDevice, Box<dyn Error>> {
    if let Ok(device) = get_device_impl(device_name).await {
        return Ok(device);
    }
    invalidate_device_cache().await;
    get_device_impl(device_name).await
}

async fn get_device_impl(device_name: &str) -> Result<AudioDevice, Box<dyn Error>> {
    let (mut input_devices, output_devices) = get_all_devices().await?;
    input_devices.extend(output_devices);

    for device in input_devices {
        if device.name == device_name {
            return Ok(device);
        }
    }

    Err("Device not found".into())
}

pub fn create_virtual_mic() -> Result<pipewire::channel::Sender<Terminate>, Box<dyn Error>> {
    let (pw_sender, pw_receiver) = pipewire::channel::channel::<Terminate>();

    let _pw_thread = thread::spawn(move || {
        pipewire::init();

        let main_loop = match MainLoopRc::new(None) {
            Ok(ml) => ml,
            Err(e) => {
                tracing::error!("Failed to initialize pipewire main loop: {}", e);
                return;
            }
        };
        let context = match ContextRc::new(&main_loop, None) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::error!("Failed to create pipewire context: {}", e);
                return;
            }
        };
        let core = match context.connect(None) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to connect to pipewire context: {}", e);
                return;
            }
        };

        let props = properties!(
            "factory.name" => "support.null-audio-sink",
            "node.name" => VIRTUAL_MIC_NAME,
            "node.description" => "PWSP Virtual Mic",
            "media.class" => "Audio/Source/Virtual",
            "audio.position" => "[ FL FR ]",
            "audio.channels" => "2",
            "object.linger" => "false",
        );

        let _node = match core.create_object::<pipewire::node::Node>("adapter", &props) {
            Ok(node) => node,
            Err(e) => {
                tracing::error!("Failed to create virtual mic: {}", e);
                return;
            }
        };

        let _receiver = pw_receiver.attach(main_loop.loop_(), {
            let _main_loop = main_loop.clone();
            move |_| _main_loop.quit()
        });

        main_loop.run();
    });

    Ok(pw_sender)
}

pub fn create_link(
    output_fl: Port,
    output_fr: Port,
    input_fl: Port,
    input_fr: Port,
) -> Result<pipewire::channel::Sender<Terminate>, Box<dyn Error>> {
    let (pw_sender, pw_receiver) = pipewire::channel::channel::<Terminate>();

    let _pw_thread = thread::spawn(move || {
        pipewire::init();

        let main_loop = match MainLoopRc::new(None) {
            Ok(ml) => ml,
            Err(e) => {
                tracing::error!("Failed to initialize pipewire main loop: {}", e);
                return;
            }
        };
        let context = match ContextRc::new(&main_loop, None) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::error!("Failed to create pipewire context: {}", e);
                return;
            }
        };
        let core = match context.connect(None) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to connect to pipewire context: {}", e);
                return;
            }
        };

        let props_fl = properties! {
            "link.output.node" => format!("{}", output_fl.node_id).as_str(),
            "link.output.port" => format!("{}", output_fl.port_id).as_str(),
            "link.input.node"  => format!("{}", input_fl.node_id).as_str(),
            "link.input.port"  => format!("{}", input_fl.port_id).as_str(),
        };
        let props_fr = properties! {
            "link.output.node" => format!("{}", output_fr.node_id).as_str(),
            "link.output.port" => format!("{}", output_fr.port_id).as_str(),
            "link.input.node"  => format!("{}", input_fr.node_id).as_str(),
            "link.input.port"  => format!("{}", input_fr.port_id).as_str(),
        };

        let _link_fl = match core.create_object::<Link>("link-factory", &props_fl) {
            Ok(link) => link,
            Err(e) => {
                tracing::error!("Failed to create link FL: {}", e);
                return;
            }
        };
        let _link_fr = match core.create_object::<Link>("link-factory", &props_fr) {
            Ok(link) => link,
            Err(e) => {
                tracing::error!("Failed to create link FR: {}", e);
                return;
            }
        };

        let _receiver = pw_receiver.attach(main_loop.loop_(), {
            let _main_loop = main_loop.clone();
            move |_| _main_loop.quit()
        });

        main_loop.run();
    });

    Ok(pw_sender)
}
