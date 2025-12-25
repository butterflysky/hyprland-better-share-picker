use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, StreamExt};
use iced::stream;
use iced::Subscription;
use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::globals::ProvidesBoundGlobal;
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use std::collections::HashMap;
use wayland_client::globals::registry_queue_init;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::{wl_registry, wl_shm};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1, zwlr_foreign_toplevel_manager_v1,
};

mod protocol {
    include!(concat!(env!("OUT_DIR"), "/hyprland_toplevel_export.rs"));
}

use protocol::hyprland_toplevel_export::hyprland_toplevel_export_frame_v1;
use protocol::hyprland_toplevel_export::hyprland_toplevel_export_manager_v1;

#[derive(Debug, Clone)]
pub enum WaylandEvent {
    Upsert { id: u32, title: String, app_id: String },
    Remove { id: u32 },
    Thumbnail {
        title: String,
        app_id: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct WindowThumbnail {
    pub handle: iced::widget::image::Handle,
}

impl WindowThumbnail {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            handle: iced::widget::image::Handle::from_rgba(width, height, rgba),
        }
    }
}

pub fn subscription() -> Subscription<WaylandEvent> {
    Subscription::run(wayland_stream)
}

fn wayland_stream() -> impl iced::futures::Stream<Item = WaylandEvent> {
    stream::channel(100, |mut output: iced::futures::channel::mpsc::Sender<WaylandEvent>| async move {
        let (tx, mut rx) = mpsc::unbounded::<WaylandEvent>();

        std::thread::spawn(move || {
            if let Err(error) = run_wayland(tx.clone()) {
                let _ = tx.unbounded_send(WaylandEvent::Error {
                    message: error.to_string(),
                });
            }
        });

        while let Some(event) = rx.next().await {
            let _ = output.send(event).await;
        }
    })
}
fn run_wayland(
    sender: mpsc::UnboundedSender<WaylandEvent>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<WaylandState>(&conn)?;
    let qh = queue.handle();

    let shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())?;
    let toplevel_manager = globals.bind::<zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1, _, _>(
        &qh,
        1..=3,
        (),
    )?;
    let export_manager = globals.bind::<hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1, _, _>(
        &qh,
        1..=2,
        (),
    )?;

    let mut state = WaylandState::new(sender, shm, toplevel_manager, export_manager);

    loop {
        queue.blocking_dispatch(&mut state)?;
        conn.flush()?;
    }
}

struct WaylandState {
    sender: mpsc::UnboundedSender<WaylandEvent>,
    shm: wl_shm::WlShm,
    toplevel_manager: zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1,
    export_manager: hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1,
    toplevels: HashMap<u32, ToplevelEntry>,
    pending_frames: HashMap<u32, PendingFrame>,
    slot_pool: Option<SlotPool>,
    slot_pool_size: usize,
}

impl WaylandState {
    fn new(
        sender: mpsc::UnboundedSender<WaylandEvent>,
        shm: wl_shm::WlShm,
        toplevel_manager: zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1,
        export_manager: hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1,
    ) -> Self {
        Self {
            sender,
            shm,
            toplevel_manager,
            export_manager,
            toplevels: HashMap::new(),
            pending_frames: HashMap::new(),
            slot_pool: None,
            slot_pool_size: 0,
        }
    }

    fn ensure_slot_pool(&mut self, size: usize) -> &mut SlotPool {
        if self.slot_pool.is_none() {
            let pool = SlotPool::new(size, self).expect("failed to create shm pool");
            self.slot_pool = Some(pool);
            self.slot_pool_size = size;
        }
        if self.slot_pool_size < size {
            self.slot_pool
                .as_mut()
                .expect("slot pool missing")
                .resize(size)
                .expect("failed to resize shm pool");
            self.slot_pool_size = size;
        }
        self.slot_pool.as_mut().expect("slot pool missing")
    }

