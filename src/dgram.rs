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
use std::net::{ IpAddr, Ipv4Addr, Ipv6Addr };
use std::str::FromStr;
use std::io::ErrorKind;

use callback::Callback;

use node::{ Loop, Core };

fn send_messages(pvt: &mut RefMut<SockPvt>) {
    loop {
        let st_ = pvt.send_queue.pop_front();
        if st_.is_none() { return; }
        let st = st_.unwrap();
        match pvt.s.as_ref().unwrap().send_to(&st.msg.buf, &st.msg.sa) {
            Ok(size) => {
                assert!(size == st.msg.buf.len());
                st.cb.call(Ok(()));
            },
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    pvt.can_send = false;
                    return;
                }
                st.cb.call(Err(e));
                //println!("err {:?}", e);
            }
        }
    }
}


struct SendTo {
    msg: Message,
    cb: Callback<io::Result<()>>
}
struct SockPvt {
    s: Option<Rc<mio::net::UdpSocket>>,

    can_send_event: Token,
    can_send: bool,
    can_recv_event: Token,

    send_queue: VecDeque<SendTo>,
    on_message: Vec<Callback<Message>>,
    core: Option<Core>,

    closed: bool
}

pub struct SockBuilder {
    pvt: Rc<RefCell<SockPvt>>,
    af: Af,
}

fn try_setup_core(pvt: &mut RefMut<SockPvt>, rc: &Rc<RefCell<SockPvt>>) {
    // can't do anything until we have the socket and core
    if pvt.s.is_none() || pvt.core.is_none() { return; }

    // done already
    if pvt.can_recv_event != Token(0) { return; }

    let c = pvt.core.as_ref().unwrap().clone();
    let s = pvt.s.as_ref().unwrap().clone();

    let can_send_cb = Callback::new(&c, rc.clone(), |pvt_,_|{
        let mut pvt = pvt_.borrow_mut();
        pvt.can_send = true;
        send_messages(&mut pvt);
    });
    pvt.can_send_event =
        c.register_event(s.clone(), can_send_cb, Ready::writable(), PollOpt::edge()).unwrap();

    let can_recv_cb = Callback::new(&c, rc.clone(), |pvt_,_| {
        let pvt = pvt_.borrow_mut();
        let s = pvt.s.as_ref().unwrap();
        loop {
            let mut buf = BytesMut::with_capacity(2048);
            let ret = unsafe { s.recv_from(buf.bytes_mut()) };
            match ret {
                Ok((count, sa)) => {
                    unsafe { buf.advance_mut(count); }
                    if pvt.on_message.len() == 1 {
                        pvt.on_message[0].call(Message { buf, sa });
                    } else {
                        for cb in &pvt.on_message {
                            cb.call(Message { buf: buf.clone(), sa: sa.clone() });
                        }
                    }
                },
                Err(e) => {
                    if e.kind() != ErrorKind::WouldBlock {
                        error!("dgram can_recv_cb {:?}", &e);
                    }
                    break;
                }
            }
        }
    });
    pvt.can_recv_event =
        c.register_event(s, can_recv_cb, Ready::readable(), PollOpt::edge()).unwrap();

    // you don't get a can_send event until you clog up the buffer first, so better send now.
    pvt.can_send = true;
    send_messages(&mut *pvt);
}

pub struct Sock {
    bldr: SockBuilder,
    s: Rc<mio::net::UdpSocket>,
}
impl Deref for Sock {
    type Target = mio::net::UdpSocket;
    fn deref(&self) -> &Self::Target { &self.s }
}
impl Sock {
    pub fn on_message<L:Loop<L>,F:'static+Fn(&mut L,Message)>(&self, l:&L, f:F) -> &Sock {
        self.bldr.on_message(l, f);
        self
    }
    pub fn send_to<L,F,A,B>(&self, l:&L, bm: B, a:A, f:F) -> &Sock where
        L: Loop<L>,
        F: 'static + Fn(&mut L, io::Result<()>),
        A: AddrLike,
        B: Into<BytesMut>
    {
        self.bldr.send_to(l, bm, a, f);
        self
    }
    pub fn close(&self) {
        debug!("close()");
        let mut pvt = self.bldr.pvt.borrow_mut();
        if pvt.s.is_none() { return; }
        if pvt.core.is_none() { return; }
        if pvt.closed { return; }
        pvt.closed = true;
        pvt.on_message.clear();
        pvt.send_queue.clear();
        let c = pvt.core.as_ref().unwrap();
        match c.deregister_event(&pvt.can_send_event) {_=>()}
        match c.deregister_event(&pvt.can_recv_event) {_=>()}
    }
}

