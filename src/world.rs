#![allow(dead_code)]

extern crate mio;
extern crate mio_extras;

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

pub trait CallbackT {
    fn call(&self);
    fn clone(&self) -> Callback;
}
pub struct CallbackS<A> {
    a: Rc<RefCell<A>>,
    f: fn(&mut A)
}
impl<A> CallbackT for CallbackS<A> where
    A: 'static,
    CallbackS<A>: Sized
{
    fn call(&self) {
        let mut a = self.a.borrow_mut();
        (self.f)(&mut (*a));
    }
    fn clone(&self) -> Callback {
        Box::new(CallbackS { a: self.a.clone(), f: self.f })
    }
}
pub type Callback = Box<CallbackT>;

struct TimerCb {
    interval: bool,
    cb: Callback,
    millis: u64,
    id: mio::Token
}
impl fmt::Debug for TimerCb {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{ interval: {}, millis: {}, id: {:?} }}", self.interval, self.millis, self.id)
    }
}

struct EventHandler {
    handler: Callback,
    token: mio::Token,
    ev: Rc<mio::Evented>
}
impl fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{ handler: Fn(?)->?, token: {:?}, ev: <Object> }}", self.token)
    }
}

#[derive(Debug)]
pub struct World {
    handlers: HashMap<mio::Token, EventHandler>,
    poll: mio::Poll,
    next_timeouts: BTreeMap<Duration, Vec<TimerCb>>,
    event_count: usize,
    next_token: usize,
    now: Duration
}

impl World {
    fn _do_timeouts(&mut self, tos: &mut Vec<Callback>) -> Option<Duration>
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

    pub fn _register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: Callback,
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

    pub fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool> {
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

    pub fn _set_timeout(&mut self, cb: Callback, millis: u64, interval: bool) -> mio::Token {
        let id = mio::Token(self.next_token);
        self.next_token += 1;
        let tcb = TimerCb { cb: cb, interval, millis, id: id.clone() };
        debug!("_set_timeout in {:?}", Duration::from_millis(millis));
        let d = self.now + Duration::from_millis(millis);
        self._schedule_timeout(tcb, d);
        id
    }
}
/*
struct WorldWrap1<A> {
    a: Rc<RefCell<A>>,
    w: Rc<RefCell<World<WorldWrap1<A>>>>,
    f: fn(&mut WorldWrap1<A>, &mut A)
}

impl<A> WorldWrap1<A> {
    fn hdlr(ww: &mut WorldWrap1<A>) {
        (ww.f)(&mut ww, &mut *ww.a.borrow_mut());
    }
    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: fn(&mut WorldWrap1<A>, &mut A),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented, E: 'static
    {
        let hdlr = WorldWrap1::hdlr;
        let a = WorldWrap1 { f: handler, a: self.a, w: self.w };
        w.w._register_event(ev, (hdlr, a), ready, pollopt)
    }
    fn set_timeout(&mut self, cb: fn(&mut WorldWrap1<A>, &mut A), millis: u64, interval: bool) -> mio::Token
    {

    }
}*/
/*
pub struct _Loop<A>
{
    w: Rc<RefCell<Loop<_Loop<A>>>>,
}
*/
pub trait Loop<A> {
    fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool>;
    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: fn(&mut A),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented + 'static;
    fn set_timeout(&mut self, cb: fn(&mut A), millis: u64, interval: bool) -> mio::Token;
    fn scope(&mut self, f:fn(&mut RootScope<()>));
}
/*
impl<A> Loop<World> for World {
    fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool> { self.deregister_event(token) }
    fn register_event(
        &mut self,
        ev: Rc<mio::Evented>,
        handler: fn(&mut Loop<A>, &mut A),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
    {
        self._register_event(ev, handler, ready, pollopt)
    }

    fn set_timeout(&mut self, cb: fn(&mut Loop<A>, &mut A), millis: u64, interval: bool) -> mio::Token
    {
        self._set_timeout(cb, millis, interval)
    }
}*/

pub struct RootScope<A> where A: 'static {
    a: A,
    s: Weak<RefCell<RootScope<A>>>,
    w: Rc<RefCell<World>>
}
pub struct Scope<P,A> where P: Loop<P>, A: 'static, P: 'static {
    p: Rc<RefCell<P>>,
    a: A,
    s: Weak<RefCell<Scope<P,A>>>,
    w: Rc<RefCell<World>>
}

