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

struct MessengerWorker {
    rx: thread_mpsc::Receiver<Instruction>,
    topics_by_id: HashMap<u64, Vec<String>>,
    sender_by_id: HashMap<u64, tokio_mpsc::Sender<Arc<Event>>>,
    ids_by_topic: HashMap<String, HashSet<u64>>,
}

impl MessengerWorker {
    fn new(rx: thread_mpsc::Receiver<Instruction>) -> MessengerWorker {
        MessengerWorker {
            rx: rx,
            topics_by_id: HashMap::new(),
            sender_by_id: HashMap::new(),
            ids_by_topic: HashMap::new(),
        }
    }

    fn subscribe(&mut self, id: u64, sender: tokio_mpsc::Sender<Arc<Event>>, topics: Vec<String>) {
        for topic in topics.iter() {
            if !self.ids_by_topic.contains_key(topic) {
                self.ids_by_topic.insert(topic.clone(), HashSet::new());
            }
            self.ids_by_topic.get_mut(topic).unwrap().insert(id);
        }
        self.topics_by_id.insert(id, topics);
        self.sender_by_id.insert(id, sender);
    }

    fn unsubscribe(&mut self, id: u64) {
        if self.sender_by_id.remove(&id).is_none() {
            // Previously unsubscribed, possibly because of an error condition.
            return;
        }

        let topics = self.topics_by_id.remove(&id).unwrap();
        for topic in topics.iter() {
            let fetched_ids = self.ids_by_topic.get_mut(topic).unwrap();
            fetched_ids.remove(&id);
            if fetched_ids.is_empty() {
                self.ids_by_topic.remove(topic);
            }
        }
    }

    fn deliver(&mut self, topic: String, event: Event) {
        if let Some(receivers) = self.ids_by_topic.get(&topic) {
            let evt_arc = Arc::new(event);
            let mut failures: Vec<u64> = Vec::new();
            for id in receivers {
                let sender = self.sender_by_id.get(id).unwrap();
                match sender.try_send(evt_arc.clone()) {
                    Err(_) => failures.push(*id),
                    Ok(()) => {}
                };
            }
            if !failures.is_empty() {
                for id in failures {
                    self.unsubscribe(id);
                }
            }
        }
    }

    fn run(mut self) {
        loop {
            let msg = self.rx.recv().unwrap();
            match msg {
                Instruction::Message(topic, evt) => {
                    self.deliver(topic, evt);
                }

                Instruction::Subscribe(id, sender, topics) => {
                    self.subscribe(id, sender, topics);
                }

                Instruction::Unsubscribe(id) => {
                    self.unsubscribe(id);
                }

                Instruction::Stop => break,
            }
        }
        println!(
            "Worker end. {:?} {:?} {:?}",
            self.topics_by_id, self.sender_by_id, self.ids_by_topic
        );
    }
}

fn worker() -> thread_mpsc::Sender<Instruction> {
    let (tx, rx) = thread_mpsc::channel();
    thread::spawn(move || {
        let worker = MessengerWorker::new(rx);
        worker.run();
    });
    tx
}
