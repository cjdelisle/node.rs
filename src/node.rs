#![allow(dead_code)]

extern crate mio;
extern crate mio_extras;

use callback::*;

use std::cell::RefMut;
use std::cell::RefCell;
use std::rc::{ Rc, Weak };
use std::fmt;
use std::io;
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::time::{ Duration, SystemTime, UNIX_EPOCH };
use std::cmp::Ordering;
use std::ops::{ Deref, DerefMut };

macro_rules! debug {
    ($fmt:expr $(,$x:expr)* ) => {
        //println!(concat!("DEBUG {}:{} ", $fmt), file!(), line!() $(,$x)* );
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Event loop core
///////////////////////////////////////////////////////////////////////////////////////////////////

struct TimerCb {
    interval: bool,
    cb: Callback<()>,
    millis: u64,
    id: mio::Token
}
impl fmt::Debug for TimerCb {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{ interval: {}, millis: {}, id: {:?} }}", self.interval, self.millis, self.id)
    }
}

struct EventHandler {
    handler: Callback<()>,
    token: mio::Token,
    ev: Rc<mio::Evented>
}
impl fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{ handler: Fn(?)->?, token: {:?}, ev: <Object> }}", self.token)
    }
}

#[derive(Debug)]
struct CorePvt {
    handlers: HashMap<mio::Token, EventHandler>,
    poll: mio::Poll,
    next_timeouts: BTreeMap<Duration, Vec<TimerCb>>,
    event_count: usize,
    next_token: usize,
    now: Duration
}

impl CorePvt {
    fn _do_timeouts(&mut self, tos: &mut Vec<Callback<()>>) -> Option<Duration>
    {
        let mut dur = None;
        let mut remove = Vec::new();
        self.now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Err(_e) => {
                debug!("Failed to get system time");
                return dur;
            }
            Ok(x) => x
        };
        let now = self.now;
        for (time, _to) in self.next_timeouts.iter() {
            match time.cmp(&now) {
                Ordering::Equal | Ordering::Less => {
                    remove.push(time.clone());
                }
                Ordering::Greater => {
                    dur = Some(time.clone() - self.now);
                    break;
                }
            }
        }
        for time in remove {
            let x = self.next_timeouts.remove(&time);
            for xx in x {
                for el in xx {
                    if el.interval {
                        tos.push(el.cb.clone());
                        let d = now + Duration::from_millis(el.millis);
                        self._schedule_timeout(el, d);
                    } else {
                        tos.push(el.cb);
                    }
                }
            }
        }
        dur
    }
    fn _schedule_timeout(&mut self, t: TimerCb, when: Duration)
    {
        match {match self.next_timeouts.get_mut(&when) {
            Some(x) => { x.push(t); None },
            None => Some(t)
        }} {
            Some(x) => { self.next_timeouts.insert(when, vec![x]); }
            None => ()
        }
    }

    //////////////////////////

    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: Callback<()>,
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented, E: 'static
    {
        let token = mio::Token(self.next_token);
        let eh = EventHandler { handler, token, ev: ev.clone() };
        self.handlers.insert(token, eh);
        self.next_token += 1;
        self.poll.register(&*ev, token, ready, pollopt)?;
        self.event_count += 1;
        Ok(token)
    }

    fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool> {
        match self.handlers.remove(token) {
            Some(handler) => {
                match self.poll.deregister(&*handler.ev) {
                    Err(e) => {
                        self.handlers.insert(token.clone(), handler);
                        Err(e)
                    },
                    Ok(_) => {
                        self.event_count -= 1;
                        Ok(true)
                    }
                }
            },
            None => {
                // No event, try it as a timeout
                let mut remove_time = None;
                for (time, tos) in self.next_timeouts.iter_mut() {
                    match {
                        match tos.iter().enumerate().find(|&x| x.1.id == *token) {
                            Some((i, _elem)) => Some(i),
                            None => None
                        }
                    } {
                        Some(i) => {
                            tos.remove(i);
                            if tos.is_empty() { remove_time = Some(time.clone()); }
                            break;
                        }
                        None => ()
                    };
                }
                match remove_time {
                    Some(rt) => {
                        self.next_timeouts.remove(&rt);
                        Ok(true)
                    },
                    None => Ok(false)
                }
            }
        }
    }

    fn set_timeout(&mut self, cb: Callback<()>, millis: u64, interval: bool) -> mio::Token {
        let id = mio::Token(self.next_token);
        self.next_token += 1;
        let tcb = TimerCb { cb: cb, interval, millis, id: id.clone() };
        debug!("_set_timeout in {:?}", Duration::from_millis(millis));
        let d = self.now + Duration::from_millis(millis);
        self._schedule_timeout(tcb, d);
        id
    }
}