fn root_scope(w:&Rc<RefCell<World>>, f:fn(&mut RootScope<()>)) {
    let s = Rc::new(RefCell::new(RootScope {
        a: (),
        s: Weak::new(),
        w: w.clone()
    }));
    s.borrow_mut().s = Rc::downgrade(&s);
    f(&mut *s.borrow_mut());
}

fn child_scope<P,A,X>(
    s: &Weak<RefCell<Scope<P,A>>>,
    w: &Rc<RefCell<World>>,
    x:X,
    f:fn(&mut Scope<Scope<P,A>,X>)) where P: Loop<P>
{
    let s = Rc::new(RefCell::new(Scope {
        p: s.upgrade().unwrap(),
        a: x,
        s: Weak::new(),
        w: w.clone()
    }));
    s.borrow_mut().s = Rc::downgrade(&s);
    w.borrow_mut()._set_timeout(Box::new(CallbackS { a: s, f: f }), 0, false);
}

impl<P,A> Scope<P,A> where P: Loop<P> {
    pub fn p(&mut self) -> RefMut<P> {
        self.p.borrow_mut()
    }
    pub fn child_scope<X>(&mut self, x:X, f:fn(&mut Scope<Scope<P,A>,X>)) {
        let s = Rc::new(RefCell::new(Scope {
            p: self.s.upgrade().unwrap(),
            a: x,
            s: Weak::new(),
            w: self.w.clone()
        }));
        s.borrow_mut().s = Rc::downgrade(&s);
        self.w.borrow_mut()._set_timeout(Box::new(CallbackS { a: s, f: f }), 0, false);
    }
}
impl<A> RootScope<A> {
    pub fn child_scope<X>(&mut self, x:X, f:fn(&mut Scope<RootScope<A>,X>)) {
        let s = Rc::new(RefCell::new(Scope {
            p: self.s.upgrade().unwrap(),
            a: x,
            s: Weak::new(),
            w: self.w.clone()
        }));
        s.borrow_mut().s = Rc::downgrade(&s);
        self.w.borrow_mut()._set_timeout(Box::new(CallbackS { a: s, f: f }), 0, false);
    }
}


impl<P,A> Deref for Scope<P,A> where P: Loop<P> {
    type Target = A;
    fn deref(&self) -> &A { &self.a }
}
impl<P,A> DerefMut for Scope<P,A> where P: Loop<P> {
    fn deref_mut(&mut self) -> &mut A { &mut self.a }
}

impl<P,A> Loop<Scope<P,A>> for Scope<P,A> where P: Loop<P> {
    fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool> { self.w.borrow_mut().deregister_event(token) }
    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: fn(&mut Scope<P,A>),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented + 'static
    {
        let cb = Box::new(CallbackS { a: self.s.upgrade().unwrap(), f: handler });
        self.w.borrow_mut()._register_event(ev, cb, ready, pollopt)
    }
    fn set_timeout(&mut self, f: fn(&mut Scope<P,A>), millis: u64, interval: bool) -> mio::Token
    {
        let cb = Box::new(CallbackS { a: self.s.upgrade().unwrap(), f: f });
        self.w.borrow_mut()._set_timeout(cb, millis, interval)
    }
    fn scope(&mut self, f:fn(&mut RootScope<()>)) { root_scope(&self.w, f) }
}
impl<A> Loop<RootScope<A>> for RootScope<A> {
    fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool> { self.w.borrow_mut().deregister_event(token) }
    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: fn(&mut RootScope<A>),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented + 'static
    {
        let cb = Box::new(CallbackS { a: self.s.upgrade().unwrap(), f: handler });
        self.w.borrow_mut()._register_event(ev, cb, ready, pollopt)
    }
    fn set_timeout(&mut self, f: fn(&mut RootScope<A>), millis: u64, interval: bool) -> mio::Token
    {
        let cb = Box::new(CallbackS { a: self.s.upgrade().unwrap(), f: f });
        self.w.borrow_mut()._set_timeout(cb, millis, interval)
    }
    fn scope(&mut self, f:fn(&mut RootScope<()>)) { root_scope(&self.w, f) }
}


