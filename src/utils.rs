use std::{array::IntoIter, borrow::Cow, future::Future};

use client::{
    harmony_rust_sdk::api::{
        chat::{
            stream_event::{Event as ChatEvent, GuildUpdated, PermissionUpdated},
            Event, GetGuildRequest, QueryHasPermissionRequest,
        },
        emote::stream_event::Event as EmoteEvent,
        emote::{EmotePackEmotesUpdated, GetEmotePackEmotesRequest},
        profile::{stream_event::Event as ProfileEvent, GetProfileRequest, ProfileUpdated},
    },
    tracing, Client, PostProcessEvent,
};
use eframe::egui::{Key, Response, Ui};

pub use anyhow::{anyhow, bail, ensure, Error};
pub use client::error::{ClientError, ClientResult};
pub(crate) use futures::{handle_future, spawn_future};

pub fn truncate_string(value: &str, new_len: usize) -> Cow<'_, str> {
    if value.chars().count() > new_len {
        let mut value = value.to_string();
        value.truncate(value.chars().take(new_len).map(char::len_utf8).sum());
        value.push('…');
        Cow::Owned(value)
    } else {
        Cow::Borrowed(value)
    }
}

pub trait TextInputExt {
    fn did_submit(&self, ui: &Ui) -> bool;
}

impl TextInputExt for Response {
    fn did_submit(&self, ui: &Ui) -> bool {
        self.lost_focus() && ui.input().key_pressed(Key::Enter)
    }
}

#[allow(clippy::too_many_lines)]
pub fn post_process_events(
    client: &Client,
    posts: Vec<PostProcessEvent>,
) -> impl Future<Output = ClientResult<Vec<Event>>> + 'static {
    let inner = client.inner_arc();

    async move {
        let mut events = Vec::new();
        for post in posts {
            match post {
                PostProcessEvent::SendNotification { content, title, .. } => {
                    let res = notify_rust::Notification::new()
                        .summary(&title)
                        .body(&truncate_string(&content, 50))
                        .auto_icon()
                        .show();
                    if let Err(err) = res {
                        tracing::debug!("failed to send notif: {}", err);
                    }
                }
                PostProcessEvent::CheckPermsForChannel(guild_id, channel_id) => {
                    let perm_queries = ["channels.manage.change-information", "messages.send"];
                    let queries = IntoIter::new(perm_queries)
                        .map(|query| {
                            QueryHasPermissionRequest::new(guild_id, Some(channel_id), None, query.to_string())
                        })
                        .collect();
                    events.extend(
                        inner
                            .batch_call(queries)
                            .await?
                            .into_iter()
                            .zip(IntoIter::new(perm_queries))
                            .map(|(resp, query)| {
                                Event::Chat(ChatEvent::PermissionUpdated(PermissionUpdated {
                                    guild_id,
                                    channel_id: Some(channel_id),
                                    ok: resp.ok,
                                    query: query.to_string(),
                                }))
                            }),
                    );
                }
                PostProcessEvent::FetchThumbnail(id) => todo!(),
                PostProcessEvent::FetchProfile(user_id) => {
                    events.push(inner.call(GetProfileRequest::new(user_id)).await.map(|resp| {
                        let profile = resp.profile.unwrap_or_default();
                        Event::Profile(ProfileEvent::ProfileUpdated(ProfileUpdated {
                            user_id,
                            new_avatar: profile.user_avatar,
                            new_status: Some(profile.user_status),
                            new_username: Some(profile.user_name),
                            new_is_bot: Some(profile.is_bot),
                        }))
                    })?);
                }
                PostProcessEvent::GoToFirstMsgOnChannel(channel_id) => {
                    //todo!()
                }
                PostProcessEvent::FetchGuildData(guild_id) => {
                    events.push(inner.call(GetGuildRequest::new(guild_id)).await.map(|resp| {
                        let guild = resp.guild.unwrap_or_default();
                        Event::Chat(ChatEvent::EditedGuild(GuildUpdated {
                            guild_id,
                            new_metadata: guild.metadata,
                            new_name: Some(guild.name),
                            new_picture: guild.picture,
                        }))
                    })?);
                    let perm_queries = [
                        "guild.manage.change-information",
                        "user.manage.kick",
                        "user.manage.ban",
                        "user.manage.unban",
                        "invites.manage.create",
                        "invites.manage.delete",
                        "invites.view",
                        "channels.manage.move",
                        "channels.manage.create",
                        "channels.manage.delete",
                        "roles.manage",
                        "roles.get",
                        "roles.user.manage",
                        "roles.user.get",
                        "permissions.manage.set",
                        "permissions.manage.get",
                    ];
                    events.reserve(perm_queries.len());
                    let queries = IntoIter::new(perm_queries)
                        .map(|query| QueryHasPermissionRequest::new(guild_id, None, None, query.to_string()))
                        .collect();
                    let perms = inner.batch_call(queries).await.map(|response| {
                        response
                            .into_iter()
                            .zip(IntoIter::new(perm_queries))
                            .map(|(resp, query)| {
                                Event::Chat(ChatEvent::PermissionUpdated(PermissionUpdated {
                                    guild_id,
                                    channel_id: None,
                                    ok: resp.ok,
                                    query: query.to_string(),
                                }))
                            })
                    })?;
                    events.extend(perms);
                }
                PostProcessEvent::FetchMessage {
                    guild_id,
                    channel_id,
                    message_id,
                } => {}
                PostProcessEvent::FetchLinkMetadata(url) => {}
                PostProcessEvent::FetchEmotes(pack_id) => {
                    events.push(inner.call(GetEmotePackEmotesRequest { pack_id }).await.map(|resp| {
                        Event::Emote(EmoteEvent::EmotePackEmotesUpdated(EmotePackEmotesUpdated {
                            pack_id,
                            added_emotes: resp.emotes,
                            deleted_emotes: Vec::new(),
                        }))
                    })?);
                }
            }
        }

        Ok(events)
    }
}

