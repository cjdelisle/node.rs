use mio::Token;
use node::Loop;
use callback::cb0;

pub fn set_timeout<L,F>(l:&L, cb:F, millis: u64) -> Token where
    L: Loop<L>,
    F: 'static + Fn(&mut L)
{
    l.core().set_timeout(cb0(l,cb), millis, false)
}
pub fn set_interval<L,F>(l:&L, cb:F, millis: u64) -> Token where
    L: Loop<L>,
    F: 'static + Fn(&mut L)
{
    l.core().set_timeout(cb0(l,cb), millis, true)
}
pub fn clear_timeout<L:Loop<L>>(l:&L, t: Token) -> bool {
    match l.core().deregister_event(&t) { Ok(r) => r, Err(_) => false }
}