use std::rc::Rc;
use std::cell::{ RefCell, RefMut };
use std::collections::VecDeque;
use std::net::SocketAddr;
use mio::{ Ready, PollOpt, Token};
use bytes::{ BytesMut, BufMut };
use mio;
use std;
use std::io;
use std::ops::Deref;
use std::io::Result;

use callback::Callback;

use node::Loop;

struct UdpSocket_pvt {
    s: Rc<mio::net::UdpSocket>,
    can_send_event: Token,
    can_send: bool,
    queue: VecDeque<Message>
}
pub struct UdpSocket {
    pvt: Rc<RefCell<UdpSocket_pvt>>,
    s: Rc<mio::net::UdpSocket>,
}
impl UdpSocket {
    fn new(s: mio::net::UdpSocket) -> UdpSocket {
        let rcs = Rc::new(s);
        UdpSocket {
            s: rcs.clone(),
            pvt: Rc::new(RefCell::new(UdpSocket_pvt {
                s: rcs,
                can_send_event: Token(0),
                can_send: false,
                queue: VecDeque::new(),
            }))
        }
    }
}
impl Deref for UdpSocket {
    type Target = mio::net::UdpSocket;
    fn deref(&self) -> &Self::Target { &self.s }
}

fn send_messages(pvt: &mut RefMut<UdpSocket_pvt>) {
    loop {
        match pvt.queue.pop_front() {
            None => { return; }
            Some(m) => match pvt.s.send_to(&m.buf, &m.sa) {
                Ok(size) => { assert!(size == m.buf.len()); },
                Err(e) => {
                    println!("err {:?}", e);
                    pvt.can_send = false;
                    return;
                }
            }
        }
    }
}

impl UdpSocket {
    pub fn bind(addr: &SocketAddr) -> Result<UdpSocket> {
        let s = mio::net::UdpSocket::bind(addr)?;
        Ok(UdpSocket::new(s))
    }
    pub fn from_socket(socket: std::net::UdpSocket) -> Result<UdpSocket> {
        let s = mio::net::UdpSocket::from_socket(socket)?;
        Ok(UdpSocket::new(s))
    }
    pub fn on_message<L:Loop<L>,F:'static+Fn(&mut L,Message)>(&self, l:&mut L, f:F) -> io::Result<Token> {
        let c = l.core();
        c.register_event(self.s.clone(), Callback::new(c, (l.as_rc(),self.s.clone(),f), |ctx,_|{
            let mut l = ctx.0.borrow_mut();
            loop {
                let mut buf = BytesMut::with_capacity(2048);
                let ret = unsafe { ctx.1.recv_from(buf.bytes_mut()) };
                match ret {
                    Ok((count, sa)) => {
                        unsafe { buf.advance_mut(count); }
                        (ctx.2)(&mut *l, Message { buf, sa });
                    },
                    Err(e) => {
                        println!("error from cb {:?}", e);
                        break;
                    }
                }
            }
        }), Ready::readable(), PollOpt::edge())
    }
    pub fn send_message<L:Loop<L>,F:Fn(L)>(&self, l:&mut L, m: Message, f:F) {
        let mut pvt = self.pvt.borrow_mut();
        pvt.queue.push_back(m);
        if pvt.can_send {
            send_messages(&mut pvt);
        } else if pvt.can_send_event == Token(0) {
            let cb = Callback::new(l.core(), self.pvt.clone(), |pvt_,_|{
                let mut pvt = pvt_.borrow_mut();
                pvt.can_send = true;
                send_messages(&mut pvt);
            });
            pvt.can_send_event = l.core().register_event(
                self.s.clone(), cb, Ready::writable(), PollOpt::edge()).unwrap();
        }
    }
}


pub struct Message {
    sa: SocketAddr,
    buf: BytesMut
}



