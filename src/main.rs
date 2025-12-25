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
    Cancel,
    CloseRequested,
}

#[derive(Debug, Clone)]
struct WindowEntry {
    id: u32,
    title: String,
    app_id: String,
    thumbnail: Option<WindowThumbnail>,
}

struct App {
    windows: Vec<WindowEntry>,
}

fn main() -> iced::Result {
    iced::application("Hyprland Better Share Picker", App::update, App::view)
        .subscription(App::subscription)
        .run()
}

impl App {
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
                print!("wayland:0x{:x}\n", id);
                let _ = io::stdout().flush();
                std::process::exit(0);
            }
            Message::Cancel | Message::CloseRequested => {
                std::process::exit(1);
            }
        }

        Task::none()
    }

    fn apply_wayland_event(&mut self, event: WaylandEvent) {
        match event {
            WaylandEvent::Upsert {
                id,
                title,
                app_id,
            } => {
                if let Some(existing) = self.windows.iter_mut().find(|w| w.id == id) {
                    existing.title = title;
                    existing.app_id = app_id;
                } else {
                    self.windows.push(WindowEntry {
                        id,
                        title,
                        app_id,
                        thumbnail: None,
                    });
                }
            }
            WaylandEvent::Remove { id } => {
                self.windows.retain(|w| w.id != id);
            }
            WaylandEvent::Thumbnail {
                id,
                width,
                height,
                rgba,
            } => {
                if let Some(existing) = self.windows.iter_mut().find(|w| w.id == id) {
                    existing.thumbnail = Some(WindowThumbnail::new(width, height, rgba));
                }
            }
            WaylandEvent::Error { message } => {
                eprintln!("Wayland error: {message}");
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let mut tiles = row!().spacing(16).wrap();

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
                    .center_x()
                    .center_y();
                placeholder.into()
            };

            let title = if window.title.is_empty() {
                "<untitled>"
            } else {
                window.title.as_str()
            };

            let card = column![thumb, text(title).size(16)]
                .width(Length::Fixed(220.0))
                .spacing(8)
                .align_items(Alignment::Center);

            let button = button(card)
                .on_press(Message::Select(window.id))
                .padding(8);

            tiles = tiles.push(button);
        }

        let content = scrollable(tiles)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16);

        container(content).width(Length::Fill).height(Length::Fill).into()
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
        }
    }
}
