use std::collections::VecDeque;

#[macro_use]
mod events;
mod allocator;
mod api;
mod boss;
mod code;
mod input;
pub mod key;
mod lister;
mod mailbox;
mod nameplate;
mod order;
mod receive;
mod rendezvous;
mod send;
mod server_messages;
mod terminator;
#[cfg(test)]
mod test;
mod timing;
mod transfer;
mod util;
mod wordlist;
pub mod io;

pub use self::events::{AppID, Code};
use self::events::{Event, Events, MySide, Nameplate};
use log::trace;

pub use self::api::{
    APIAction, APIEvent, IOAction, IOEvent, InputHelperError, Mood,
    TimerHandle, WSHandle,
};
pub use self::transfer::{
    Abilities, AnswerType,
    DirectType, Hints, OfferType, PeerMessage, RelayType, TransitType,
    TransitAck,
};

pub struct WormholeCore {
    allocator: allocator::AllocatorMachine,
    boss: boss::BossMachine,
    code: code::CodeMachine,
    input: input::InputMachine,
    key: key::KeyMachine,
    lister: lister::ListerMachine,
    mailbox: mailbox::MailboxMachine,
    nameplate: nameplate::NameplateMachine,
    order: order::OrderMachine,
    receive: receive::ReceiveMachine,
    rendezvous: rendezvous::RendezvousMachine,
    send: send::SendMachine,
    terminator: terminator::TerminatorMachine,
    timing: timing::Timing,
    io: io::WormholeIO,
}

// I don't know how to write this
/*fn to_results<Vec<T>>(from: Vec<T>) -> Vec<Result> {
    from.into_iter().map(|r| Result::from(r)).collect::<Vec<Result>>()
}*/

impl WormholeCore {
    pub fn new<T>(appid: T, relay_url: &str, io: io::WormholeIO) -> Self
    where
        T: Into<AppID>,
    {
        // TODO wrap AppID in Arc
        let appid: AppID = appid.into();
        let side = MySide::generate();
        WormholeCore {
            allocator: allocator::AllocatorMachine::new(),
            boss: boss::BossMachine::new(),
            code: code::CodeMachine::new(),
            input: input::InputMachine::new(),
            key: key::KeyMachine::new(&appid, &side),
            lister: lister::ListerMachine::new(),
            mailbox: mailbox::MailboxMachine::new(&side),
            nameplate: nameplate::NameplateMachine::new(),
            order: order::OrderMachine::new(),
            receive: receive::ReceiveMachine::new(),
            rendezvous: rendezvous::RendezvousMachine::new(
                &appid, relay_url, &side, 5.0,
            ),
            send: send::SendMachine::new(&side),
            terminator: terminator::TerminatorMachine::new(),
            timing: timing::Timing::new(),
            io,
        }
    }

    // the IO layer must either call start() or do_api(APIEvent::Start), and
    // must act upon all the Actions it gets back
    #[must_use = "You must execute these actions to make things work"]
    pub fn start(&mut self) -> Vec<APIAction> {
        self.do_api(APIEvent::Start)
    }

    #[must_use = "You must execute these actions to make things work"]
    pub fn do_api(&mut self, event: APIEvent) -> Vec<APIAction> {
        // run with RUST_LOG=magic_wormhole=trace to see these
        trace!("  api: {:?}", event);
        let events = self.boss.process_api(event);
        self._execute(events)
    }

    #[must_use = "You must execute these actions to make things work"]
    pub fn do_io(&mut self, event: IOEvent) -> Vec<APIAction> {
        trace!("   io: {:?}", event);
        let events = self.rendezvous.process_io(event);
        self._execute(events)
    }

    pub fn derive_key(&mut self, _purpose: &str, _length: u8) -> Vec<u8> {
        // TODO: only valid after GotVerifiedKey, but should return
        // synchronously. Maybe the Core should expose the conversion
        // function (which requires the key as input) and let the IO glue
        // layer decide how to manage the synchronization?
        panic!("not implemented");
    }

    pub fn input_helper_get_nameplate_completions(
        &mut self,
        prefix: &str,
    ) -> Result<Vec<String>, InputHelperError> {
        self.input.get_nameplate_completions(prefix)
    }

    pub fn input_helper_get_word_completions(
        &mut self,
        prefix: &str,
    ) -> Result<Vec<String>, InputHelperError> {
        self.input.get_word_completions(prefix)
    }

    // TODO: remove this, the helper should remember whether it's called
    // choose_nameplate yet or not instead of asking the core
    pub fn input_helper_committed_nameplate(&self) -> Option<Nameplate> {
        self.input.committed_nameplate()
    }

    fn _execute(&mut self, events: Events) -> Vec<APIAction> {
        let mut action_queue: Vec<APIAction> = Vec::new(); // returned
        let mut event_queue: VecDeque<Event> = VecDeque::new();

        event_queue.append(&mut VecDeque::from(events.events));

        while let Some(e) = event_queue.pop_front() {
            trace!("event: {:?}", e);
            use self::events::Event::*; // machine names
            let actions: Events = match e {
                API(a) => {
                    action_queue.push(a);
                    events![]
                },
                IO(a) => {self.io.process(a); events![]},
                Allocator(e) => self.allocator.process(e),
                Boss(e) => self.boss.process(e),
                Code(e) => self.code.process(e),
                Input(e) => self.input.process(e),
                Key(e) => self.key.process(e),
                Lister(e) => self.lister.process(e),
                Mailbox(e) => self.mailbox.process(e),
                Nameplate(e) => self.nameplate.process(e),
                Order(e) => self.order.process(e),
                Receive(e) => self.receive.process(e),
                Rendezvous(e) => self.rendezvous.process(e),
                Send(e) => self.send.process(e),
                Terminator(e) => self.terminator.process(e),
                Timing(_) => events![], // TODO: unimplemented
            };

            for a in actions.events {
                // TODO use iter
                // TODO: insert in front of queue: depth-first processing
                trace!("  out: {:?}", a);
                match a {
                    Timing(e) => self.timing.add(e),
                    _ => event_queue.push_back(a),
                }
            }
        }
        action_queue
    }
}

// TODO: is there a generic way (e.g. impl From) to convert a Vec<A> into
// Vec<B> when we've got an A->B convertor?
