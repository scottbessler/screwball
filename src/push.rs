use std::env;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;
use uuid::Uuid;
use web_push::{
    ContentEncoding, IsahcWebPushClient, PartialVapidSignatureBuilder, SubscriptionInfo, Urgency,
    VapidSignatureBuilder, WebPushClient, WebPushMessageBuilder,
};

use crate::{
    error::AppError,
    models::{Game, GameStatus, SeatKind},
    users::{PushSubscription, UserStore},
};

const LOCAL_DEV_VAPID_PRIVATE_KEY: &str = "GOUXfxqzqhlIXF7mcuoriQnHt7rmodZJQvRK1vD16Bc";

#[derive(Clone)]
pub struct PushService {
    inner: Option<PushServiceInner>,
}

#[derive(Clone)]
struct PushServiceInner {
    vapid: PartialVapidSignatureBuilder,
    public_key: String,
    subject: String,
    client: IsahcWebPushClient,
}

#[derive(Serialize)]
struct TurnNotification<'a> {
    title: &'a str,
    body: String,
    url: String,
    tag: String,
}

impl PushService {
    pub fn from_env() -> Result<Self, AppError> {
        let private_key = match env::var("VAPID_PRIVATE_KEY") {
            Ok(private_key) if !private_key.trim().is_empty() => private_key,
            Ok(_) => {
                return Err(AppError::internal(
                    "VAPID_PRIVATE_KEY is set but empty; unset it or provide a key",
                ));
            }
            Err(_) if cfg!(debug_assertions) => LOCAL_DEV_VAPID_PRIVATE_KEY.to_string(),
            Err(_) => return Ok(Self::disabled()),
        };

        let subject =
            env::var("VAPID_SUBJECT").unwrap_or_else(|_| "mailto:admin@example.com".to_string());
        Self::from_private_key(&private_key, &subject)
    }

    pub fn from_private_key(private_key: &str, subject: &str) -> Result<Self, AppError> {
        let vapid = VapidSignatureBuilder::from_base64_no_sub(private_key)
            .map_err(|err| AppError::internal(format!("invalid VAPID_PRIVATE_KEY: {err}")))?;
        let public_key = URL_SAFE_NO_PAD.encode(vapid.get_public_key());
        let client = IsahcWebPushClient::new().map_err(AppError::internal)?;

        Ok(Self {
            inner: Some(PushServiceInner {
                vapid,
                public_key,
                subject: subject.to_string(),
                client,
            }),
        })
    }

    pub fn disabled() -> Self {
        Self { inner: None }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    pub fn public_key(&self) -> Option<&str> {
        self.inner.as_ref().map(|inner| inner.public_key.as_str())
    }

    pub async fn notify_turn(&self, users: &UserStore, game: &Game) {
        let Some(inner) = &self.inner else {
            return;
        };
        let Some((user_id, player_name)) = current_human_turn(game) else {
            return;
        };
        let Some(user) = users.get(user_id).await else {
            return;
        };
        if user.push_subscriptions.is_empty() {
            return;
        }

        let body = last_move_player(game)
            .filter(|name| name != &player_name)
            .map(|name| format!("{name} just played. It is your turn."))
            .unwrap_or_else(|| "It is your turn in Screwball.".to_string());
        let notification = TurnNotification {
            title: "Your turn!",
            body,
            url: format!("/games/{}", game.id),
            tag: format!("turn-{}", game.id),
        };
        let payload = match serde_json::to_vec(&notification) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::warn!(error = %err, game_id = %game.id, "could not serialize push payload");
                return;
            }
        };

        for subscription in user.push_subscriptions {
            if let Err(err) = send_one(inner, &subscription, game.id, &payload).await {
                tracing::warn!(
                    error = %err,
                    game_id = %game.id,
                    user_id = %user_id,
                    endpoint = %subscription.endpoint,
                    "web push send failed"
                );
            }
        }
    }
}

async fn send_one(
    inner: &PushServiceInner,
    subscription: &PushSubscription,
    game_id: Uuid,
    payload: &[u8],
) -> Result<(), web_push::WebPushError> {
    let subscription_info = SubscriptionInfo::new(
        subscription.endpoint.clone(),
        subscription.keys.p256dh.clone(),
        subscription.keys.auth.clone(),
    );
    let mut signature_builder = inner.vapid.clone().add_sub_info(&subscription_info);
    signature_builder.add_claim("sub", inner.subject.clone());

    let mut message = WebPushMessageBuilder::new(&subscription_info);
    message.set_payload(ContentEncoding::Aes128Gcm, payload);
    message.set_ttl(86_400);
    message.set_urgency(Urgency::High);
    message.set_topic(game_id.simple().to_string());
    message.set_vapid_signature(signature_builder.build()?);
    inner.client.send(message.build()?).await
}

fn current_human_turn(game: &Game) -> Option<(Uuid, String)> {
    if game.status != GameStatus::Active {
        return None;
    }
    let seat = game.seats.get(game.turn)?;
    match seat.kind {
        SeatKind::Human {
            user_id: Some(user),
        } => Some((user, seat.name.clone())),
        SeatKind::Human { user_id: None } | SeatKind::Bot { .. } => None,
    }
}

fn last_move_player(game: &Game) -> Option<String> {
    let last = game.moves.last()?;
    game.seats.get(last.seat).map(|seat| seat.name.clone())
}
