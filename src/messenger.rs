use rocket::tokio::sync::mpsc as tokio_mpsc;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as thread_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::vec::Vec;

#[derive(Debug)]
pub struct Event {
    pub event: String,
    pub data: String,
}

impl Event {
    pub fn new(event: String, data: String) -> Event {
        Event { event: event, data }
    }
}

#[derive(Debug)]
pub struct Messenger {
    chan: Mutex<thread_mpsc::Sender<Instruction>>,
    id_counter: AtomicU64,
}

#[derive(Debug)]
pub struct MessageReceiver {
    id: u64,
    messenger: thread_mpsc::Sender<Instruction>,
    pub subscription: tokio_mpsc::Receiver<Arc<Event>>, // XXX not pub
}

impl Drop for MessageReceiver {
    fn drop(&mut self) {
        self.messenger
            .send(Instruction::Unsubscribe(self.id))
            .unwrap();
    }
}

#[derive(Debug)]
enum Instruction {
    Stop,
    Subscribe(u64, tokio_mpsc::Sender<Arc<Event>>, Vec<String>),
    Unsubscribe(u64),
    Message(String, Event),
}

impl Messenger {
    pub fn new() -> Messenger {
        let chan = worker();
        Messenger {
            chan: Mutex::new(chan),
            id_counter: AtomicU64::new(0),
        }
    }

    pub fn send(&self, topic: String, evt: Event) {
        self.chan
            .lock()
            .unwrap()
            .send(Instruction::Message(topic, evt))
            .unwrap();
    }

    pub fn subscribe(&self, topics: Vec<String>) -> MessageReceiver {
        let (tx, rx) = tokio_mpsc::channel(16);
        let id = self.id_counter.fetch_add(1, Ordering::Relaxed);
        self.chan
            .lock()
            .unwrap()
            .send(Instruction::Subscribe(id, tx, topics))
            .unwrap();
        MessageReceiver {
            id: id,
            messenger: self.chan.lock().unwrap().clone(),
            subscription: rx,
        }
    }
}

impl Drop for Messenger {
    fn drop(&mut self) {
        self.chan
            .get_mut()
            .unwrap()
            .send(Instruction::Stop)
            .unwrap();
    }
}

fn worker() -> thread_mpsc::Sender<Instruction> {
    let (tx, rx) = thread_mpsc::channel();
    thread::spawn(move || {
        let mut topics_by_id: HashMap<u64, Vec<String>> = HashMap::new();
        let mut sender_by_id: HashMap<u64, tokio_mpsc::Sender<Arc<Event>>> = HashMap::new();
        let mut ids_by_topic: HashMap<String, HashSet<u64>> = HashMap::new();

        loop {
            let msg = rx.recv().unwrap();
            match msg {
                Instruction::Message(topic, evt) => {
                    let evt_arc = Arc::new(evt);
                    if let Some(receivers) = ids_by_topic.get(&topic) {
                        for id in receivers.iter() {
                            let sender = sender_by_id.get(id).unwrap();
                            sender.try_send(evt_arc.clone()).unwrap(); // XXX Handle errors
                        }
                    }
                }

                Instruction::Subscribe(id, sender, topics) => {
                    for topic in topics.iter() {
                        if !ids_by_topic.contains_key(topic) {
                            ids_by_topic.insert(topic.clone(), HashSet::new());
                        }
                        ids_by_topic.get_mut(topic).unwrap().insert(id);
                    }
                    topics_by_id.insert(id, topics);
                    sender_by_id.insert(id, sender);
                }

                Instruction::Unsubscribe(id) => {
                    let topics = topics_by_id.remove(&id).unwrap();
                    for topic in topics.iter() {
                        let fetched_ids = ids_by_topic.get_mut(topic).unwrap();
                        fetched_ids.remove(&id);
                        if fetched_ids.is_empty() {
                            ids_by_topic.remove(topic);
                        }
                    }
                    sender_by_id.remove(&id);
                }

                Instruction::Stop => break,
            }
        }
        println!(
            "Worker end. {:?} {:?} {:?}",
            topics_by_id, sender_by_id, ids_by_topic
        );
    });
    tx
}
