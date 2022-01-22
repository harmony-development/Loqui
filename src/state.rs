use std::{
    cell::RefCell,
    ops::Not,
    sync::{
        mpsc::{self, Receiver},
        Arc, RwLock,
    },
};

use client::{
    harmony_rust_sdk::{
        api::{
            chat::Event,
            rest::{About, FileId},
        },
        client::{EventsReadSocket, EventsSocket},
    },
    message::{Content, Message},
    tracing, Cache, Client, FetchEvent,
};
use eframe::egui::{self, TextureHandle, Vec2};
use instant::Instant;
use tokio::sync::mpsc as tokio_mpsc;

use super::utils::*;

use crate::{
    futures::{Futures, UploadMessageResult},
    image_cache::{ImageCache, LoadedImage},
    screen::{BoxedScreen, Screen},
};

pub struct State {
    pub socket_rx_tx: tokio_mpsc::Sender<EventsReadSocket>,
    pub socket_event_rx: mpsc::Receiver<Event>,
    pub client: Option<Client>,
    pub cache: Cache,
    pub image_cache: ImageCache,
    pub loading_images: RefCell<Vec<FileId>>,
    pub uploading_files: Arc<RwLock<Vec<String>>>,
    pub futures: Futures,
    pub latest_errors: Vec<String>,
    pub about: Option<About>,
    pub harmony_lotus: Option<(TextureHandle, Vec2)>,
    pub reset_socket: AtomBool,
    pub connecting_socket: bool,
    pub is_connected: bool,
    pub socket_retry_count: u8,
    pub last_socket_retry: Option<Instant>,
    pub images_rx: Receiver<LoadedImage>,
    pub next_screen: Option<BoxedScreen>,
    pub prev_screen: bool,
}

impl State {
    pub fn new() -> Self {
        let cache = Cache::default();
        let futures = Futures::new();

        let reset_socket = AtomBool::new(false);

        let (socket_rx_tx, mut socket_rx_rx) = tokio_mpsc::channel::<EventsReadSocket>(2);
        let (socket_event_tx, socket_event_rx) = mpsc::channel::<Event>();
        {
            let reset_socket = reset_socket.clone();
            futures.spawn(async move {
                let mut rx = socket_rx_rx.recv().await.expect("closed");

                loop {
                    tokio::select! {
                        Some(sock) = socket_rx_rx.recv() => {
                            rx = sock;
                        }
                        res = rx.get_event(), if reset_socket.get().not() => {
                            match res {
                                Ok(Some(ev)) => {
                                    if socket_event_tx.send(ev).is_err() {
                                        reset_socket.set(true);
                                    }
                                }
                                Err(err) => {
                                    tracing::error!("socket recv error: {}", err);
                                    reset_socket.set(true);
                                }
                                _ => {}
                            }
                        }
                        else => std::hint::spin_loop(),
                    }
                }
            });
        }

        let (images_tx, images_rx) = std::sync::mpsc::sync_channel(100);
        crate::image_cache::op::set_image_channel(images_tx);

        Self {
            socket_rx_tx,
            socket_event_rx,
            reset_socket,
            connecting_socket: false,
            is_connected: false,
            socket_retry_count: 0,
            last_socket_retry: None,
            client: None,
            cache,
            image_cache: Default::default(),
            loading_images: RefCell::new(Vec::new()),
            uploading_files: Arc::new(RwLock::new(Vec::new())),
            futures,
            latest_errors: Vec::new(),
            about: None,
            harmony_lotus: None,
            next_screen: None,
            prev_screen: false,
            images_rx,
        }
    }

    pub fn maintain(&mut self, ctx: &egui::Context) {
        self.futures.run();

        self.handle_upload_message();
        self.handle_about();
        self.handle_errors();
        self.handle_events();
        self.handle_sockets();
        self.handle_images(ctx);
        self.handle_socket_events();

        self.cache.maintain();
    }

    pub fn client(&self) -> &Client {
        self.client.as_ref().expect("client not initialized yet")
    }

    pub fn push_screen<S: Screen>(&mut self, screen: S) {
        self.next_screen = Some(Box::new(screen));
    }

    pub fn pop_screen(&mut self) {
        self.prev_screen = true;
    }

    pub fn run<F, E, O>(&mut self, res: Result<O, E>, f: F) -> bool
    where
        F: FnOnce(&mut Self, O),
        E: std::error::Error + Send + Sync + 'static,
    {
        match res {
            Ok(val) => {
                f(self, val);
                false
            }
            Err(err) => {
                let msg = err.to_string();
                let exit = msg.contains("h.bad-session");
                self.latest_errors.push(msg);
                exit
            }
        }
    }

