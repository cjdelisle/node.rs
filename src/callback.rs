//use tooples::*;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Weak;
use std::any::Any;
use mio_extras::channel::Sender;

use node::{ Core, Loop };


pub struct CallbackReq {
    pub x: Box<Any + Send>,
    pub canary: Arc<Canary>
}
pub enum CallbackEv {
    Req(CallbackReq)
}

pub struct Canary {
    pub callback_id: i32,
}



pub struct CallbackImpl {
    w:Box<Any>,
    f:Box<Any>,
    pub dispatch: fn(&mut CallbackImpl, Box<Any + Send>),
    pub canary: Weak<Canary>
}
fn dispatch<W,X,F>(cbi: &mut CallbackImpl, mut x: Box<Any + Send>) where
    F: 'static + Fn(&mut W,X),
    W: 'static,
    X: 'static
{
    let w: &mut W = cbi.w.downcast_mut().unwrap();
    let optx: &mut Option<X> = x.downcast_mut().unwrap();
    let x = optx.take().unwrap();
    let f: &Fn(&mut W,X) = cbi.f.downcast_ref::<F>().unwrap();
    f(w,x);
}
impl CallbackImpl {
    fn new<W,X,F>(w:W, f:F, c:&Arc<Canary>) -> CallbackImpl where
        F: 'static + Fn(&mut W,X),
        W: 'static,
        X: 'static
    {
        CallbackImpl {
            w:Box::new(w),
            f:Box::new(f),
            dispatch: dispatch::<W,X,F>,
            canary: Arc::downgrade(c)
        }
    }
}

pub struct Callback<X> where X: Send {
    canary: Arc<Canary>,
    sender: Sender<CallbackEv>,
    _x: PhantomData<X>
}

impl<X> Callback<X> where X: Send + 'static {
    pub fn new<W,F>(c:&Core, w:W, f:F) -> Callback<X> where
        F: 'static + Fn(&mut W,X),
        W: 'static,
    {
        let id = c.next_callback_id();
        let canary = Arc::new(Canary {
            callback_id: id,
        });
        c.register_callback(id, CallbackImpl::new(w, f, &canary));
        Callback { canary, _x: PhantomData, sender: c.callback_sender.clone() }
    }
    pub fn call(&self, x:X) {
        self.sender.send(CallbackEv::Req(CallbackReq {
            x: Box::new(Some(x)),
            canary: self.canary.clone()
        }));
    }
}

pub fn cb0<W,F>(w:&W, f:F) -> Callback<()> where
    W: Loop<W>,
    F: 'static + Fn(&mut W)
{
    Callback::new(w.core(), (f,w.as_rc()), |fw,_y|{ fw.0(&mut *fw.1.borrow_mut()) })
}
pub fn cb<W,X,F>(w:&W, f:F) -> Callback<X> where
    W: Loop<W>,
    X: 'static + Send,
    F: 'static + Fn(&mut W, X)
{
    Callback::new(w.core(), (f,w.as_rc()), |fw,x|{ fw.0(&mut *fw.1.borrow_mut(), x) })
}