#[derive(Debug,Clone)]
pub struct Core {
    wp: Rc<RefCell<CorePvt>>
}
impl Core {
    pub fn register_event<E>(
        &self,
        ev: Rc<E>,
        handler: Callback<()>,
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented, E: 'static
    {
        self.wp.borrow_mut().register_event(ev, handler, ready, pollopt)
    }
    pub fn deregister_event(&self, token: &mio::Token) -> io::Result<bool> {
        self.wp.borrow_mut().deregister_event(token)
    }
    pub fn set_timeout(&self, cb: Callback<()>, millis: u64, interval: bool) -> mio::Token {
        self.wp.borrow_mut().set_timeout(cb, millis, interval)
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Scope
///////////////////////////////////////////////////////////////////////////////////////////////////

fn exec<X,Y>(w:&Core, x:X, f:fn(&mut Scope<X>)->Y) -> Y {
    let s = Rc::new(RefCell::new(Scope {
        a: x,
        s: Weak::new(),
        w: w.clone()
    }));
    s.borrow_mut().s = Rc::downgrade(&s);
    let x = &mut *s.borrow_mut();
    f(x)
}


pub struct Scope<A> where A: 'static {
    a: A,
    s: Weak<RefCell<Scope<A>>>,
    w: Core
}
pub struct SubScope<P,A> where P: Loop<P>, A: 'static, P: 'static {
    p: Rc<RefCell<P>>,
    a: A,
    s: Weak<RefCell<SubScope<P,A>>>,
    w: Core
}

impl<P,A> SubScope<P,A> where P: Loop<P> {
    pub fn p(&mut self) -> RefMut<P> {
        self.p.borrow_mut()
    }
}

impl<P,A> Deref for SubScope<P,A> where P: Loop<P> {
    type Target = A;
    fn deref(&self) -> &A { &self.a }
}
impl<P,A> DerefMut for SubScope<P,A> where P: Loop<P> {
    fn deref_mut(&mut self) -> &mut A { &mut self.a }
}
impl<A> Deref for Scope<A> {
    type Target = A;
    fn deref(&self) -> &A { &self.a }
}
impl<A> DerefMut for Scope<A> {
    fn deref_mut(&mut self) -> &mut A { &mut self.a }
}

pub trait Loop<A> where A: 'static + Loop<A>, Self: 'static {
    fn module(&self) -> ModuleCfg { module().with_loop(Some(self.core().clone())) }
    fn with_scope<X>(&self, x:X, f:fn(&mut SubScope<A,X>)) {
        let ss = Rc::new(RefCell::new(SubScope {
            p: self.as_rc(),
            a: x,
            s: Weak::new(),
            w: self.core().clone()
        }));
        ss.borrow_mut().s = Rc::downgrade(&ss);
        self.core().set_timeout(Box::new(CallbackS0 { a: ss, f: f }), 0, false);
    }
    fn core(&self) -> &Core;
    fn as_rc(&self) -> Rc<RefCell<A>>;
}
impl<P,A> Loop<SubScope<P,A>> for SubScope<P,A> where P: Loop<P> {
    fn core(&self) -> &Core { &self.w }
    fn as_rc(&self) -> Rc<RefCell<Self>> { self.s.upgrade().unwrap() }
}
impl<A> Loop<Scope<A>> for Scope<A> {
    fn core(&self) -> &Core { &self.w }
    fn as_rc(&self) -> Rc<RefCell<Self>> { self.s.upgrade().unwrap() }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Loop runner
///////////////////////////////////////////////////////////////////////////////////////////////////

fn _get_events(_w: &Core, handlers: &mut Vec<Callback<()>>, events: &mut mio::Events) -> bool
{
    handlers.clear();
    let mut w = _w.wp.borrow_mut();
    let dur = w._do_timeouts(handlers);
    for ev in events.iter() {
        match w.handlers.get(&ev.token()) {
            Some(eh) => { handlers.push(eh.handler.clone()); }
            None => ()
        };
    }
    if handlers.len() > 0 { return true; }
    if 0 == w.event_count && dur == None { return false; }
    debug!("Polling for [{:?}], [{}] events [{}] timeouts", &dur, w.event_count, w.next_timeouts.len());
    w.poll.poll(events, dur).unwrap();
    return true;
}

use std::thread;
use std::sync::mpsc;

pub struct ModuleCfg
{
    new_thread: bool,
    with_loop: Option<Core>
}
impl ModuleCfg
{
    pub fn new_thread(mut self, it: bool) -> Self { self.new_thread = it; self }
    pub fn with_loop(mut self, core: Option<Core>) -> Self { self.with_loop = core; self }

    pub fn run<T,U>(self, t:T, f: fn(&mut Scope<T>)->U) -> U where
        T: Send + 'static,
        U: Send + 'static
    {
        if self.new_thread {
            let (tx, rx) = mpsc::channel();
            thread::spawn(move|| {
                let (w, u) = new_core(t, f);
                tx.send(u).unwrap();
                loop_core(w);
            });
            return rx.recv().unwrap();
        }
        match self.with_loop {
            Some(core) => exec(&core, t, f),
            None => {
                let (w, u) = new_core(t, f);
                loop_core(w);
                u
            }
        }
    }
}
pub fn module() -> ModuleCfg {
    ModuleCfg {
        new_thread: false,
        with_loop: None
    }
}

fn new_core<T,U>(t:T, f: fn(&mut Scope<T>)->U) -> (Core, U)
{
    let core = Core {
        wp: Rc::new(RefCell::new(CorePvt {
            poll: mio::Poll::new().unwrap(),
            event_count: 0,
            next_token: 0,
            handlers: HashMap::new(),
            next_timeouts: BTreeMap::new(),
            now: SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
        }))
    };
    let u = exec(&core, t, f);
    (core, u)
}

fn loop_core(w: Core)
{
    let mut events = mio::Events::with_capacity(1024);
    let mut handlers: Vec<Callback<()>> = Vec::new();
    loop {
        debug!("Calling [{}] handlers", handlers.len());
        for h in &handlers { h.call(()); }
        if !_get_events(&w, &mut handlers, &mut events) { break; }
    }
}