    fn request_thumbnail(
        &mut self,
        qh: &QueueHandle<Self>,
        handle: &zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
    ) {
        let frame = self.export_manager.capture_toplevel_with_wlr_toplevel_handle(
            0,
            handle,
            qh,
            (),
        );

        let id = frame.id().protocol_id();
        let toplevel_id = handle.id().protocol_id();
        self.pending_frames.insert(
            id,
            PendingFrame {
                toplevel_id,
                width: 0,
                height: 0,
                stride: 0,
                format: None,
                y_invert: false,
                buffer: None,
            },
        );
    }

    fn send_upsert(&self, id: u32, title: &str, app_id: &str) {
        let _ = self.sender.unbounded_send(WaylandEvent::Upsert {
            id,
            title: title.to_string(),
            app_id: app_id.to_string(),
        });
    }

    fn send_thumbnail(&self, id: u32, width: u32, height: u32, rgba: Vec<u8>) {
        if let Some(entry) = self.toplevels.get(&id) {
            let _ = self.sender.unbounded_send(WaylandEvent::Thumbnail {
                title: entry.title.clone(),
                app_id: entry.app_id.clone(),
                width,
                height,
                rgba,
            });
        }
    }

    fn send_remove(&self, id: u32) {
        let _ = self.sender.unbounded_send(WaylandEvent::Remove { id });
    }
}

struct ToplevelEntry {
    handle: zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
    title: String,
    app_id: String,
    captured: bool,
}

struct PendingFrame {
    toplevel_id: u32,
    width: u32,
    height: u32,
    stride: u32,
    format: Option<wl_shm::Format>,
    y_invert: bool,
    buffer: Option<Buffer>,
}

impl ProvidesBoundGlobal<wl_shm::WlShm, 1> for WaylandState {
    fn bound_global(&self) -> Result<wl_shm::WlShm, GlobalError> {
        Ok(self.shm.clone())
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_shm::WlShm,
        _event: wl_shm::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } => {
                let id = toplevel.id().protocol_id();
                let entry = ToplevelEntry {
                    handle: toplevel.clone(),
                    title: String::new(),
                    app_id: String::new(),
                    captured: false,
                };
                state.toplevels.insert(id, entry);
                state.send_upsert(id, "", "");
                if let Some(entry) = state.toplevels.get_mut(&id) {
                    if !entry.captured {
                        entry.captured = true;
                        state.request_thumbnail(qh, &toplevel);
                    }
                }
            }
            zwlr_foreign_toplevel_manager_v1::Event::Finished => {}
            _ => {}
        }
    }

    wayland_client::event_created_child!(
        WaylandState,
        zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1,
        [
            zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE
                => (zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1, ()),
        ]
    );
}

