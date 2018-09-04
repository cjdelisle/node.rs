#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(non_camel_case_types)]

extern crate mio;
extern crate mio_extras;

use wrapper::*;

use mio_extras::timer::Timer;
use mio_extras::timer::Timeout;
//use mio::udp::UdpSocket;
use std::time::Duration;

use std::rc::Rc;
use std::cell::RefCell;
use std::cell::RefMut;

const TIMER: mio::Token = mio::Token(0);


pub struct TimerCb_pvt {
    to: Option<Timeout>
}
pub struct _TimerCb {
    pvt: RefCell<TimerCb_pvt>,
    interval: bool,
    cb: Callback0,
    millis: u64
}
type TimerCb = Rc<_TimerCb>;





pub struct World_pvt {
    poll: mio::Poll,
    timer: Timer<TimerCb>,
    event_count: i32
}
pub struct World {
    pvt: RefCell<World_pvt>
}
fn set_timeout(mut wp: RefMut<World_pvt>, cb: Callback0, millis: u64, interval: bool) -> TimerCb {
    let tcb = Rc::new(_TimerCb { cb, interval, pvt: RefCell::new(TimerCb_pvt { to: None }), millis });
    let to = wp.timer.set_timeout(Duration::from_millis(millis), tcb.clone());
    tcb.pvt.borrow_mut().to = Some(to);
    wp.event_count += 1;
    tcb
}
fn init(wp: RefMut<World_pvt>)
{
    wp.poll.register(&wp.timer, TIMER, mio::Ready::all(), mio::PollOpt::edge()).unwrap();
}
fn poll(wp: RefMut<World_pvt>, ev: &mut mio::event::Events) -> bool
{
    if 0 == wp.event_count { return false; }
    wp.poll.poll(ev, None).unwrap();
    return true;
}
fn handle_timeouts(wp: &mut RefMut<World_pvt>) -> Vec<TimerCb>
{
    let mut callbacks = Vec::new();
    loop {
        match wp.timer.poll() {
            None => break,
            Some(to) => {
                // this is bizarre, if you inline these clones, you get errors but if you
                // put them before the first usage, everything works fine.
                let t = to.clone();
                let t2 = to.clone();
                let mut to_pvt = t.pvt.borrow_mut();
                if to.interval {
                    to_pvt.to = Some(wp.timer.set_timeout(Duration::from_millis(to.millis), t2));
                } else {
                    wp.event_count -= 1;
                }
                callbacks.push(to);
            }
        }
    }
    callbacks
}
fn enter(cb: fn(w:&World))
{
    let w = World {
        pvt: RefCell::new(World_pvt {
            poll: mio::Poll::new().unwrap(),
            timer: Timer::default(),
            event_count: 0
        })
    };
    w._init();

    cb(&w);

    loop {
        let mut events = mio::Events::with_capacity(1024);
        if !w._poll(&mut events) { break; }
        for ev in events {
            match ev.token() {
                TIMER => for tp in w._handle_timeouts().iter() { tp.cb.cb(); }
                _ => {
                    println!("event with unexpected token {:?}", ev.token());
                }
            }
        }
    }
}
impl World {
    fn _handle_timeouts(&self) -> Vec<TimerCb> {
        handle_timeouts(&mut self.pvt.borrow_mut())
    }
    fn _init(&self) {
        init(self.pvt.borrow_mut())
    }
    fn _poll(&self, ev: &mut mio::event::Events) -> bool {
        poll(self.pvt.borrow_mut(), ev)
    }

    pub fn enter(cb: fn(w:&World)) {
        enter(cb);
    }
    pub fn set_timeout(&self, cb: Callback0, millis: u64) -> TimerCb {
        set_timeout(self.pvt.borrow_mut(), cb, millis, false)
    }
    pub fn set_interval(&self, cb: Callback0, millis: u64) -> TimerCb {
        set_timeout(self.pvt.borrow_mut(), cb, millis, true)
    }
    /*pub fn clear_timeout(&self, to: &TimerCb) {
        let mut pvt = self.pvt.borrow_mut();
        pvt.timer.cancel_timeout(to._pvt().to.as_ref().unwrap());
    }
    pub fn clear_interval(&self, to: &TimerCb) { self.clear_timeout(to) }*/
}