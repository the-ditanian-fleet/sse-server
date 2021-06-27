#[macro_use]
extern crate rocket;

mod cors;
mod messenger;

use rocket::response::stream::{Event, EventStream};
use rocket::tokio::select;
use rocket::{Shutdown, State};

use branca::decode as branca_decode;
use serde::Deserialize;

use std::borrow::Cow;
use std::vec::Vec;

struct AppConfig {
    key: Vec<u8>,
    signature_lifetime: u32,
}

#[derive(Debug, Deserialize)]
struct SubscribeRequest {
    topics: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Message {
    topic: String,
    event: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct Submission {
    events: Vec<Message>,
}

#[get("/events?<token>")]
async fn events(
    token: String,
    messager: &State<messenger::Messenger>,
    config: &State<AppConfig>,
    mut end: Shutdown,
) -> EventStream![] {
    let decoded = branca_decode(&token, &config.key, config.signature_lifetime).unwrap();
    let request: SubscribeRequest = rmp_serde::from_read_ref(&decoded).unwrap();

    let mut receiver = messager.subscribe(request.topics);
    EventStream! {
        loop {
            select! {
                msg = receiver.subscription.recv() => match msg {
                    Some(evt) => {
                        yield Event::data(evt.data.clone()).event(Cow::from(evt.event.clone()));
                    },
                    None => break
                },
                _ = &mut end => break,
            };
        }
    }
}

#[post("/submit", data = "<encoded>")]
fn submit(encoded: String, messager: &State<messenger::Messenger>, config: &State<AppConfig>) {
    let decoded = branca_decode(&encoded, &config.key, config.signature_lifetime).unwrap();
    let request: Submission = rmp_serde::from_read_ref(&decoded).unwrap();
    for message in request.events {
        messager.send(
            message.topic,
            messenger::Event::new(message.event, message.data),
        );
    }
}

#[get("/ping")]
fn ping() {}

#[launch]
async fn rocket() -> _ {
    let env_key = std::env::var("SSE_SECRET");
    if let Err(e) = env_key {
        println!("{}", e);
        panic!("Missing SSE_SECRET");
    }

    let decoded_key = hex::decode(env_key.unwrap()).unwrap();
    if decoded_key.len() != 32 {
        panic!("SSE_SECRET must be a hex-encoded 32-byte secret");
    }

    rocket::build()
        .manage(messenger::Messenger::new())
        .manage(AppConfig {
            key: decoded_key,
            signature_lifetime: 86400,
        })
        .attach(cors::CORS)
        .mount("/", routes![submit, events, ping])
}