impl SockBuilder {
    pub fn on_message<L:Loop<L>,F:'static+Fn(&mut L,Message)>(&self, l:&L, f:F) -> &SockBuilder {
        let c = l.core();
        let mut pvt = self.pvt.borrow_mut();
        if pvt.closed { error!("on_message() Socket already closed"); return self; }
        pvt.on_message.push(Callback::new(c, rec!{ l: l.as_rc(), f:f }, |ctx,msg|{
            (ctx.f)(&mut *ctx.l.borrow_mut(), msg);
        }));
        if pvt.core.is_none() { pvt.core = Some(c.clone()); }
        try_setup_core(&mut pvt, &self.pvt);
        self
    }
    pub fn send_to<L,F,A,B>(&self, l:&L, bm: B, a:A, f:F) -> &SockBuilder where
        L: Loop<L>,
        F: 'static + Fn(&mut L, io::Result<()>),
        A: AddrLike,
        B: Into<BytesMut>
    {
        let addr_str = a.to_string();
        let sa = match a.as_sockaddr(self.af) {
            Ok(sa) => sa,
            Err(_e) => {
                error!("send_to() Failed to parse address {}", &addr_str);
                return self;
            }
        };
        let c = l.core();
        let mut pvt = self.pvt.borrow_mut();
        if pvt.closed { error!("send_to() Socket already closed"); return self; }
        pvt.send_queue.push_back(SendTo {
            msg: bm.to_msg(sa),
            cb: Callback::new(c, rec!{ l: l.as_rc(), f:f }, |ctx,res|{
                (ctx.f)(&mut *ctx.l.borrow_mut(), res);
            })
        });
        if pvt.can_send {
            send_messages(&mut pvt);
        } else {
            if pvt.core.is_none() { pvt.core = Some(c.clone()); }
            try_setup_core(&mut pvt, &self.pvt);
        }
        self
    }
    pub fn _bind(self, addr: &SocketAddr) -> io::Result<Sock> {
        let s = mio::net::UdpSocket::bind(addr)?;
        let rc = Rc::new(s);
        {
            let mut pvt = self.pvt.borrow_mut();
            pvt.s = Some(rc.clone());
            try_setup_core(&mut pvt, &self.pvt);
        }
        Ok(Sock { s: rc, bldr: self })
    }
    pub fn bind<T:AddrLike>(self, t:T) -> io::Result<Sock> {
        let af = self.af;
        match t.as_sockaddr(af) {
            Ok(sa) => self._bind(&sa),
            Err(e) => Err(e)
        }
    }
}

pub fn create_socket(afs: &'static str) -> io::Result<SockBuilder> {
    let af = match afs {
        "udp4" => Af::Inet,
        "udp6" => Af::Inet6,
        _ => { return Err(io::Error::new(ErrorKind::InvalidInput, "expecting udp4 or udp6")); }
    };
    Ok(SockBuilder {
        af,
        pvt: Rc::new(RefCell::new(SockPvt {
            s: None,

            can_send_event: Token(0),
            can_send: false,
            can_recv_event: Token(0),

            send_queue: VecDeque::new(),
            on_message: Vec::new(),
            core: None,

            closed: false
        }))
    })
}

///

#[derive(Clone, Copy)]
pub enum Af { Inet, Inet6 }

pub trait AddrLike {
    fn as_sockaddr(self, af: Af) -> io::Result<SocketAddr>;
    fn to_string(&self) -> String;
}
impl AddrLike for u16 {
    fn to_string(&self) -> String { <Self as std::string::ToString>::to_string(self) }
    fn as_sockaddr(self, af: Af) -> io::Result<SocketAddr> {
        Ok(match af {
            Af::Inet => SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0,0,0,0)), self),
            Af::Inet6 => SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0,0,0,0,0,0,0,0)), self)
        })
    }
}
impl AddrLike for &'static str {
    fn to_string(&self) -> String { <Self as std::string::ToString>::to_string(self) }
    fn as_sockaddr(self, af: Af) -> io::Result<SocketAddr> { (0, self).as_sockaddr(af) }
}
impl AddrLike for (u16, &'static str) {
    fn to_string(&self) -> String { format!("({},{})", self.0, self.1) }
    fn as_sockaddr(self, _af: Af) -> io::Result<SocketAddr> {
        let (port, addr) = self;
        let res = match IpAddr::from_str(addr) {
            Ok(addr) => addr,
            Err(e) => { return Result::Err(io::Error::new(ErrorKind::InvalidInput, e)); }
        };
        Ok(SocketAddr::new(res, port))
    }
}


pub struct Message {
    pub sa: SocketAddr,
    pub buf: BytesMut
}
pub trait MsgLike {
    fn to_msg(self, sa: SocketAddr) -> Message;
}
impl<T> MsgLike for T where T: Into<BytesMut> {
    fn to_msg(self, sa: SocketAddr) -> Message {
        let buf = self.into();
        Message { sa, buf }
    }
}