    pub fn reset_socket_state(&mut self) {
        self.socket_retry_count = 0;
        self.last_socket_retry = None;
        self.is_connected = false;
        self.connecting_socket = false;
        self.reset_socket.set(false);
    }

    #[inline(always)]
    fn handle_sockets(&mut self) {
        let last_retry_period_passed = self.last_socket_retry.map_or(true, |ins| ins.elapsed().as_secs() > 5);
        let retry_socket = last_retry_period_passed
            && self.connecting_socket.not()
            && self.socket_retry_count < 5
            && self.reset_socket.get();

        if retry_socket {
            self.is_connected = false;
            self.connecting_socket = true;
            self.socket_retry_count += 1;
            self.last_socket_retry = Some(Instant::now());
            spawn_client_fut!(self, |client| client.connect_socket(Vec::new()).await);
        }

        handle_future!(self, |res: ClientResult<EventsSocket>| {
            self.connecting_socket = false;
            self.run(res, |state, sock| {
                let (_, rx) = sock.split();
                state.socket_rx_tx.try_send(rx).expect("socket task panicked");
                state.reset_socket.set(false);
                state.is_connected = true;
                state.socket_retry_count = 0;
                state.last_socket_retry = None;
            });
        });
    }

    #[inline(always)]
    fn handle_errors(&mut self) {
        handle_future!(self, |res: ClientResult<()>| {
            self.run(res, |_, _| {});
        });
        handle_future!(self, |res: ClientResult<u64>| {
            self.run(res, |_, _| {});
        });
    }

    #[inline(always)]
    fn handle_upload_message(&mut self) {
        handle_future!(self, |res: ClientResult<Option<UploadMessageResult>>| {
            match res {
                Ok(maybe_upload) => {
                    let Some(UploadMessageResult { guild_id, channel_id, attachments }) = maybe_upload else { return };

                    {
                        let mut uploading_files = self.uploading_files.write().expect("poisoned");
                        for attachment in attachments.iter() {
                            if let Some(pos) = uploading_files.iter().position(|name| name == &attachment.name) {
                                uploading_files.remove(pos);
                            }
                        }
                    }

                    let message = Message {
                        content: Content::Files(attachments),
                        sender: self.client().user_id(),
                        ..Default::default()
                    };
                    let echo_id = self.cache.prepare_send_message(guild_id, channel_id, message.clone());
                    spawn_evs!(self, |evs, client| {
                        client.send_message(echo_id, guild_id, channel_id, message, evs).await?;
                    });
                }
                Err(err) => {
                    self.latest_errors.push(err.to_string());
                    self.uploading_files.write().expect("poisoned").clear();
                }
            }
        });
    }

    #[inline(always)]
    fn handle_events(&mut self) {
        handle_future!(self, |res: ClientResult<Vec<FetchEvent>>| {
            self.run(res, |state, events| {
                let mut posts = Vec::new();
                for event in events {
                    match event {
                        FetchEvent::Attachment { attachment, file } => {
                            if attachment.is_raster_image() {
                                crate::image_cache::op::decode_image(
                                    file.data().clone(),
                                    attachment.id,
                                    attachment.name,
                                );
                            }
                        }
                        event => state.cache.process_event(&mut posts, event),
                    }
                }
                if let Some(client) = state.client.as_ref().cloned() {
                    spawn_future!(state, {
                        async move {
                            let mut events = Vec::with_capacity(posts.len());
                            for post in posts {
                                client.process_post(&mut events, post).await?;
                            }
                            ClientResult::Ok(events)
                        }
                    });
                }
            });
        });
    }

    #[inline(always)]
    fn handle_images(&mut self, ctx: &egui::Context) {
        while let Ok(image) = self.images_rx.try_recv() {
            let maybe_pos = self.loading_images.borrow().iter().position(|id| image.id() == id);
            if let Some(pos) = maybe_pos {
                self.loading_images.borrow_mut().remove(pos);
            }
            self.image_cache.add(ctx, image);
        }
    }

    #[inline(always)]
    fn handle_socket_events(&mut self) {
        let mut evs = Vec::new();
        while let Ok(ev) = self.socket_event_rx.try_recv() {
            evs.push(FetchEvent::Harmony(ev));
        }
        if !evs.is_empty() {
            spawn_future!(self, std::future::ready(ClientResult::Ok(evs)));
        }
    }

    #[inline(always)]
    fn handle_about(&mut self) {
        handle_future!(self, |res: ClientResult<About>| {
            self.run(res, |state, about| {
                state.about = Some(about);
            });
        });
    }
}
