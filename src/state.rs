use std::{
    cell::RefCell,
    ops::Not,
    sync::{
        mpsc::{Receiver, SyncSender},
        Arc, RwLock,
    },
};

use client::{
    harmony_rust_sdk::{
        api::{
            chat::{stream_event::Event as ChatEvent, Event},
            rest::{About, FileId},
        },
        client::{EventsReadSocket, EventsSocket},
    },
    message::{Content, Message},
    smol_str::SmolStr,
    tracing, Cache, Client, EventSender, FetchEvent,
};
use eframe::{
    egui::{self, TextureHandle, Vec2},
    epi::IntegrationInfo,
};
use instant::Instant;
use tokio::sync::mpsc as tokio_mpsc;

use super::utils::*;

use crate::{
    config::{Config, LocalConfig},
    futures::{Futures, UploadMessageResult},
    image_cache::{ImageCache, LoadedImage},
    screen::{BoxedScreen, Screen},
};

/// Big, monolithic struct that holds all state for everything.
pub struct State {
    /// State for managing the event socket ///
    /// Channel to send new connected event sockets to the event processor task.
    pub socket_rx_tx: tokio_mpsc::Sender<EventsReadSocket>,
    /// Whether to reset the socket or not.
    pub reset_socket: AtomBool,
    /// Whether we are currently connecting a new socket or not.
    pub connecting_socket: bool,
    /// Whether we are currently connected with a socket.
    pub is_connected: bool,
    /// The amount of tries we tried to connect a socket, but failed.
    pub socket_retry_count: u8,
    /// Last time we tried to connect to a socket.
    pub last_socket_retry: Option<Instant>,

    /// Config that is synced across all loqui instances for this user.
    pub config: Config,
    /// Config that is local to this loqui instance.
    pub local_config: LocalConfig,

    /// Futures task manager and output handler.
    pub futures: Futures,

    /// The current client. Will be `None` if none is connected.
    pub client: Option<Client>,
    /// The cache used to store everything harmony related.
    pub cache: Cache,
    /// Channel to send events to the cache for processing.
    pub event_sender: EventSender,
    /// Channel to send newly connected clients to the post event process task.
    pub post_client_tx: tokio_mpsc::Sender<Client>,

    /// Cache containing decoded images in memory.
    pub image_cache: ImageCache,
    /// Channel for receiving decoded images.
    pub images_rx: Receiver<LoadedImage>,
    /// Channel for decoded images to be sent.
    pub images_tx: SyncSender<LoadedImage>,

    /// Images we are currently loading.
    pub loading_images: RefCell<Vec<FileId>>,
    /// Files that are being uploaded.
    pub uploading_files: Arc<RwLock<Vec<String>>>,
    /// Screen to push to screen stack on next frame.
    pub next_screen: Option<BoxedScreen>,
    /// Whether pop the current screen on next frame.
    pub prev_screen: bool,

    /// Latest errors received.
    pub latest_errors: Vec<String>,
    /// About server information. Will be `None` if we aren't connected
    /// to any server.
    pub about: Option<About>,
    /// The harmony lotus. This is loaded on app start.
    pub harmony_lotus: Option<(TextureHandle, Vec2)>,
    pub integration_info: Option<IntegrationInfo>,
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

        let cache = Cache::new(event_rx, post_tx);

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
            images_tx,
            event_sender: event_tx,
            post_client_tx,
            config: Config::default(),
            local_config: LocalConfig::load(),
            integration_info: None,
        }
    }

    pub fn init(&mut self, ctx: &egui::Context, frame: &eframe::epi::Frame) {
        crate::image_cache::op::set_image_channel(self.images_tx.clone(), frame.lock().repaint_signal.clone());

        self.integration_info = Some(frame.info());
        #[cfg(not(target_arch = "wasm32"))]
        if self.local_config.scale_factor < 0.5 {
            self.local_config.scale_factor = self
                .integration_info
                .as_ref()
                .and_then(|info| info.native_pixels_per_point)
                .unwrap_or(1.45);
        }

        self.futures.init(frame);

        // load harmony lotus
        self.harmony_lotus.replace(load_harmony_lotus(ctx));
    }

    /// Saves the current config in memory to disk.
    pub fn save_config(&self) {
        self.local_config.store();

        if let Some(client) = self.client.clone() {
            let conf = self.config.clone();
            self.futures.spawn(async move { conf.store(&client).await });
        }
    }

    /// This must be run each frame to handle stuff like:
    /// - completed future outputs,
    /// - socket events,
    /// - typing notifs,
    /// - etc.
    pub fn maintain(&mut self, ctx: &egui::Context) {
        self.futures.poll();

        self.handle_upload_message();
        self.handle_about();
        self.handle_errors();
        self.handle_sockets();
        self.handle_images(ctx);
        self.handle_config();

        self.cache.maintain(|ev| match ev {
            FetchEvent::Attachment { attachment, file } => {
                if attachment.is_raster_image() {
                    crate::image_cache::op::decode_image(file.data().clone(), attachment.id, attachment.name);
                }
                None
            }
            ev => {
                if let FetchEvent::Harmony(Event::Chat(ChatEvent::SentMessage(message_sent))) = &ev {
                    let Some(message) = message_sent.message.as_ref() else { return Some(ev) };
                    if let Some(text) = message.get_text_content() {
                        if message.author_id != self.client.as_ref().unwrap().user_id() {
                            let triggers_keyword = text
                                .text
                                .split_whitespace()
                                .filter_map(|word| self.config.mention_keywords.iter().find(|keyword| *keyword == word))
                                .next();
                            if let Some(keyword) = triggers_keyword {
                                show_notification(format!("mention keyword '{}' triggered", keyword), &text.text);
                            }
                        }
                    }
                }
                Some(ev)
            }
        });
    }

    /// Get the current client. Will panic if there is none.
    pub fn client(&self) -> &Client {
        self.client.as_ref().expect("client not initialized yet")
    }

    /// Set a screen to be pushed onto the stack in the next frame.
    pub fn push_screen<S: Screen>(&mut self, screen: S) {
        self.next_screen = Some(Box::new(screen));
    }

    /// Sets the state to pop the current screen in the next frame.
    pub fn pop_screen(&mut self) {
        self.prev_screen = true;
    }

    /// Takes a result and runs a closure on it that takes the result's
    /// success value. Will handle bad session and other connection errors,
    /// and automatically push to `latest_errors`.
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

    /// Reset socket state to disconnected.
    pub fn reset_socket_state(&mut self) {
        self.socket_retry_count = 0;
        self.last_socket_retry = None;
        self.is_connected = false;
        self.connecting_socket = false;
        self.reset_socket.set(false);
    }

    /// Returns member display name for a message (ie. respecting overrides).
    ///
    /// It will return "unknown" if there is no override in the message and
    /// the user isn't in the cache.
    pub fn get_member_display_name<'a>(&'a self, msg: &'a Message) -> &'a str {
        let user = self.cache.get_user(msg.sender);
        let overrides = msg.overrides.as_ref();
        let override_name = overrides.and_then(|ov| ov.name.as_ref().map(SmolStr::as_str));
        let sender_name = user.map_or_else(|| "unknown", |u| u.username.as_str());
        override_name.unwrap_or(sender_name)
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
            spawn_client_fut!(self, |client| client.connect_socket().await);
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

    #[inline(always)]
    fn handle_config(&mut self) {
        handle_future!(self, |res: ClientResult<Config>| {
            self.run(res, |state, config| {
                state.config = config;
            });
        });
    }
}
