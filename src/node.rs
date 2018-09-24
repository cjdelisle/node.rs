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
use mio_extras::channel::{ Sender, Receiver };
use std::io::ErrorKind;

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

struct CorePvt {
    handlers: HashMap<mio::Token, EventHandler>,
    poll: mio::Poll,
    next_timeouts: BTreeMap<Duration, Vec<TimerCb>>,
    event_count: usize,
    next_token: usize,
    now: Duration,
    callback_receiver: Receiver<CallbackEv>,

    next_callback_id: i32,
    callbacks_this_cycle: Vec<(i32, CallbackImpl)>
}

impl CorePvt {
    fn _do_timeouts(&mut self) -> Option<Duration>
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
                    el.cb.call(());
                    if el.interval {
                        let d = now + Duration::from_millis(el.millis);
                        self._schedule_timeout(el, d);
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
                self.event_count -= 1;
                match self.poll.deregister(&*handler.ev) {
                    Err(e) => {
                        if e.kind() == ErrorKind::NotFound { Ok(true) } else { Err(e) }
                    }
                    Ok(_) => Ok(true)
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

    fn next_callback_id(&mut self) -> i32 {
        self.next_callback_id
    }
    fn register_callback(&mut self, id: i32, cbi: CallbackImpl) {
        self.callbacks_this_cycle.push((id, cbi));
        self.next_callback_id += 1;
    }
}

#[derive(Clone)]
pub struct Core {
    wp: Rc<RefCell<CorePvt>>,
    pub callback_sender: Sender<CallbackEv>
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
        let out = self.wp.borrow_mut().register_event(ev, handler, ready, pollopt);
        debug!("Register {:?}", &out);
        out
    }
    pub fn deregister_event(&self, token: &mio::Token) -> io::Result<bool> {
        let out = self.wp.borrow_mut().deregister_event(token);
        debug!("Deregister {} {:?}", token.0, &out);
        out
    }
    pub fn set_timeout(&self, cb: Callback<()>, millis: u64, interval: bool) -> mio::Token {
        self.wp.borrow_mut().set_timeout(cb, millis, interval)
    }

    pub fn next_callback_id(&self) -> i32 {
        self.wp.borrow_mut().next_callback_id()
    }
    pub fn register_callback(&self, id: i32, cbi: CallbackImpl) {
        self.wp.borrow_mut().register_callback(id, cbi)
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
    fn module(&self) -> ModuleCfg { module().with_loop(self.core().clone()) }
    fn with_scope<X>(&self, x:X, f:fn(&mut SubScope<A,X>)) {
        let ss = Rc::new(RefCell::new(SubScope {
            p: self.as_rc(),
            a: x,
            s: Weak::new(),
            w: self.core().clone()
        }));
        ss.borrow_mut().s = Rc::downgrade(&ss);
        self.core().set_timeout(Callback::new(self.core(), (f,ss.clone()), |fw,_y|{
            fw.0(&mut *fw.1.borrow_mut())
        }), 0, false);
    }
    fn core(&self) -> &Core;
    fn as_rc(&self) -> Rc<RefCell<A>>;
    fn cb<X,F>(&self, f:F) -> Callback<X> where
        X: 'static + Send,
        F: 'static + Fn(&mut A, X);
}
impl<P,A> Loop<SubScope<P,A>> for SubScope<P,A> where P: Loop<P> {
    fn core(&self) -> &Core { &self.w }
    fn as_rc(&self) -> Rc<RefCell<Self>> { self.s.upgrade().unwrap() }
    fn cb<X,F>(&self, f:F) -> Callback<X> where
        X: 'static + Send,
        F: 'static + Fn(&mut SubScope<P,A>, X)
    {
        Callback::new(self.core(), rec!{ fun: f, l: self.as_rc() }, |ctx,x|{
            (ctx.fun)(&mut *ctx.l.borrow_mut(), x)
        })
    }
}
impl<A> Loop<Scope<A>> for Scope<A> {
    fn core(&self) -> &Core { &self.w }
    fn as_rc(&self) -> Rc<RefCell<Self>> { self.s.upgrade().unwrap() }
    fn cb<X,F>(&self, f:F) -> Callback<X> where
        X: 'static + Send,
        F: 'static + Fn(&mut Scope<A>, X)
    {
        Callback::new(self.core(), rec!{ fun: f, l: self.as_rc() }, |ctx,x|{
            (ctx.fun)(&mut *ctx.l.borrow_mut(), x)
        })
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Loop runner
///////////////////////////////////////////////////////////////////////////////////////////////////

const CB_RECV_TOKEN: mio::Token = mio::Token(100);
const FIRST_TOKEN:   usize      = 101;

//self.callback_by_id.insert(id, cbi);

fn _get_events(
    _w: &Core,
    calls: &mut Vec<CallbackEv>,
    events: &mut mio::Events,
    callback_by_id: &mut HashMap<i32, CallbackImpl>,
) -> bool
{
    let mut w = _w.wp.borrow_mut();
    for icb in w.callbacks_this_cycle.drain(..) { callback_by_id.insert(icb.0, icb.1); }
    let dur = w._do_timeouts();
    for ev in events.iter() {
        // If we get the CB_RECV_TOKEN, it doesn't matter, we're going to poll anyway
        match w.handlers.get(&ev.token()) {
            Some(eh) => { eh.handler.call(()); }
            None => ()
        };
    }
    events.clear();
    loop {
        match w.callback_receiver.try_recv() {
            Ok(cb) => { calls.push(cb) }
            _ => break,
        }
    }
    if calls.len() > 0 { return true; }
    callback_by_id.retain(|_k,v|{ v.canary.upgrade().is_some() });
    if 0 == w.event_count && dur == None && 0 == callback_by_id.len() { return false; }
    debug!("Polling for [{:?}], [{}] events [{}] timeouts [{}] callbacks",
        &dur, w.event_count, w.next_timeouts.len(), callback_by_id.len());
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
    pub fn with_loop(mut self, core: Core) -> Self { self.with_loop = Some(core); self }

    pub fn run<T,U>(self, t:T, f: fn(&mut Scope<T>)->U) -> U where
        T: Send + 'static,
        U: Send + 'static
    {
        match self.with_loop {
            Some(core) => {
                if self.new_thread {
                    let cb: Callback<thread::ThreadId> = Callback::new(&core, (), |_,tid| {
                        // This callback exists only to prevent the main parent loop from
                        // shutting down until all child loops have shutdown.
                        debug!("Thread ended {:?}", tid);
                    });

                    let (tx, rx) = mpsc::channel();
                    thread::spawn(move|| {
                        let tid = thread::current().id();
                        debug!("Thread started {:?}", tid);
                        let (w, u) = new_core(t, f, );
                        tx.send(u).unwrap();
                        loop_core(w);
                        cb.call(tid);
                    });
                    return rx.recv().unwrap();
                }
                exec(&core, t, f)
            }
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
    let (tx, rx) = mio_extras::channel::channel();
    let poll = mio::Poll::new().unwrap();
    poll.register(
        &rx,
        CB_RECV_TOKEN,
        mio::Ready::readable(),
        mio::PollOpt::edge()
    ).unwrap();

    let core = Core {
        callback_sender: tx,
        wp: Rc::new(RefCell::new(CorePvt {
            callback_receiver: rx,
            poll: poll,
            event_count: 0,
            next_token: FIRST_TOKEN,
            handlers: HashMap::new(),
            next_timeouts: BTreeMap::new(),
            now: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),

            next_callback_id: 0,
            callbacks_this_cycle: Vec::new()
        }))
    };
    let u = exec(&core, t, f);
    (core, u)
}

fn loop_core(w: Core)
{
    let mut events = mio::Events::with_capacity(1024);
    let mut calls: Vec<CallbackEv> = Vec::new();
    let mut callback_by_id: HashMap<i32, CallbackImpl> = HashMap::new();
    loop {
        debug!("Dispatching [{}] events", calls.len());
        for ev in calls.drain(..) {
            match ev {
                CallbackEv::Req(c) => {
                    let mut cbi = callback_by_id.get_mut(&c.canary.callback_id).unwrap();
                    (cbi.dispatch)(&mut cbi, c.x);
                }
            }
        }
        if !_get_events(&w, &mut calls, &mut events, &mut callback_by_id) { break; }
    }
}