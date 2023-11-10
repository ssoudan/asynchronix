//! Example: Dynamic dispatching.
//!
//! This example demonstrates in particular:
//!
//! * Scheduler::send_event to send an event to a [`Model`] by [`Address`].
//! * Scheduler::schedule_event_by_address to schedule an event for a [`Model`] by [`Address`].
//!
use std::time::Duration;

use asynchronix::model::{Model, Output, Requestor};
use asynchronix::simulation::{Address, Mailbox, SimInit};
use asynchronix::time::{MonotonicTime, Scheduler};

/// Repeatedly ask the [`Dispatcher`] for a [`Receiver`] and send it a message now and later.
struct Emitter {
    ///
    pub id: String,
    ///
    pub subscribe: Requestor<Subscribe, Address<Receiver>>,
    ///
    pub delay: Duration,
}

impl Emitter {
    fn new(id: String, delay: Duration) -> Self {
        Self {
            id,
            subscribe: Default::default(),
            delay,
        }
    }

    async fn subscribe_now(&mut self, (): (), scheduler: &Scheduler<Self>) {
        for a in self.subscribe.send(Subscribe(2)).await {
            // deal directly with the receiver now
            scheduler
                .send_event(Receiver::receive, self.id.clone(), a.clone())
                .await;

            // deal with the receiver later
            let deadline = scheduler.time() + 2 * self.delay;
            scheduler
                .schedule_event(deadline, Emitter::unsubscribe_later, a.clone())
                .expect("failed to schedule event");

            let another_deadline = scheduler.time() + self.delay;
            scheduler
                .schedule_event_by_address(
                    another_deadline,
                    Receiver::something,
                    self.id.clone(),
                    a,
                )
                .expect("failed to schedule event");
        }
    }

    async fn unsubscribe_later(&mut self, a: Address<Receiver>, scheduler: &Scheduler<Self>) {
        scheduler
            .send_event(Receiver::unsubscribe, self.id.clone(), a)
            .await;
    }
}

impl Model for Emitter {}

struct Receiver {
    id: String,
    output: Output<String>,
    register: Output<Address<Self>>,
}

impl Receiver {
    ///
    fn new(id: String) -> Self {
        Self {
            id,
            output: Default::default(),
            register: Default::default(),
        }
    }
    ///
    async fn receive(&mut self, id: String) {
        self.output
            .send(format!("[{}] receive: {}", self.id, id))
            .await;
    }

    async fn unsubscribe(&mut self, id: String) {
        self.output
            .send(format!("[{}] unsubscribe: {}", self.id, id))
            .await;
    }

    async fn something(&mut self, id: String) {
        self.output
            .send(format!("[{}] something: {}", self.id, id))
            .await;
    }

    async fn register(&mut self, (): (), scheduler: &Scheduler<Self>) {
        self.register.send(scheduler.address()).await;
    }
}

#[derive(Clone)]
struct Subscribe(u64);

impl Model for Receiver {}

/// A [`Dispatcher`] is a [`Model`] that keeps track of a list of [`Receiver`]s and dispatches them
struct Dispatcher {
    receivers: Vec<Address<Receiver>>,
    last: usize,
}

impl Dispatcher {
    fn new() -> Self {
        Self {
            receivers: Default::default(),
            last: 0,
        }
    }

    fn register_receiver(&mut self, a: Address<Receiver>) {
        self.receivers.push(a);
    }

    async fn dispatch(&mut self, _s: Subscribe, _scheduler: &Scheduler<Self>) -> Address<Receiver> {
        let a = &self.receivers[self.last];
        self.last = (self.last + 1) % self.receivers.len();

        a.clone()
    }

    async fn dynamic_registeration(&mut self, a: Address<Receiver>) {
        self.register_receiver(a);
    }
}

impl Model for Dispatcher {}

fn main() {
    // ---------------
    // Bench assembly.
    // ---------------
    const DELAY: Duration = Duration::from_secs(10);
    // Models.
    let mut a = Emitter::new("a".to_string(), DELAY);

    let mut dispatcher = Dispatcher::new();

    let mut r_1 = Receiver::new("r1".to_string());
    let mut r_2 = Receiver::new("r2".to_string());
    let mut r_3 = Receiver::new("r3".to_string());

    // Mailboxes.
    let a_mbox = Mailbox::new();

    let d_mbox = Mailbox::new();

    let r_1_mbox = Mailbox::new();
    let r_2_mbox = Mailbox::new();
    let r_3_mbox = Mailbox::new();

    dispatcher.register_receiver(r_1_mbox.address());
    dispatcher.register_receiver(r_2_mbox.address());
    dispatcher.register_receiver(r_3_mbox.address());

    // Connections.
    a.subscribe.connect(Dispatcher::dispatch, &d_mbox);
    r_3.register
        .connect(Dispatcher::dynamic_registeration, &d_mbox);

    // Model handles for simulation.
    let a_addr = a_mbox.address();
    let r_3_addr = r_3_mbox.address();
    let mut r_1_output = r_1.output.connect_slot().0;
    let mut r_2_output = r_2.output.connect_slot().0;
    let mut r_3_output = r_3.output.connect_slot().0;

    // Start time (arbitrary since models do not depend on absolute time).
    let t0 = MonotonicTime::EPOCH;

    // Assembly and initialization.
    let mut simu = SimInit::new()
        .add_model(a, a_mbox)
        .add_model(dispatcher, d_mbox)
        .add_model(r_1, r_1_mbox)
        .add_model(r_2, r_2_mbox)
        .add_model(r_3, r_3_mbox)
        .init(t0);

    // ----------
    // Simulation.
    // ----------

    // Check initial conditions.
    let mut t = t0;
    assert_eq!(simu.time(), t);

    ///////////////
    simu.send_event(Emitter::subscribe_now, (), &a_addr);
    assert_eq!(r_1_output.take(), Some("[r1] receive: a".to_string()));

    simu.step();
    t += DELAY;

    assert_eq!(simu.time(), t);
    assert_eq!(r_1_output.take(), Some("[r1] something: a".to_string()));

    simu.step();
    t += DELAY;

    assert_eq!(simu.time(), t);
    assert_eq!(r_1_output.take(), Some("[r1] unsubscribe: a".to_string()));

    ///////////////
    simu.send_event(Emitter::subscribe_now, (), &a_addr);
    assert_eq!(r_2_output.take(), Some("[r2] receive: a".to_string()));

    simu.step();
    t += DELAY;

    assert_eq!(simu.time(), t);
    assert_eq!(r_2_output.take(), Some("[r2] something: a".to_string()));

    simu.step();
    t += DELAY;

    assert_eq!(simu.time(), t);
    assert_eq!(r_2_output.take(), Some("[r2] unsubscribe: a".to_string()));

    ///////////////
    // get r3 registered
    simu.send_event(Receiver::register, (), &r_3_addr);

    ///////////////
    simu.send_event(Emitter::subscribe_now, (), &a_addr);
    assert_eq!(r_3_output.take(), Some("[r3] receive: a".to_string()));

    simu.step();
    t += DELAY;

    assert_eq!(simu.time(), t);
    assert_eq!(r_3_output.take(), Some("[r3] something: a".to_string()));

    simu.step();
    t += DELAY;

    assert_eq!(simu.time(), t);
    assert_eq!(r_3_output.take(), Some("[r3] unsubscribe: a".to_string()));
}
