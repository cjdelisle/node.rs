//use wrapper::*;

extern crate mio;
extern crate mio_extras;

mod wrapper;
mod world;

use wrapper::*;
use world::World;

fn loop_main(w: &World) {
    println!("here1");
    w.set_timeout(Callback0::mk(||{
        println!("Hello world!");
    }), 2000);
    println!("here2");
    w.set_timeout(Callback0::mk(||{
        println!("I was first");
    }), 1000);
}

fn main()
{
    World::enter(loop_main);
}