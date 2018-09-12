use std::cell::RefCell;
use std::rc::Rc;

use node::Loop;

pub trait CallbackT<X> where X: Send {
    fn call(&self, x:X);
    fn clone(&self) -> Callback<X>;
}
pub struct CallbackS<A,X> {
    pub a: Rc<RefCell<A>>,
    pub f: fn(&mut A, X)
}
pub struct CallbackS0<A> {
    pub a: Rc<RefCell<A>>,
    pub f: fn(&mut A)
}
impl<A> CallbackT<()> for CallbackS0<A>
    where A: 'static
{
    fn call(&self, _x:()) {
        let mut a = self.a.borrow_mut();
        (self.f)(&mut (*a));
    }
    fn clone(&self) -> Callback<()> {
        Box::new(CallbackS0 { a: self.a.clone(), f: self.f })
    }
}
impl<A,X> CallbackT<X> for CallbackS<A,X> where
    A: 'static,
    X: 'static + Send
{
    fn call(&self, x:X) {
        let mut a = self.a.borrow_mut();
        (self.f)(&mut (*a), x);
    }
    fn clone(&self) -> Callback<X> {
        Box::new(CallbackS { a: self.a.clone(), f: self.f })
    }
}
pub type Callback<X> = Box<CallbackT<X>>;

pub fn cb<X,Y>(x:&X, f:fn(&mut X, y:Y)) -> Callback<Y> where
    X: Loop<X>,
    Y: 'static + Send
{
    Box::new(CallbackS { a: x.as_rc(), f })
}

pub fn cb0<X>(x:&X, f:fn(&mut X)) -> Callback<()> where
    X: Loop<X>
{
    Box::new(CallbackS0 { a: x.as_rc(), f })
}