fn _get_events(_w: &Rc<RefCell<World>>, handlers: &mut Vec<Callback>, events: &mut mio::Events) -> bool
{
    handlers.clear();
    let mut w = _w.borrow_mut();
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

pub fn enter(cb: fn(&mut RootScope<()>))
{
    let w = Rc::new(RefCell::new(World {
        poll: mio::Poll::new().unwrap(),
        event_count: 0,
        next_token: 0,
        handlers: HashMap::new(),
        next_timeouts: BTreeMap::new(),
        now: SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
    }));

    root_scope(&w, cb);

    let mut events = mio::Events::with_capacity(1024);
    let mut handlers: Vec<Callback> = Vec::new();
    loop {
        debug!("Calling [{}] handlers", handlers.len());
        for h in &handlers { h.call(); }
        if !_get_events(&w, &mut handlers, &mut events) { break; }
    }
}

/*
pub struct Wrapper<P> {
    pub parent: P
}

impl<'a,X,Y,A> Loop<X,A> for Wrapper<&'a mut Y> where Y: Loop<X,A>
{
    fn _register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: (fn(&mut X, &mut A), A),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented, E: 'static
    {
        self.parent._register_event(ev, handler, ready, pollopt)
    }

    fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool>
    {
        self.parent.deregister_event(token)
    }

    fn _set_timeout(&mut self, cb: (fn(&mut X, &mut A), A), millis: u64, interval: bool) -> mio::Token
    {
        self.parent._set_timeout(cb, millis, interval)
    }


    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: fn(&mut X, &mut A),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented, E: 'static
    {
        self.parent._register_event(ev, (handler, self.a.clone()), ready, pollopt)
    }
    fn set_timeout(&mut self, cb: fn(&mut X, &mut A), millis: u64, interval: bool) -> mio::Token
    {
        self.parent._set_timeout((cb, self.a.clone()), millis, interval)
    }
}




/*
struct Wrapper<A,O> {
    parent: World<(A,)>,
    obj: O
}
impl<A,O> Wrapper<A,O> {
    fn wrap(f: fn(&mut World<(A,O)>, &mut (A,O))) -> fn(&mut World<(A,)>, &mut (A))
    {
        |w|{  }
    }
}
impl<A,O> Loop<World<(A,O)>,(A,O)> for Wrapper<A,O> {
    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: fn(&mut World<(A,O)>, &mut (A,O)),
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented, E: 'static
    {
        self.parent.register_event(ev, handler, ready, pollopt)
    }

    fn deregister_event(&mut self, token: &mio::Token) -> io::Result<bool>
    {
        self.parent.deregister_event(token)
    }

    fn set_timeout(&mut self, cb: fn(&mut World<(A,O)>, &mut (A,O)), millis: u64, interval: bool) -> mio::Token
    {
        self.parent.set_timeout(cb, millis, interval)
    }
}

/*
trait WithScope<A,C> {
    fn with_scope(&mut self, a:A) -> Option<C>;
}

struct Scope1<X,A> {
    parent: X,
    obj: A
}
impl<X,Y,A> Loop<Box<Fn(&mut Y, &mut A)>> for Scope1<X,A>
    where X: Loop<Box<Fn(&mut Y)>>
{
    fn register_event<E>(
        &mut self,
        ev: Rc<E>,
        handler: Box<Fn(&mut Y, &mut A)>,
        ready: mio::Ready,
        pollopt: mio::PollOpt) -> io::Result<mio::Token>
        where E: mio::Evented, E: 'static
    {
        Ok(mio::Token(0))
    }

    fn deregister_event(&mut self, token: mio::Token) -> io::Result<bool>
    {
        self.parent.deregister_event(token)
    }

    fn set_timeout(&mut self, cb: Box<Fn(&mut Y, &mut A)>, millis: u64, interval: bool) -> mio::Token
    {
        mio::Token(0)
    }
}
impl <X,Y,A,C> WithScope<A,C> for Scope1<X,A> where
        X: Loop<Box<Fn(&mut Y)>>,
        C: Loop<Box<Fn(&mut Y, &mut A)>>
{
    fn with_scope(&mut self, a:A) -> Option<C>
    {
        None
    }
}

/*
pub struct Scope<'a,P,O> where P: 'a {
    parent: &'a mut P,
    obj: O
}

pub trait Scope2<A,B> {
    fn call(&mut self, &Fn(&mut A, &mut B));
    //fn obj(&mut self) -> &mut B;
}
impl<'a,A,B> Scope2<A,B> for Scope<'a,A,B> where A: 'a {
    fn call(&mut self, f: &Fn(&mut A, &mut B)) { f(&mut self.a, &mut self.b) }
    //fn obj(&mut self) -> &mut X { &mut self.obj }
}

*/

/*
macro_rules! mk_scope {
    ($name:ident ; $tup:expr ; [$($type:ident),*] ; [$($arg:tt:$argt:tt),*] ; $call:expr) => {
        trait $name<$($type,)*> {

        }
    }
}
mk_scope!(Scope1 ; (A,) ; [A] ; [a:A] ; (a,));
mk_scope!(Scope2 ; (A,B) ; [A,B] ; [a:A,b:B] ; (a,b));
mk_scope!(Scope3 ; (A,B,C) ; [A,B,C] ; [a:A,b:B,c:C] ; (a,b,c));
mk_scope!(Scope4 ; (A,B,C,D) ; [A,B,C,D] ; [a:A,b:B,c:C,d:D] ; (a,b,c,d));
mk_scope!(Scope5 ; (A,B,C,D,E) ; [A,B,C,D,E] ; [a:A,b:B,c:C,d:D,e:E] ; (a,b,c,d,e));
mk_scope!(Scope6 ; (A,B,C,D,E,F) ; [A,B,C,D,E,F] ; [a:A,b:B,c:C,d:D,e:E,f:F] ; (a,b,c,d,e,f));
*/

/*

use mio::net::UdpSocket;
use std::net::SocketAddr;



pub struct UDPSocket_pvt {
    sock: UdpSocket,
    on_msg_cb: Cb2<Vec<u8>, SocketAddr>
}
pub struct UDPSocket_ {
    pvt: RefCell<UDPSocket_pvt>,
}
impl UDPSocket_ {
    pub fn on_message(&self, cb: Cb2<Vec<u8>, SocketAddr>) {
        self.pvt.borrow_mut().on_msg_cb = cb;
    }
}
type UDPSocket = Rc<UDPSocket_>;
fn dummy_on_msg(_v:Vec<u8>, _s:SocketAddr) { }
pub fn udp_create(wp: Ref<World>, udp: UdpSocket) -> io::Result<UDPSocket>
{
    let out = UDPSocket_pvt {
        sock: udp,
        on_msg_cb: dummy_on_msg.fwrap()
    };
    //wp.poll.register(&out.sock, SOCK_SEND, mio::Ready::writable(), mio::PollOpt::edge())?;
    //wp.poll.register(&out.sock, SOCK_RECV, mio::Ready::readable(), mio::PollOpt::edge())?;
    let s = Rc::new(UDPSocket_ {
        pvt: RefCell::new(out)
    });


    Ok(s)
}
*/

*/
*/
*/