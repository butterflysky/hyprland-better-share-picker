mod wayland;

use iced::widget::{button, column, container, image, row, scrollable, text};
use iced::{keyboard, Alignment, Element, Event, Length, Subscription, Task};
use std::io::{self, Write};
use wayland::{WaylandEvent, WindowThumbnail};

#[derive(Debug, Clone)]
enum Message {
    Wayland(WaylandEvent),
    Select(u32),
    UiEvent(Event),
    CloseRequested,
}

#[derive(Debug, Clone)]
struct WindowEntry {
    handle_lo: u32,
    class: String,
    title: String,
    mapped_id: u64,
    group_index: usize,
    group_size: usize,
    thumbnail: Option<WindowThumbnail>,
}

struct App {
    windows: Vec<WindowEntry>,
    allow_token: bool,
}

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .run()
}

impl App {
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                windows: parse_window_list(),
                allow_token: std::env::args().any(|arg| arg == "--allow-token"),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Wayland(event) => {
                self.apply_wayland_event(event);
            }
            Message::UiEvent(event) => {
                if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event {
                    if key == keyboard::Key::Named(keyboard::key::Named::Escape) {
                        std::process::exit(1);
                    }
                }
            }
            Message::Select(id) => {
                let flags = if self.allow_token { "r" } else { "" };
                print!("[SELECTION]{}/window:{}\n", flags, id);
                let _ = io::stdout().flush();
                std::process::exit(0);
            }
            Message::CloseRequested => {
                std::process::exit(1);
            }
        }

        Task::none()
    }

    fn apply_wayland_event(&mut self, event: WaylandEvent) {
        match event {
            WaylandEvent::Upsert { .. } | WaylandEvent::Remove { .. } => {}
            WaylandEvent::Thumbnail {
                title,
                app_id,
                group_index,
                group_size,
                width,
                height,
                rgba,
            } => {
                if let Some(existing) = self.windows.iter_mut().find(|w| {
                    w.class == app_id
                        && w.title == title
                        && w.group_index == group_index
                        && w.group_size == group_size
                }) {
                    existing.thumbnail = Some(WindowThumbnail::new(width, height, rgba));
                }
            }
            WaylandEvent::Error { message } => {
                eprintln!("Wayland error: {message}");
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let mut tiles = row!().spacing(16);

        for window in &self.windows {
            let thumb: Element<_> = if let Some(thumbnail) = &window.thumbnail {
                image(thumbnail.handle.clone())
                    .width(Length::Fixed(220.0))
                    .height(Length::Fixed(140.0))
                    .into()
            } else {
                let placeholder = container(text("No preview").size(14))
                    .width(Length::Fixed(220.0))
                    .height(Length::Fixed(140.0))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill);
                placeholder.into()
            };

            let title = if window.title.is_empty() {
                "<untitled>"
            } else {
                window.title.as_str()
            };

            let subtitle = if window.class.is_empty() {
                ""
            } else {
                window.class.as_str()
            };

            let card = column![
                thumb,
                text(title).size(16),
                text(subtitle).size(12)
            ]
                .width(Length::Fixed(220.0))
                .spacing(8)
                .align_x(Alignment::Center);

            let button = button(card)
                .on_press(Message::Select(window.handle_lo))
                .padding(8);

            tiles = tiles.push(button);
        }

        let content = scrollable(tiles.wrap())
            .width(Length::Fill)
            .height(Length::Fill);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            wayland::subscription().map(Message::Wayland),
            iced::event::listen().map(Message::UiEvent),
            iced::window::close_requests().map(|_| Message::CloseRequested),
        ])
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            windows: Vec::new(),
            allow_token: false,
        }
    }
}

fn parse_window_list() -> Vec<WindowEntry> {
    let raw = std::env::var("XDPH_WINDOW_SHARING_LIST").unwrap_or_default();
    let mut entries = Vec::new();
    let mut input = raw.as_str();
    let mut counts: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    let mut temp = Vec::new();

    while let Some(hc) = input.find("[HC>]") {
        let (handle_str, rest) = input.split_at(hc);
        let handle_lo = handle_str.trim().parse::<u32>().unwrap_or(0);
        let rest = &rest["[HC>]".len()..];

        let Some(ht_pos) = rest.find("[HT>]") else { break };
        let (class, rest) = rest.split_at(ht_pos);
        let rest = &rest["[HT>]".len()..];

        let Some(he_pos) = rest.find("[HE>]") else { break };
        let (title, rest) = rest.split_at(he_pos);
        let rest = &rest["[HE>]".len()..];

        let Some(ha_pos) = rest.find("[HA>]") else { break };
        let (mapped, rest) = rest.split_at(ha_pos);
        let rest = &rest["[HA>]".len()..];

        let mapped_id = mapped.trim().parse::<u64>().unwrap_or(0);

        let class = class.to_string();
        let title = title.to_string();
        *counts.entry((class.clone(), title.clone())).or_insert(0) += 1;
        temp.push((handle_lo, class, title, mapped_id));

        input = rest;
    }

    let mut seen: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    for (handle_lo, class, title, mapped_id) in temp {
        let group_key = (class.clone(), title.clone());
        let group_size = *counts.get(&group_key).unwrap_or(&1);
        let entry = seen.entry(group_key).or_insert(0);
        let group_index = *entry;
        *entry += 1;

        entries.push(WindowEntry {
            handle_lo,
            class,
            title,
            mapped_id,
            group_index,
            group_size,
            thumbnail: None,
        });
    }

    entries
}
