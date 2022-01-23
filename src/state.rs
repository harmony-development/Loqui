use std::{
    cell::RefCell,
    ops::Not,
    sync::{mpsc::Receiver, Arc, RwLock},
};

use client::{
    harmony_rust_sdk::{
        api::rest::{About, FileId},
        client::{EventsReadSocket, EventsSocket},
    },
    message::{Content, Message},
    tracing, Cache, Client, EventSender, FetchEvent,
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
    pub event_sender: EventSender,
    pub post_client_tx: tokio_mpsc::Sender<Client>,
}

impl State {
    pub fn new() -> Self {
        let futures = Futures::new();

        // Set up event processing and the cache
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (post_tx, mut post_rx) = tokio::sync::mpsc::unbounded_channel();
        let (post_client_tx, mut post_client_rx) = tokio::sync::mpsc::channel::<Client>(2);

        {
            let event_tx = event_tx.clone();
            futures.spawn(async move {
                let mut client = post_client_rx.recv().await.expect("no clients");

                loop {
                    tokio::select! {
                        Some(new_client) = post_client_rx.recv() => {
                            client = new_client;
                        }
                        Some(post) = post_rx.recv() => {
                            if let Err(err) = client.process_post(&event_tx, post).await {
                                tracing::error!("failed to post process event: {}", err);
                            }
                        }
                        else => break,
                    }
                }
            });
        }

        let cache = Cache::new(
            event_rx,
            post_tx,
            Box::new(|ev| match ev {
                FetchEvent::Attachment { attachment, file } => {
                    if attachment.is_raster_image() {
                        crate::image_cache::op::decode_image(file.data().clone(), attachment.id, attachment.name);
                    }
                    None
                }
                ev => Some(ev),
            }),
        );

        // start socket processor task
        let reset_socket = AtomBool::new(false);

        let (socket_rx_tx, mut socket_rx_rx) = tokio_mpsc::channel::<EventsReadSocket>(2);
        {
            let event_tx = event_tx.clone();
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
                                    if event_tx.send(FetchEvent::Harmony(ev)).is_err() {
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
                        else => break,
                    }
                }
            });
        }

        let (images_tx, images_rx) = std::sync::mpsc::sync_channel(100);
        crate::image_cache::op::set_image_channel(images_tx);

        Self {
            socket_rx_tx,
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
            event_sender: event_tx,
            post_client_tx,
        }
    }

    pub fn maintain(&mut self, ctx: &egui::Context) {
        self.futures.run();

        self.handle_upload_message();
        self.handle_about();
        self.handle_errors();
        self.handle_sockets();
        self.handle_images(ctx);

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
                let exit = msg.contains("bad-session") || msg.contains("invalid-session");
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
    fn handle_about(&mut self) {
        handle_future!(self, |res: ClientResult<About>| {
            self.run(res, |state, about| {
                state.about = Some(about);
            });
        });
    }
}