impl Dispatch<zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id().protocol_id();
        if let Some(entry) = state.toplevels.get_mut(&id) {
            match event {
                zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                    entry.title = title;
                    let title = entry.title.clone();
                    let app_id = entry.app_id.clone();
                    state.send_upsert(id, &title, &app_id);
                }
                zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                    entry.app_id = app_id;
                    let title = entry.title.clone();
                    let app_id = entry.app_id.clone();
                    state.send_upsert(id, &title, &app_id);
                }
                zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                    state.toplevels.remove(&id);
                    state.send_remove(id);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1,
        _event: hyprland_toplevel_export_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<hyprland_toplevel_export_frame_v1::HyprlandToplevelExportFrameV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &hyprland_toplevel_export_frame_v1::HyprlandToplevelExportFrameV1,
        event: hyprland_toplevel_export_frame_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = proxy.id().protocol_id();
        if !state.pending_frames.contains_key(&id) {
            return;
        }

        match event {
            hyprland_toplevel_export_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                if let Some(frame) = state.pending_frames.get_mut(&id) {
                    frame.format = match format {
                        WEnum::Value(value) => Some(value),
                        _ => None,
                    };
                    frame.width = width;
                    frame.height = height;
                    frame.stride = stride;
                }
            }
            hyprland_toplevel_export_frame_v1::Event::Flags { flags } => {
                if let Some(frame) = state.pending_frames.get_mut(&id) {
                    if let WEnum::Value(value) = flags {
                        frame.y_invert =
                            value.contains(hyprland_toplevel_export_frame_v1::Flags::YInvert);
                    }
                }
            }
            hyprland_toplevel_export_frame_v1::Event::BufferDone => {
                let (width, height, stride, format, has_buffer) = {
                    let frame = state.pending_frames.get(&id).expect("frame missing");
                    (
                        frame.width,
                        frame.height,
                        frame.stride,
                        frame.format,
                        frame.buffer.is_some(),
                    )
                };
                if has_buffer {
                    return;
                }
                let format = match format {
                    Some(wl_shm::Format::Argb8888) => wl_shm::Format::Argb8888,
                    Some(wl_shm::Format::Xrgb8888) => wl_shm::Format::Xrgb8888,
                    _ => {
                        proxy.destroy();
                        return;
                    }
                };

                let size = (stride * height) as usize;
                let pool = state.ensure_slot_pool(size);
                let (buffer, _) = pool
                    .create_buffer(width as i32, height as i32, stride as i32, format)
                    .expect("failed to create shm buffer");

                proxy.copy(buffer.wl_buffer(), 0);
                if let Some(frame) = state.pending_frames.get_mut(&id) {
                    frame.buffer = Some(buffer);
                }
            }
            hyprland_toplevel_export_frame_v1::Event::Ready { .. } => {
                let (buffer, width, height, stride, format, y_invert, toplevel_id) = {
                    let frame = state.pending_frames.get_mut(&id).expect("frame missing");
                    (
                        frame.buffer.take(),
                        frame.width,
                        frame.height,
                        frame.stride,
                        frame.format.unwrap_or(wl_shm::Format::Argb8888),
                        frame.y_invert,
                        frame.toplevel_id,
                    )
                };
                if let (Some(buffer), Some(pool)) = (buffer, state.slot_pool.as_mut()) {
                    if let Some(data) = buffer.canvas(pool) {
                        let rgba = convert_to_rgba(data, width, height, stride, format, y_invert);
                        state.send_thumbnail(toplevel_id, width, height, rgba);
                    }
                }
                proxy.destroy();
                state.pending_frames.remove(&id);
            }
            hyprland_toplevel_export_frame_v1::Event::Failed => {
                proxy.destroy();
                state.pending_frames.remove(&id);
            }
            _ => {}
        }
    }
}

fn convert_to_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    y_invert: bool,
) -> Vec<u8> {
    let mut out = vec![0u8; (width * height * 4) as usize];
    let row_bytes = (width * 4) as usize;
    for y in 0..height {
        let src_y = if y_invert { height - 1 - y } else { y };
        let src_start = (src_y * stride) as usize;
        let src_row = &data[src_start..src_start + row_bytes];
        let dst_start = (y * width * 4) as usize;
        let dst_row = &mut out[dst_start..dst_start + row_bytes];

        for x in 0..width {
            let i = (x * 4) as usize;
            let px = u32::from_ne_bytes([
                src_row[i],
                src_row[i + 1],
                src_row[i + 2],
                src_row[i + 3],
            ]);
            let (a, r, g, b) = match format {
                wl_shm::Format::Argb8888 => (
                    ((px >> 24) & 0xff) as u8,
                    ((px >> 16) & 0xff) as u8,
                    ((px >> 8) & 0xff) as u8,
                    (px & 0xff) as u8,
                ),
                wl_shm::Format::Xrgb8888 => (
                    0xff,
                    ((px >> 16) & 0xff) as u8,
                    ((px >> 8) & 0xff) as u8,
                    (px & 0xff) as u8,
                ),
                _ => (0xff, 0, 0, 0),
            };
            dst_row[i] = r;
            dst_row[i + 1] = g;
            dst_row[i + 2] = b;
            dst_row[i + 3] = a;
        }
    }
    out
}