pub mod futures {
    use client::tracing;
    use eframe::epi::RepaintSignal;
    use std::{
        any::{Any, TypeId},
        collections::HashMap,
        future::Future,
        hash::{BuildHasherDefault, Hasher},
        sync::Arc,
    };
    use tokio::sync::oneshot;

    #[derive(Default)]
    struct IdHasher(u64);

    impl Hasher for IdHasher {
        fn write(&mut self, _: &[u8]) {
            unreachable!("TypeId calls write_u64");
        }

        #[inline]
        fn write_u64(&mut self, id: u64) {
            self.0 = id;
        }

        #[inline]
        fn finish(&self) -> u64 {
            self.0
        }
    }

    type FutureMap = HashMap<TypeId, Vec<oneshot::Receiver<AnyItem>>, BuildHasherDefault<IdHasher>>;

    type AnyItem = Box<dyn Any + Send + 'static>;

    pub enum FutureProgress<T> {
        NotFound,
        Cancelled,
        InProgress,
        Done(T),
    }

    impl<T> FutureProgress<T> {
        #[inline]
        pub fn is_done(&self) -> bool {
            matches!(self, FutureProgress::Done(_))
        }

        #[inline]
        pub fn is_cancelled(&self) -> bool {
            matches!(self, FutureProgress::Cancelled)
        }

        #[inline]
        pub fn is_in_progress(&self) -> bool {
            matches!(self, FutureProgress::InProgress)
        }

        #[inline]
        pub fn extract(self) -> T {
            match self {
                FutureProgress::Done(value) => value,
                _ => panic!("tried to extract future value but its not done yet"),
            }
        }
    }

    #[derive(Default)]
    pub struct Futures {
        inner: FutureMap,
        rr: Option<Arc<dyn RepaintSignal>>,
    }

    impl Futures {
        pub fn init(&mut self, frame: &eframe::epi::Frame) {
            self.rr = Some(frame.repaint_signal());
        }

        pub fn spawn<Id, Fut, Out>(&mut self, fut: Fut)
        where
            Fut: Future<Output = Out> + Send + 'static,
            Out: Send + 'static,
            Id: 'static,
        {
            let (tx, rx) = oneshot::channel::<AnyItem>();

            let rr = self.rr.clone().expect("futures not initialized yet -- this is a bug");
            tokio::spawn(async move {
                let result = fut.await;
                let item = Box::new(result);
                if tx.send(item).is_err() {
                    tracing::debug!("future output dropped before result was sent");
                }
                rr.request_repaint();
            });

            self.inner.entry(TypeId::of::<Id>()).or_default().push(rx);
        }

        pub fn get<Id, T>(&mut self) -> FutureProgress<T>
        where
            T: 'static,
            Id: 'static,
        {
            let id = TypeId::of::<Id>();

            if let Some(rxs) = self.inner.get_mut(&id) {
                let progress = if let Some(rx) = rxs.first_mut() {
                    let res = rx.try_recv();
                    match res {
                        Ok(item) => FutureProgress::Done(*item.downcast::<T>().expect("value is not of type T")),
                        Err(oneshot::error::TryRecvError::Empty) => FutureProgress::InProgress,
                        Err(oneshot::error::TryRecvError::Closed) => FutureProgress::Cancelled,
                    }
                } else {
                    FutureProgress::NotFound
                };

                if progress.is_done() || progress.is_cancelled() {
                    rxs.remove(0);
                }

                progress
            } else {
                FutureProgress::NotFound
            }
        }
    }

    macro_rules! spawn_future {
        ($state:ident, $id:ty, $fut:expr) => {
            $state.futures.borrow_mut().spawn::<$id, _, _>($fut);
        };
    }

    macro_rules! handle_future {
        ($state:ident, $id:ty, |$val:ident: $val_ty:ty| $handler:expr) => {
            if let $crate::utils::futures::FutureProgress::Done($val) = {
                let mut brw = $state.futures.borrow_mut();
                let progress = brw.get::<$id, $val_ty>();
                drop(brw);
                progress
            } {
                $handler
            }
        };
    }

    pub(crate) use handle_future;
    pub(crate) use spawn_future;
}
