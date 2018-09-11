extern crate mio;
extern crate mio_extras;

mod world;
use world::*;

struct X {
    i: u32,
}
impl X {
    fn hi(&self) {
        println!("Hello {}", self.i);
    }
}

fn test()
{
    println!("Plain old function");
}
struct ABC {
    i: i32,
    x: mio::Token
}

fn main()
{
    world::enter(|s| {
        println!("here0");
        s.set_timeout(|_s|{ println!("Hello1"); }, 100, false);

        s.child_scope(ABC {
            i: 0,
            x: mio::Token(0)
        },|s|{
            s.i = 300;
            s.set_timeout(|s|{
                println!("Hello world! {}", s.i);
                s.i += 1;
            }, 150, false);
            s.set_timeout(|s|{
                println!("Hello again! {}", s.i);
                s.i += 1;
            }, 160, false);

            s.child_scope(X {
                i: 33
            },|s|{
                s.set_timeout(|s|{ s.hi() }, 500, false);
                println!("s.i = {}", s.i);
                println!("s.p().i = {}", s.p().i);
            });

            s.x = s.set_timeout(|_s|{ test(); }, 50, true);
            s.set_timeout(|s|{
                let t = s.x;
                match s.deregister_event(&t) { _=>() }
            }, 1000, false);
        });
    });
}